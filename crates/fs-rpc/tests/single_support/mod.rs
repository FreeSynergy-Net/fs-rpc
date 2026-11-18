//! Shared by the `ensure_single` tests: the hanging server in a child process, and the child's guard.
#![allow(dead_code)]

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use std::time::Instant;

use std::path::PathBuf;

use fs_interfaces::common_capnp::blob;
use fs_rpc::{
    Expected, HangTimes, Heartbeat, RestartTimes, Saved, SingleError, SingleTimes, bind_socket,
    ensure_single, stop_server,
};

use crate::handshake_support::*;

mod started;
#[allow(unused_imports)] // not every test file that shares this module checks the start time
pub use started::assert_started_just_now;

/// A child that is gone at the end of the test whatever happens.
pub struct Guard(pub Child);
impl Drop for Guard {
    fn drop(&mut self) {
        let id = self.0.id();
        CHILDREN.lock().unwrap().retain(|p| *p != id);
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The hanging server (in the child): holds the socket, never answers `Control`. `FS_SINGLE_MODE`: `silent` (no heartbeat),
/// `silent-beat` (heartbeat with a hook, work thread asleep, no tick), `busy-moving`, `busy-stalled`, `stuck-worker` (one tick, then
/// blocked), `ignore-term` (no heartbeat, SIGTERM ignored).
/// # Panics
/// If the socket cannot be bound (the test then fails).
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

pub fn spawn_child(exe: &Path, socket: &Path, mode: &str, marker: Option<&Path>) -> Guard {
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
    CHILDREN.lock().unwrap().push(guard.0.id());
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

pub fn times() -> SingleTimes {
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

pub fn expected() -> Expected {
    Expected::new("0.0.3").with_program("test")
}

/// The program's starter: the new server runs in this process (what `fs-x serve` does).
pub fn starter(socket: &Path) -> impl Fn() -> std::io::Result<()> {
    let socket: PathBuf = socket.to_owned();
    move || {
        start_new(&socket, "0.0.3");
        Ok(())
    }
}

/// Connect through `ensure_single`; on success the connection is used once (the server answers), then the server is stopped, all inside
/// one runtime: the connection lives on the runtime that made it. Answers who was taken over.
pub fn run(
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

pub fn alive(child: &mut Guard) -> bool {
    child.0.try_wait().unwrap().is_none()
}

pub fn setup(mode: &str, marker: bool) -> (tempfile::TempDir, PathBuf, Guard, PathBuf) {
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
