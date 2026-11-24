//! `hold_calling`: jobs on the held connection, and a registration (`OnAttach`) that runs first on every connection. Unix.
//!
//! Pattern: Test (a real server per test in a thread, short real times, an outer watchdog); one server run at a time.
#![cfg(unix)]

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use fs_interfaces::common_capnp::blob;
use fs_rpc::{Hold, HoldEvent, OnAttach, hold_calling, stop_server};
use futures::future::LocalBoxFuture;

mod handshake_support;
mod hold_support;
mod reconnect_support;
use handshake_support::*;
use hold_support::*;

/// Counts the attaches and keeps a guard that counts the drops.
struct Intro {
    attached: Arc<AtomicU32>,
    dropped: Arc<AtomicU32>,
}
struct Guard(Arc<AtomicU32>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
impl OnAttach for Intro {
    fn attach(
        &self,
        _c: capnp::capability::Client,
    ) -> LocalBoxFuture<'_, io::Result<Box<dyn std::any::Any>>> {
        self.attached.fetch_add(1, Ordering::SeqCst);
        let guard = Guard(self.dropped.clone());
        Box::pin(async move { Ok(Box::new(guard) as Box<dyn std::any::Any>) })
    }
}

fn counters() -> (Arc<AtomicU32>, Arc<AtomicU32>) {
    (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(0)))
}

fn start(socket: &std::path::Path, give_up: u64) -> (Hold, Arc<AtomicU32>, Arc<AtomicU32>) {
    let (attached, dropped) = counters();
    let intro = Intro {
        attached: attached.clone(),
        dropped: dropped.clone(),
    };
    let held = hold_calling(
        spec(socket, Duration::from_secs(30), give_up),
        move || Box::new(intro),
        |_: HoldEvent| {},
    )
    .unwrap();
    (held, attached, dropped)
}

fn size_job() -> fs_rpc::Job<u64> {
    Box::new(|client| {
        Box::pin(async move {
            let blob: blob::Client = capnp::capability::FromClientHook::new(client.hook);
            let answer = blob
                .size_request()
                .send()
                .promise
                .await
                .map_err(io::Error::other)?;
            Ok(answer.get().map_err(io::Error::other)?.get_bytes())
        })
    })
}

/// C1: a job runs on the held connection and gives its answer (after `OnAttach` ran once).
#[test]
fn a_job_runs_on_the_held_connection() {
    let _one = lock();
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("c1.sock");
    serve_and_wait(&socket, Duration::from_secs(30));
    let (held, attached, _dropped) = start(&socket, 5_000);
    assert_eq!(attached.load(Ordering::SeqCst), 1);
    assert_eq!(held.call(size_job(), Duration::from_secs(5)).unwrap(), 7);
    drop(held);
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
}

/// C2: the module is lost and started again: `OnAttach` ran again (the old registration was dropped), and calls work again.
#[test]
fn the_registration_runs_again_after_a_reconnect() {
    let _one = lock();
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("c2.sock");
    serve_and_wait(&socket, Duration::from_secs(30));
    let (held, attached, dropped) = start(&socket, 10_000);
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
    let began = Instant::now();
    while attached.load(Ordering::SeqCst) < 2 {
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "no second attach"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        dropped.load(Ordering::SeqCst) >= 1,
        "the old registration must end"
    );
    // The slot is filled after `attach`; a call right now may still see no connection: try for a short while.
    let answer = (0..100).find_map(|_| {
        let got = held.call(size_job(), Duration::from_secs(5)).ok();
        if got.is_none() {
            std::thread::sleep(Duration::from_millis(50));
        }
        got
    });
    assert_eq!(answer, Some(7));
    drop(held);
    tokio::task::LocalSet::new().block_on(&rt, async {
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
}

/// C3: no connection gives `NotConnected` at once (never buffered); a job over its limit gives `TimedOut`.
#[test]
fn no_connection_is_not_connected_and_a_slow_job_times_out() {
    let _one = lock();
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("c3.sock");
    serve_and_wait(&socket, Duration::from_secs(30));
    let (attached, dropped) = counters();
    let intro = Intro { attached, dropped };
    let mut spec = spec(&socket, Duration::from_secs(30), 30_000);
    spec.starter = Box::new(|| -> io::Result<()> { Err(io::Error::other("stays down")) });
    let held = hold_calling(spec, move || Box::new(intro), |_: HoldEvent| {}).unwrap();
    let slow: fs_rpc::Job<u64> = Box::new(|_| {
        Box::pin(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(0)
        })
    });
    let began = Instant::now();
    let err = held.call(slow, Duration::from_millis(200)).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    assert!(began.elapsed() < Duration::from_secs(3));
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
    std::thread::sleep(Duration::from_millis(300));
    let began = Instant::now();
    let err = held.call(size_job(), Duration::from_secs(5)).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotConnected);
    assert!(
        began.elapsed() < Duration::from_millis(500),
        "must not wait"
    );
}
