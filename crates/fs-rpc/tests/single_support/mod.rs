//! Shared by the `ensure_single` tests: the hanging server in a child process, and the child's guard.
#![allow(dead_code)]

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use std::time::Instant;

use fs_rpc::{Heartbeat, Saved, bind_socket};

use crate::handshake_support::*;

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
