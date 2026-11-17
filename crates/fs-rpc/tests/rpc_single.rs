//! `ensure_single` against real hanging servers: a child process (this test binary again, or a copy of it) holds the socket and does
//! not answer.
//!
//! Pattern: Table of scenarios, one child per test, serial (`ONE_AT_A_TIME`). Rules and their tests (the mutant that removes the rule
//! turns the test red; the pure rules have unit tests in `src/single/*_tests.rs`):
//! R1 own program, hangs, heartbeat with a hook -> saved, then ended, replaced (`a_hanging_server_with_a_heartbeat_is_saved_then_ended`);
//! R2 hanging without heartbeat -> ended by SIGTERM, nothing saved (`..._without_a_heartbeat_is_ended_by_sigterm`);
//! R3 busy and moving -> never ended, `server-busy` (`a_busy_server_that_moves_is_never_ended`);
//! R4 busy, number stands still -> hanging -> ended (`a_busy_server_that_stands_still_is_ended`);
//! R5 another program under the same name (a copy of the file) -> reported at once, never signalled, nothing saved
//! (`another_program_is_only_reported`); R6 the heartbeat answers but the work thread's tick stands still -> hanging -> ended
//! (`a_blocked_work_thread_behind_a_live_heartbeat_is_ended`); R7 no heartbeat channel and SIGTERM ignored -> never killed, `no-control`
//! (`a_program_without_a_channel_that_ignores_sigterm_is_never_killed`); R8 every wait has its limit
//! (`the_whole_call_has_a_limit`). Mutants outside the table: M1 no limit on the whole call; M2 SIGKILL for R7; M3 `Busy` counts as
//! hanging; M4 the copy of the program is taken for this program; M5 the heartbeat answer counts as health although the tick stands still.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use fs_interfaces::common_capnp::blob;
use fs_rpc::{
    Expected, Heartbeat, RestartTimes, RpcError, RpcErrorCode, Saved, SingleError, SingleTimes,
    bind_socket, ensure_single, stop_server,
};

mod handshake_support;
use handshake_support::*;

