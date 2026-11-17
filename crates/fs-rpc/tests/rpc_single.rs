//! `ensure_single` against real hanging servers: a child process (this test binary again, or a copy of it) holds the socket and does
//! not answer.
//!
//! Pattern: Table of scenarios, one child per test, serial (`ONE_AT_A_TIME`). Rules and their tests (the mutant that removes the rule
//! turns the test red; the pure rules have unit tests in `src/single/*_tests.rs`):
//! R1 own program, hangs, heartbeat with a hook -> saved, then ended, replaced (`a_hanging_server_with_a_heartbeat_is_saved_then_ended`);
//! R2 hanging without a heartbeat channel -> NO signal at all, only `no-control` (`a_hanging_server_without_a_heartbeat_gets_no_signal_only_a_report`);
//! R3 busy and moving -> never ended, `server-busy` (`a_busy_server_that_moves_is_never_ended`);
//! R4 busy, number stands still -> hanging -> ended (`a_busy_server_that_stands_still_is_ended`);
//! R5 another program under the same name (a copy of the file) -> reported at once, never signalled, nothing saved
//! (`another_program_is_only_reported`); R6 the heartbeat answers but the work thread's tick stands still -> hanging -> ended
//! (`a_blocked_work_thread_behind_a_live_heartbeat_is_ended`); R8 every wait has its limit
//! (`the_whole_call_has_a_limit`). Mutants outside the table: M1 no limit on the whole call; M2 a signal for R2; M3 `Busy` counts as
//! hanging; M4 the copy of the program is taken for this program; M5 the heartbeat answer counts as health although the tick stands still.
#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs_interfaces::common_capnp::blob;
use fs_rpc::{
    Expected, HangTimes, RestartTimes, RpcError, RpcErrorCode, Saved, SingleError, SingleTimes,
    ensure_single, stop_server,
};

mod handshake_support;
mod single_support;
use handshake_support::*;
use single_support::*;

#[test]
fn helper_hanging_server() {
    hanging_server_main();
}

fn times() -> SingleTimes {
    let ms = |n: u64| Duration::from_millis(n) * scale();
    SingleTimes {
        handshake: RestartTimes {
            call: ms(300),
            stop: ms(2000),
            start: ms(3000),
        },
        hang: HangTimes {
            ping: ms(100),
            no_beat: ms(500),
            no_progress: ms(700),
            no_tick: ms(500),
        },
        save: ms(2000),
        term: ms(1500),
        kill: ms(1500),
        overall: ms(20_000),
    }
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

/// Connect through `ensure_single`; on success the connection is used once (the server answers), then the server is stopped, all inside
/// one runtime: the connection lives on the runtime that made it. Answers who was taken over.
fn run(
    socket: &Path,
    t: &SingleTimes,
    exe: Option<&Path>,
) -> Result<Option<fs_rpc::TakenOver>, SingleError> {
    let starter = starter(socket);
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let single = ensure_single::<blob::Client>(socket, &expected(), &starter, t, exe).await?;
        assert_eq!(seven(&single.connection.client).await, 7);
        let taken = single.taken_over.clone();
        drop(single);
        stop_server(socket).await.unwrap();
        wait_gone(socket).await;
        Ok(taken)
    })
}

fn alive(child: &mut Guard) -> bool {
    child.0.try_wait().unwrap().is_none()
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
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, marker) = setup("silent-beat", true);
    let pid = child.0.id();
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600); // the child never calls tick(): only the standing tick can say hanging
    let taken = run(&socket, &t, None)
        .expect("taken over and replaced")
        .expect("a process was ended");
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
}

#[test]
fn a_hanging_server_without_a_heartbeat_gets_no_signal_only_a_report() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("silent", false);
    let error = run(&socket, &times(), None).expect_err("not connected");
    let coded = RpcError::from(&error);
    assert_eq!(coded.code(), RpcErrorCode::ServerUnresponsive);
    assert_eq!(
        coded.param("why").and_then(fs_rpc::Param::as_text),
        Some("no-control")
    );
    assert_eq!(
        coded.param("pid").and_then(fs_rpc::Param::as_number),
        Some(u64::from(child.0.id()))
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(alive(&mut child), "no signal: it could not save first");
}

#[test]
fn a_busy_server_that_moves_is_never_ended() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("busy-moving", false);
    let mut t = times();
    t.overall = Duration::from_millis(4000);
    t.hang.no_progress = Duration::from_millis(700);
    let began = Instant::now();
    let error = run(&socket, &t, None).expect_err("not connected");
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
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("busy-stalled", true);
    let taken = run(&socket, &times(), None).expect("ended after the standstill");
    assert!(taken.is_some());
    assert!(!alive(&mut child));
}

#[test]
fn a_blocked_work_thread_behind_a_live_heartbeat_is_ended() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600); // only the tick can say hanging here
    let (_dir, socket, mut child, _) = setup("stuck-worker", true);
    let began = Instant::now();
    let taken = run(&socket, &t, None).expect("ended because the tick stood still");
    assert!(began.elapsed() < Duration::from_secs(15));
    assert!(taken.is_some());
    assert!(!alive(&mut child));
}

#[test]
fn another_program_is_only_reported() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("the-same-name");
    std::fs::copy(std::env::current_exe().unwrap(), &copy).unwrap();
    let socket = dir.path().join("s.sock");
    let marker = dir.path().join("saved.marker");
    let mut child = spawn_child(&copy, &socket, "silent-beat", Some(&marker));
    let began = Instant::now();
    let error = run(&socket, &times(), None).expect_err("not connected");
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
fn the_whole_call_has_a_limit() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("silent-beat", false);
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600);
    t.hang.no_tick = Duration::from_secs(600);
    t.hang.no_beat = Duration::from_secs(600);
    t.overall = Duration::from_millis(1500);
    let began = Instant::now();
    let error = run(&socket, &t, None).expect_err("not connected");
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );
    assert!(matches!(error, SingleError::TooLong(_)), "{error}");
    assert!(alive(&mut child));
}
