//! Holding a module: one connection per frontend (or program) to a module service, kept on its own thread until the handle drops.
//! Attached = the connection stands; detached = the handle dropped. With no connection left, the module ends after its idle time.
//!
//! Pattern: Observer (the owner learns attach, re-attach and give-up as events) + RAII (the handle is the registration). The loop is
//! [`keep_connected`](super::keep_connected); the frame (own thread, `LocalSet`, block until the first answer) is that of
//! [`client::Worker`](super::client::Worker).

use std::any::Any;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use capnp::capability::FromClientHook;
use futures::future::LocalBoxFuture;
use tokio::sync::oneshot;

use super::{
    Expected, ReconnectTimes, Reconnected, Resync, RpcError, ServerStarter, Session, keep_connected,
};

/// How long the thread of a dropped hold lets the closing connection finish.
const CLOSE_GRACE: Duration = Duration::from_millis(100);

/// What happens to a hold, in the order it happens: `Attached` (again and again), maybe `GaveUp`, `Detached` once at the end.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HoldEvent {
    /// The connection stands. `resync_needed`: something may have been missed (not the first connection): read anew.
    Attached {
        /// Which try of this connection succeeded (1 = the first).
        attempt: u32,
        /// Not the first connection of this hold.
        resync_needed: bool,
    },
    /// No success within `times.give_up`; the holding is over (the handle lives on until it drops).
    GaveUp {
        /// What the last try said.
        error: RpcError,
    },
    /// The handle dropped; the connection is closed. The last event, exactly once.
    Detached,
}

/// Pattern: Observer - whoever holds learns every attach, re-attach and give-up as an event.
///
/// The observer runs on the thread of the connection: it may only pass on (channel, flag) and never block.
pub trait HoldObserver: Send + 'static {
    /// One event of the hold.
    fn event(&self, event: HoldEvent);
}

impl<F: Fn(HoldEvent) + Send + 'static> HoldObserver for F {
    fn event(&self, event: HoldEvent) {
        self(event);
    }
}

/// Pattern: Builder - what is held and how: the module's socket, what it must say, how to start it again.
#[non_exhaustive]
pub struct HoldSpec {
    /// For the thread name `fs-hold-<name>` and error texts.
    pub name: String,
    /// The module's socket.
    pub socket: PathBuf,
    /// What the server must say about itself (`Expected::new("")` for any).
    pub expected: Expected,
    /// Starts the module again when it is gone; only the service of the module folder.
    pub starter: Box<dyn ServerStarter + Send>,
    /// The waits of connecting again (the defaults of [`keep_connected`]).
    pub times: ReconnectTimes,
}

impl HoldSpec {
    /// A hold of `socket` with the default times.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        socket: impl Into<PathBuf>,
        expected: Expected,
        starter: Box<dyn ServerStarter + Send>,
    ) -> Self {
        Self {
            name: name.into(),
            socket: socket.into(),
            expected,
            starter,
            times: ReconnectTimes::default(),
        }
    }

    /// The same with other times.
    #[must_use]
    pub fn with_times(mut self, times: ReconnectTimes) -> Self {
        self.times = times;
        self
    }
}

/// Pattern: RAII - the handle is the registration with the module; dropping it signs off (and waits for the end of the thread).
pub struct Hold {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Hold {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// No session: the connection is only kept (pings), nothing is subscribed on it.
struct NoSession;

impl Session<capnp::capability::Client> for NoSession {
    fn attach(
        &self,
        _client: capnp::capability::Client,
    ) -> LocalBoxFuture<'_, io::Result<Box<dyn Any>>> {
        Box::pin(async { Ok(Box::new(()) as Box<dyn Any>) })
    }
}

/// Turns the loop's hints into events, and tells `hold_with` about the first result.
struct Events<O> {
    observer: O,
    first: std::cell::RefCell<Option<mpsc::Sender<io::Result<()>>>>,
}

impl<O: HoldObserver> Resync for Events<O> {
    fn reconnected(&self, event: Reconnected) {
        self.observer.event(HoldEvent::Attached {
            attempt: event.attempt,
            resync_needed: event.resync_needed,
        });
        if let Some(first) = self.first.borrow_mut().take() {
            let _ = first.send(Ok(()));
        }
    }

    fn gave_up(&self, error: &RpcError) {
        if let Some(first) = self.first.borrow_mut().take() {
            let _ = first.send(Err(io::Error::other(error.to_string())));
        }
        self.observer.event(HoldEvent::GaveUp {
            error: error.clone(),
        });
    }
}

/// Holds the connection (and only that: no session on it). Blocks until the first connection stands.
///
/// # Errors
/// The thread did not start, or nothing could be connected within `spec.times.give_up` (the text is the problem's).
pub fn hold(spec: HoldSpec, observer: impl HoldObserver) -> io::Result<Hold> {
    hold_with::<capnp::capability::Client, NoSession>(spec, || NoSession, observer)
}

/// [`hold`] with a [`Session`] on every connection (a subscription, for instance). `session` is built on the new thread, so it may
/// be `!Send`. Blocks until the first connection stands and is subscribed.
///
/// # Errors
/// As for [`hold`].
pub fn hold_with<C, S>(
    spec: HoldSpec,
    session: impl FnOnce() -> S + Send + 'static,
    observer: impl HoldObserver,
) -> io::Result<Hold>
where
    C: FromClientHook + Clone + 'static,
    S: Session<C> + 'static,
{
    let (stop, stopped) = oneshot::channel::<()>();
    let (ready, answer) = mpsc::channel::<io::Result<()>>();
    let name = format!("fs-hold-{}", spec.name);
    let thread = std::thread::Builder::new()
        .name(name)
        .spawn(move || run::<C, S>(spec, session, observer, stopped, ready))?;
    let hold = Hold {
        stop: Some(stop),
        thread: Some(thread),
    };
    // On `Err` the handle drops here: the stop is sent and the thread joined.
    answer
        .recv()
        .map_err(|_| io::Error::other("the hold thread ended"))??;
    Ok(hold)
}

fn run<C, S>(
    spec: HoldSpec,
    session: impl FnOnce() -> S,
    observer: impl HoldObserver,
    stopped: oneshot::Receiver<()>,
    ready: mpsc::Sender<io::Result<()>>,
) where
    C: FromClientHook + Clone + 'static,
    S: Session<C> + 'static,
{
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => return drop(ready.send(Err(e))),
    };
    let local = tokio::task::LocalSet::new();
    let events = Events {
        observer,
        first: std::cell::RefCell::new(Some(ready)),
    };
    let session = session();
    let HoldSpec {
        socket,
        expected,
        starter,
        times,
        ..
    } = spec;
    local.block_on(&runtime, async {
        let dropped = std::cell::Cell::new(false);
        let mut stopped = stopped;
        keep_connected::<C>(
            &socket,
            &expected,
            starter.as_ref(),
            &times,
            &session,
            &events,
            async {
                let _ = (&mut stopped).await;
                dropped.set(true);
            },
        )
        .await;
        // After a give-up the handle lives on until it drops: `Detached` is the end of the handle, never of the loop.
        if !dropped.get() {
            let _ = stopped.await;
        }
    });
    // Let the closing of the connection reach the module (bounded: the owner of the handle waits for this thread).
    runtime.block_on(async {
        let _ = tokio::time::timeout(CLOSE_GRACE, local).await;
    });
    events.observer.event(HoldEvent::Detached);
}