/// A child that is gone at the end of the test whatever happens.
struct Guard(Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The hanging server (in the child): holds the socket, never answers `Control`. FS_SINGLE_MODE: `silent` (no heartbeat),
/// `silent-beat` (heartbeat with a hook, work thread asleep, no tick), `busy-moving`, `busy-stalled`, `stuck-worker` (one tick, then
/// blocked), `ignore-term` (no heartbeat, SIGTERM ignored).
pub fn hanging_server_main() {
    let (Some(mode), Some(socket)) = (
        std::env::var_os("FS_SINGLE_MODE"),
        std::env::var_os("FS_SINGLE_SOCKET"),
    ) else {
        return;
    };
    let mode = mode.to_string_lossy().into_owned();
    let runtime = runtime();
    let _bound = tokio::task::LocalSet::new().block_on(&runtime, async {
        bind_socket(Path::new(&socket)).await.unwrap()
    });
    let _ignore = (mode == "ignore-term").then(|| {
        runtime.block_on(async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap()
        })
    });
    let beat = (mode != "silent" && mode != "ignore-term")
        .then(|| Heartbeat::start(Path::new(&socket)).unwrap());
    if let (Some(hb), Some(marker)) = (&beat, std::env::var_os("FS_SINGLE_MARKER")) {
        hb.on_save(move || {
            std::fs::write(&marker, "saved").unwrap();
            Saved::Saved
        });
    }
    let mut n = 0u64;
    loop {
        match (&beat, mode.as_str()) {
            (Some(hb), "busy-moving") => {
                n += 1;
                hb.busy("importing", n);
            }
            (Some(hb), "busy-stalled") => hb.busy("importing", 7),
            (Some(hb), "stuck-worker") if n == 0 => {
                n = 1;
                hb.tick();
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[test]
fn helper_hanging_server() {
    hanging_server_main();
}

fn spawn_child(exe: &Path, socket: &Path, mode: &str, marker: Option<&Path>) -> Guard {
    let mut command = Command::new(exe);
    command
        .args(["--exact", "helper_hanging_server", "--nocapture"])
        .env("FS_SINGLE_MODE", mode)
        .env("FS_SINGLE_SOCKET", socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(marker) = marker {
        command.env("FS_SINGLE_MARKER", marker);
    }
    let mut child = None;
    for _ in 0..100 {
        match command.spawn() {
            Ok(c) => {
                child = Some(c);
                break;
            }
            Err(e) if e.raw_os_error() == Some(26) => std::thread::sleep(Duration::from_millis(40)), // ETXTBSY
            Err(e) => panic!("{e}"),
        }
    }
    let guard = Guard(child.expect("started"));
    let began = Instant::now();
    while connect_once(socket) {
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "the child did not bind"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    guard
}

fn times() -> SingleTimes {
    let ms = Duration::from_millis;
    let mut t = SingleTimes::default();
    t.handshake = RestartTimes {
        call: ms(300),
        stop: ms(2000),
        start: ms(3000),
    };
    t.hang.ping = ms(100);
    t.hang.no_beat = ms(500);
    t.hang.no_progress = ms(700);
    t.hang.no_tick = ms(500);
    t.save = ms(2000);
    t.term = ms(1500);
    t.kill = ms(1500);
    t.overall = ms(20_000);
    t
}

fn expected() -> Expected {
    Expected::new("0.0.3").with_program("test")
}

/// The program's starter: the new server runs in this process (what `fs-x serve` does).
fn starter(socket: &Path) -> impl Fn() -> std::io::Result<()> {
    let socket: PathBuf = socket.to_owned();
    move || {
        start_new(&socket, "0.0.3");
        Ok(())
    }
}

fn run(
    socket: &Path,
    t: &SingleTimes,
    exe: Option<&Path>,
) -> Result<fs_rpc::Single<blob::Client>, SingleError> {
    let starter = starter(socket);
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(
        &rt,
        ensure_single::<blob::Client>(socket, &expected(), &starter, t, exe),
    )
}

fn alive(child: &mut Guard) -> bool {
    child.0.try_wait().unwrap().is_none()
}

fn finish(socket: &Path, single: fs_rpc::Single<blob::Client>) {
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        assert_eq!(seven(&single.connection.client).await, 7);
        drop(single);
        stop_server(socket).await.unwrap();
        wait_gone(socket).await;
    });
}

fn setup(mode: &str, marker: bool) -> (tempfile::TempDir, PathBuf, Guard, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("s.sock");
    let marker_path = dir.path().join("saved.marker");
    let child = spawn_child(
        &std::env::current_exe().unwrap(),
        &socket,
        mode,
        marker.then_some(marker_path.as_path()),
    );
    (dir, socket, child, marker_path)
}

#[test]
fn a_hanging_server_with_a_heartbeat_is_saved_then_ended() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let (_dir, socket, mut child, marker) = setup("silent-beat", true);
    let pid = child.0.id();
    let single = run(&socket, &times(), None).expect("taken over and replaced");
    let taken = single.taken_over.clone().expect("a process was ended");
    assert_eq!(
        (taken.pid, taken.saved, taken.how),
        (pid, Saved::Saved, "sigterm")
    );
    assert!(marker.exists(), "the program saved before it was ended");
    assert!(!alive(&mut child));
    let log = std::fs::read_to_string(socket.with_extension("takeover.log")).unwrap();
    assert!(
        log.contains(&format!("pid={pid}")) && log.contains("saved=saved"),
        "{log}"
    );
    finish(&socket, single);
}

#[test]
fn a_hanging_server_without_a_heartbeat_is_ended_by_sigterm() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let (_dir, socket, mut child, _) = setup("silent", false);
    let single = run(&socket, &times(), None).expect("taken over");
    let taken = single.taken_over.clone().unwrap();
    assert_eq!((taken.saved, taken.how), (Saved::Nothing, "sigterm"));
    assert!(!alive(&mut child));
    finish(&socket, single);
}

#[test]
fn a_busy_server_that_moves_is_never_ended() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let (_dir, socket, mut child, _) = setup("busy-moving", false);
    let mut t = times();
    t.overall = Duration::from_millis(4000);
    t.hang.no_progress = Duration::from_millis(700);
    let began = Instant::now();
    let error = run(&socket, &t, None).err().expect("not connected");
    assert!(
        began.elapsed() < Duration::from_secs(8),
        "the call has its limit"
    );
    match &error {
        SingleError::Busy {
            pid,
            what,
            progress,
        } => {
            assert_eq!(*pid, Some(child.0.id()));
            assert_eq!(what, "importing");
            assert!(*progress > 1);
        }
        other => panic!("{other}"),
    }
    let coded = RpcError::from(&error);
    assert_eq!(coded.code(), RpcErrorCode::ServerBusy);
    assert_eq!(coded.message_key(), "rpc-error-server-busy");
    assert!(alive(&mut child), "a busy program is never ended");
}

#[test]
fn a_busy_server_that_stands_still_is_ended() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let (_dir, socket, mut child, _) = setup("busy-stalled", true);
    let single = run(&socket, &times(), None).expect("ended after the standstill");
    assert!(single.taken_over.is_some());
    assert!(!alive(&mut child));
    finish(&socket, single);
}

