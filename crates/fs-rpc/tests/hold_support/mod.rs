//! Shared by the hold tests: a real server with an idle end in a thread, the spec with the starter of the module, a few waits.
//!
//! Pattern: Test support (one server run at a time in the process: `stop_self` is a flag of the process).
#![cfg(unix)]
#![allow(dead_code)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use capnp_rpc::new_client;
use fs_interfaces::common_capnp::blob;
use fs_rpc::{
    Expected, HoldEvent, HoldSpec, ServerIdentity, Serving, Shared, run_until_signal_serving,
};

use crate::handshake_support::*;
use crate::reconnect_support::quick;

pub const IDLE: Duration = Duration::from_millis(300);

/// Starts a server with the idle end in a thread (like a module service) and waits until it listens.
pub fn serve(socket: &Path, idle: Duration) {
    let socket: PathBuf = socket.to_owned();
    let handle = std::thread::spawn(move || {
        let serving = Serving::new(ServerIdentity::new("test", "0.0.3")).with_idle_timeout(idle);
        let _ = run_until_signal_serving(&socket, serving, || async {
            let root: blob::Client = new_client(Seven);
            Ok(Shared(root.client))
        });
    });
    SERVERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(handle);
}

pub fn serve_and_wait(socket: &Path, idle: Duration) {
    serve(socket, idle);
    let began = Instant::now();
    while connect_once(socket) {
        assert!(began.elapsed() < Duration::from_secs(10), "no server");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The starter of the module: the old server thread is joined first (its stop request is forgotten), then a new one starts.
pub fn spec(socket: &Path, idle: Duration, give_up_ms: u64) -> HoldSpec {
    let own = socket.to_owned();
    let starter = move || -> io::Result<()> {
        let old: Vec<_> = SERVERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain(..)
            .collect();
        for handle in old {
            let _ = handle.join();
        }
        serve(&own, idle);
        Ok(())
    };
    let expected = Expected::new("0.0.3").with_program("test");
    HoldSpec::new("test", socket, expected, Box::new(starter)).with_times(quick(give_up_ms))
}

/// Waits until every server thread of this test has ended (no connection is made: a probe would count as a user and keep it up).
pub fn wait_ended() {
    let began = Instant::now();
    loop {
        let mut all = SERVERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if all.iter().all(std::thread::JoinHandle::is_finished) {
            for handle in all.drain(..) {
                let _ = handle.join();
            }
            return;
        }
        drop(all);
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "the module did not end"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn events() -> (mpsc::Sender<HoldEvent>, mpsc::Receiver<HoldEvent>) {
    mpsc::channel()
}

pub fn next(rx: &mpsc::Receiver<HoldEvent>) -> HoldEvent {
    rx.recv_timeout(Duration::from_secs(10)).expect("an event")
}

pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
