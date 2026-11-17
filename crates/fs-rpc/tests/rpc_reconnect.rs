//! Reconnecting: a server that dies and comes back is found again, subscribed again, and the program is told to load anew;
//! a server that never comes back ends the loop within its deadline; a pause never grows past its ceiling.
//!
//! Pattern: Test server in this process (`handshake_support`), a recorder that is `Session` and `Resync` at once. Every test
//! has an outer limit, so a loop without a deadline is a red test, not a hang. Handshake tests run under `ONE_AT_A_TIME`.
//! Mutants (each turns a named test red): M1 no reconnect (the loop ends after the first link):
//! `a_dead_server_that_comes_back_is_reconnected_and_subscribed_again`, `a_feed_that_ended_on_a_live_server_is_subscribed_again`;
//! M2 no deadline (`give_up` ignored): `a_server_that_never_comes_back_ends_the_loop_within_the_deadline`; M3 the subscription
//! is not renewed (`attach` only on the first link): the same two tests; M4 the backoff has no ceiling:
//! `the_pause_grows_to_a_ceiling_and_starts_again`; M5 `resync_needed` always `false`: the first test; M6 a held
//! subscription is not dropped when the link is lost: `a_lost_link_drops_its_subscription`.
#![cfg(unix)]

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use fs_interfaces::common_capnp::blob;
use fs_rpc::{
    Backoff, Expected, ReconnectEnd, ReconnectTimes, Reconnected, RestartTimes, Resync, RpcError,
    Session, keep_connected, stop_server,
};
use futures::future::LocalBoxFuture;

mod handshake_support;
use handshake_support::*;

#[derive(Default)]
struct Recorder {
    attached: Cell<u32>,
    dropped: Rc<Cell<u32>>,
    events: RefCell<Vec<Reconnected>>,
    gave_up: RefCell<Vec<RpcError>>,
    ended: Rc<tokio::sync::Notify>,
}

struct Held(Rc<Cell<u32>>);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

impl Session<blob::Client> for Recorder {
    fn attach<'a>(&'a self, _client: blob::Client) -> LocalBoxFuture<'a, io::Result<Box<dyn Any>>> {
        self.attached.set(self.attached.get() + 1);
        let held: Box<dyn Any> = Box::new(Held(self.dropped.clone()));
        Box::pin(async move { Ok(held) })
    }

    fn ended(&self) -> LocalBoxFuture<'_, ()> {
        let ended = self.ended.clone();
        Box::pin(async move { ended.notified().await })
    }
}

impl Resync for Recorder {
    fn reconnected(&self, event: Reconnected) {
        self.events.borrow_mut().push(event);
    }
    fn gave_up(&self, error: &RpcError) {
        self.gave_up.borrow_mut().push(error.clone());
    }
}

fn quick(give_up_ms: u64) -> ReconnectTimes {
    let ms = Duration::from_millis;
    ReconnectTimes {
        attempt: ms(2000),
        backoff: vec![ms(30), ms(60), ms(100)],
        give_up: ms(give_up_ms),
        ping: ms(40),
        ping_limit: ms(500),
        handshake: RestartTimes {
            call: ms(500),
            stop: ms(2000),
            start: ms(2000),
        },
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let began = Instant::now();
    while !done() {
        assert!(began.elapsed() < Duration::from_secs(15), "{what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn a_dead_server_that_comes_back_is_reconnected_and_subscribed_again() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("r.sock");
    start_first(&socket, "0.0.3");
    static ALLOW: AtomicBool = AtomicBool::new(false);
    static STARTS: AtomicU32 = AtomicU32::new(0);
    ALLOW.store(false, Ordering::SeqCst);
    STARTS.store(0, Ordering::SeqCst);
    let starter_socket = socket.clone();
    let starter = move || -> io::Result<()> {
        STARTS.fetch_add(1, Ordering::SeqCst);
        if !ALLOW.load(Ordering::SeqCst) {
            return Err(io::Error::other("not yet"));
        }
        start_new(&starter_socket, "0.0.3");
        Ok(())
    };
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let recorder = Rc::new(Recorder::default());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let run = {
            let (recorder, socket) = (recorder.clone(), socket.clone());
            tokio::task::spawn_local(async move {
                let expected = Expected::new("0.0.3").with_program("test");
                keep_connected::<blob::Client>(
                    &socket,
                    &expected,
                    &starter,
                    &quick(10_000),
                    &*recorder,
                    &*recorder,
                    async {
                        let _ = stopped.await;
                    },
                )
                .await
            })
        };
        until("first link", || !recorder.events.borrow().is_empty()).await;
        assert_eq!(
            recorder.events.borrow()[0],
            Reconnected {
                attempt: 1,
                resync_needed: false
            }
        );
        assert_eq!(recorder.attached.get(), 1);
        // The server dies; nobody may start a new one for a while: the loop tries again, with pauses.
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        ALLOW.store(true, Ordering::SeqCst);
        until("second link", || recorder.events.borrow().len() >= 2).await;
        let second = recorder.events.borrow()[1];
        assert!(second.resync_needed, "{second:?}");
        assert!(
            second.attempt >= 2,
            "{second:?}: the first tries failed, the loop tried again"
        );
        assert_eq!(recorder.attached.get(), 2, "subscribed again");
        assert_eq!(recorder.dropped.get(), 1, "the old subscription was let go");
        assert!(STARTS.load(Ordering::SeqCst) >= 2);
        // Stopping ends the loop and lets the subscription go.
        stop.send(()).unwrap();
        assert_eq!(run.await.unwrap(), ReconnectEnd::Stopped);
        assert_eq!(recorder.dropped.get(), 2);
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
}

#[test]
fn a_server_that_never_comes_back_ends_the_loop_within_the_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("none.sock");
    let tries = Rc::new(Cell::new(0u32));
    let counted = tries.clone();
    let starter = move || -> io::Result<()> {
        counted.set(counted.get() + 1);
        Err(io::Error::other("no server can be started here"))
    };
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let recorder = Recorder::default();
        let began = Instant::now();
        let end = tokio::time::timeout(
            Duration::from_secs(10),
            keep_connected::<blob::Client>(
                &socket,
                &Expected::new("0.0.3"),
                &starter,
                &quick(400),
                &recorder,
                &recorder,
                std::future::pending(),
            ),
        )
        .await
        .expect("the loop has a deadline: it must not wait for ever");
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "{:?}",
            began.elapsed()
        );
        assert!(
            began.elapsed() >= Duration::from_millis(400),
            "not before the deadline"
        );
        let ReconnectEnd::GaveUp(error) = end else {
            panic!("expected GaveUp")
        };
        assert_eq!(
            recorder.gave_up.borrow().as_slice(),
            &[error.clone()],
            "told once"
        );
        assert!(recorder.events.borrow().is_empty());
        assert_eq!(recorder.attached.get(), 0);
        assert!(tries.get() >= 3, "tried again and again: {}", tries.get());
        assert!(tries.get() <= 15, "with pauses: {}", tries.get());
    });
}

