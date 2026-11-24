//! `hold`: a handle that keeps a connection to a module service and tells its owner what happens (attached, attached again after a
//! loss, gave up, detached); dropping it signs off, and the module ends after its idle time. Unix (the helpers start real servers).
//!
//! Pattern: Test (a real server per test in a thread, short real times, an outer watchdog). One server run at a time in the process
//! (`stop_self` is a flag of the process), so the tests take `ONE_AT_A_TIME`.
#![cfg(unix)]

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use capnp_rpc::new_client;
use fs_interfaces::common_capnp::blob;
use fs_rpc::{
    Expected, Hold, HoldEvent, HoldSpec, ServerIdentity, Serving, Session, Shared, hold, hold_with,
    run_until_signal_serving, stop_server,
};
use futures::future::LocalBoxFuture;

mod handshake_support;
mod reconnect_support;
use handshake_support::*;
use reconnect_support::quick;

const IDLE: Duration = Duration::from_millis(300);

/// Starts a server with the idle end in a thread (like a module service) and waits until it listens.
fn serve(socket: &Path, idle: Duration) {
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

fn serve_and_wait(socket: &Path, idle: Duration) {
    serve(socket, idle);
    let began = Instant::now();
    while connect_once(socket) {
        assert!(began.elapsed() < Duration::from_secs(10), "no server");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The starter of the module: the old server thread is joined first (its stop request is forgotten), then a new one starts.
fn spec(socket: &Path, idle: Duration, give_up_ms: u64) -> HoldSpec {
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
fn wait_ended() {
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

fn events() -> (mpsc::Sender<HoldEvent>, mpsc::Receiver<HoldEvent>) {
    mpsc::channel()
}

fn next(rx: &mpsc::Receiver<HoldEvent>) -> HoldEvent {
    rx.recv_timeout(Duration::from_secs(10)).expect("an event")
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// H1: a held module lives past three idle times; after the drop it ends within the idle time (plus a margin).
#[test]
fn a_held_module_stays_up_and_ends_after_the_drop() {
    let _one = lock();
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("h1.sock");
    serve_and_wait(&socket, IDLE);
    let (tx, _rx) = events();
    let held = hold(spec(&socket, IDLE, 5_000), move |e| drop(tx.send(e))).unwrap();
    std::thread::sleep(IDLE * 3);
    assert!(
        SERVERS.lock().unwrap().iter().all(|h| !h.is_finished()),
        "the module ended while held"
    );
    let dropped = Instant::now();
    drop(held);
    wait_ended();
    assert!(dropped.elapsed() < IDLE + Duration::from_millis(600));
}

/// H2: exactly one `Attached` (first try, no resync) before `Ok`, and exactly one `Detached` as the last event.
#[test]
fn the_events_come_in_order_and_detached_comes_once_at_the_end() {
    let _one = lock();
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("h2.sock");
    serve_and_wait(&socket, IDLE);
    let (tx, rx) = events();
    let held = hold(spec(&socket, IDLE, 5_000), move |e| drop(tx.send(e))).unwrap();
    let attached = HoldEvent::Attached {
        attempt: 1,
        resync_needed: false,
    };
    assert_eq!(rx.try_recv().unwrap(), attached);
    assert!(rx.try_recv().is_err(), "nothing else before the drop");
    drop(held);
    let rest: Vec<_> = rx.try_iter().collect();
    assert_eq!(rest, vec![HoldEvent::Detached]);
    wait_ended();
}

struct Count(Arc<AtomicU32>);
impl Session<blob::Client> for Count {
    fn attach(
        &self,
        _client: blob::Client,
    ) -> LocalBoxFuture<'_, io::Result<Box<dyn std::any::Any>>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(Box::new(()) as Box<dyn std::any::Any>) })
    }
}

/// H3: the module is stopped while held: the starter brings it back, `Attached { resync_needed: true }`, the session ran twice.
#[test]
fn a_lost_module_is_started_again_and_the_session_attaches_again() {
    let _one = lock();
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("h3.sock");
    serve_and_wait(&socket, Duration::from_secs(30));
    let (tx, rx) = events();
    let attached = Arc::new(AtomicU32::new(0));
    let counter = attached.clone();
    let held = hold_with::<blob::Client, _>(
        spec(&socket, Duration::from_secs(30), 10_000),
        move || Count(counter),
        move |e| drop(tx.send(e)),
    )
    .unwrap();
    assert!(matches!(
        next(&rx),
        HoldEvent::Attached {
            resync_needed: false,
            ..
        }
    ));
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
    match next(&rx) {
        HoldEvent::Attached { resync_needed, .. } => assert!(resync_needed),
        other => panic!("{other:?}"),
    }
    assert_eq!(attached.load(Ordering::SeqCst), 2);
    drop(held);
    tokio::task::LocalSet::new().block_on(&rt, async {
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
}

/// H4: nothing listens and nothing can be started: `hold` gives an error within the give-up time and the thread ends (`Detached`).
#[test]
fn a_hold_without_a_module_fails_and_leaves_no_thread() {
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("h4.sock");
    let starter = || -> io::Result<()> { Err(io::Error::other("cannot start")) };
    let expected = Expected::new("0.0.3").with_program("test");
    let spec = HoldSpec::new("h4", &socket, expected, Box::new(starter)).with_times(quick(200));
    let (tx, rx) = events();
    let began = Instant::now();
    let failed = hold(spec, move |e| drop(tx.send(e)));
    assert!(failed.is_err());
    assert!(began.elapsed() < Duration::from_secs(5));
    let seen: Vec<_> = rx.try_iter().collect();
    assert!(
        matches!(
            seen.as_slice(),
            [HoldEvent::GaveUp { .. }, HoldEvent::Detached]
        ),
        "{seen:?}"
    );
}

/// H5: a drop during the pauses of connecting again returns fast (the owner may be a window thread).
#[test]
fn a_drop_while_connecting_again_returns_fast() {
    let _one = lock();
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("h5.sock");
    serve_and_wait(&socket, Duration::from_secs(30));
    let (tx, rx) = events();
    let mut spec = spec(&socket, Duration::from_secs(30), 30_000);
    spec.starter = Box::new(|| -> io::Result<()> { Err(io::Error::other("stays down")) });
    let held = hold(spec, move |e| drop(tx.send(e))).unwrap();
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
    std::thread::sleep(Duration::from_millis(250));
    let began = Instant::now();
    drop(held);
    assert!(began.elapsed() < Duration::from_millis(300));
    assert_eq!(rx.try_iter().last(), Some(HoldEvent::Detached));
}

/// H6: the handle can be kept in a `Send + Sync` owner (a `Records` of the host).
#[test]
fn the_handle_is_send_and_sync() {
    fn needs<T: Send + Sync>() {}
    needs::<Hold>();
}