#[test]
fn a_blocked_work_thread_behind_a_live_heartbeat_is_ended() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600); // only the tick can say hanging here
    let (_dir, socket, mut child, _) = setup("stuck-worker", true);
    let began = Instant::now();
    let single = run(&socket, &t, None).expect("ended because the tick stood still");
    assert!(began.elapsed() < Duration::from_secs(15));
    assert!(single.taken_over.is_some());
    assert!(!alive(&mut child));
    finish(&socket, single);
}

#[test]
fn another_program_is_only_reported() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("the-same-name");
    std::fs::copy(std::env::current_exe().unwrap(), &copy).unwrap();
    let socket = dir.path().join("s.sock");
    let marker = dir.path().join("saved.marker");
    let mut child = spawn_child(&copy, &socket, "silent-beat", Some(&marker));
    let began = Instant::now();
    let error = run(&socket, &times(), None).err().expect("not connected");
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "reported at once: {:?}",
        began.elapsed()
    );
    let coded = RpcError::from(&error);
    assert_eq!(coded.code(), RpcErrorCode::ServerUnresponsive);
    assert_eq!(
        coded.param("why").and_then(fs_rpc::Param::as_text),
        Some("other-program")
    );
    assert_eq!(
        coded.param("pid").and_then(fs_rpc::Param::as_number),
        Some(u64::from(child.0.id()))
    );
    assert!(coded.param("exe").is_some());
    assert!(alive(&mut child), "another program is never ended");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "nothing was saved in it either");
    // And when the copy is the program that counts as ours, it is ours: the program file is a parameter.
}

#[test]
fn a_program_without_a_channel_that_ignores_sigterm_is_never_killed() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let (_dir, socket, mut child, _) = setup("ignore-term", false);
    let error = run(&socket, &times(), None).err().expect("not connected");
    match &error {
        SingleError::Unresponsive(s) => assert_eq!(s.why, "no-control"),
        other => panic!("{other}"),
    }
    assert!(
        alive(&mut child),
        "SIGTERM was ignored and SIGKILL is not allowed here"
    );
}

#[test]
fn the_whole_call_has_a_limit() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let (_dir, socket, mut child, _) = setup("silent-beat", false);
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600);
    t.hang.no_tick = Duration::from_secs(600);
    t.hang.no_beat = Duration::from_secs(600);
    t.overall = Duration::from_millis(1500);
    let began = Instant::now();
    let error = run(&socket, &t, None).err().expect("not connected");
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );
    assert!(matches!(error, SingleError::TooLong(_)), "{error}");
    assert!(alive(&mut child));
}