#[test]
fn a_feed_that_ended_on_a_live_server_is_subscribed_again() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("e.sock");
    start_first(&socket, "0.0.3");
    let never = || -> io::Result<()> { panic!("the server lives: nothing to start") };
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let recorder = Rc::new(Recorder::default());
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let run = {
            let (recorder, socket) = (recorder.clone(), socket.clone());
            tokio::task::spawn_local(async move {
                keep_connected::<blob::Client>(
                    &socket,
                    &Expected::new("0.0.3").with_program("test"),
                    &never,
                    &quick(10_000),
                    &*recorder,
                    &*recorder,
                    async {
                        let _ = stopped.await;
                    },
                )
                .await
            })
        };
        until("first link", || !recorder.events.borrow().is_empty()).await;
        recorder.ended.notify_one(); // ChangeListener.ended: the server dropped this feed
        until("second link", || recorder.events.borrow().len() >= 2).await;
        assert!(recorder.events.borrow()[1].resync_needed);
        assert_eq!(recorder.attached.get(), 2);
        stop.send(()).unwrap();
        run.await.unwrap();
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
    });
}

#[test]
fn a_lost_link_drops_its_subscription() {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("d.sock");
    start_first(&socket, "0.0.3");
    let never = || -> io::Result<()> { Err(io::Error::other("no restart")) };
    let rt = runtime();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let recorder = Rc::new(Recorder::default());
        let run = {
            let (recorder, socket) = (recorder.clone(), socket.clone());
            tokio::task::spawn_local(async move {
                keep_connected::<blob::Client>(
                    &socket,
                    &Expected::new("0.0.3").with_program("test"),
                    &never,
                    &quick(500),
                    &*recorder,
                    &*recorder,
                    std::future::pending(),
                )
                .await
            })
        };
        until("link", || !recorder.events.borrow().is_empty()).await;
        stop_server(&socket).await.unwrap();
        wait_gone(&socket).await;
        until("dropped", || recorder.dropped.get() == 1).await;
        assert!(matches!(run.await.unwrap(), ReconnectEnd::GaveUp(_)));
    });
}

#[test]
fn the_pause_grows_to_a_ceiling_and_starts_again() {
    let s = Duration::from_secs;
    let mut backoff = Backoff::new(&[s(1), s(2), s(5)]);
    let first: Vec<_> = (0..5).map(|_| backoff.next_pause()).collect();
    assert_eq!(first, [s(1), s(2), s(5), s(5), s(5)]);
    for _ in 0..1000 {
        assert!(backoff.next_pause() <= s(5), "the ceiling");
    }
    backoff.reset();
    assert_eq!(backoff.next_pause(), s(1));
    assert_eq!(
        Backoff::new(&[]).next_pause(),
        s(1),
        "an empty list is one second"
    );
}

#[test]
fn the_defaults_are_the_plan_of_the_self_update() {
    let t = ReconnectTimes::default();
    let s = Duration::from_secs;
    assert_eq!(t.backoff, [s(1), s(2), s(5)]);
    assert_eq!(t.give_up, s(30));
    assert!(
        t.attempt <= s(10) && t.ping_limit <= s(5),
        "every step has a limit"
    );
}
