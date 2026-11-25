//! Holding a module: one connection per frontend (or program) to a module service, kept on its own thread until the handle
//! drops (then the module ends after its idle time). Attached = the connection stands; detached = the handle dropped.
//!
//! Pattern: Observer (attach, re-attach, give-up as events) + RAII (the handle is the registration). The loop is
//! [`keep_connected`](super::keep_connected), the thread frame that of [`client::Worker`](super::client::Worker).

use std::any::Any;
use std::cell::RefCell;
use std::io;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use capnp::capability::{Client, FromClientHook};
use futures::future::LocalBoxFuture;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;

use super::hold_call::Call;
use super::{HoldSpec, Reconnected, Resync, RpcError, Session, keep_connected};

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

/// Pattern: RAII - the handle is the registration with the module; dropping it signs off (and waits for the end of the thread).
pub struct Hold {
    pub(super) stop: Option<oneshot::Sender<()>>,
    pub(super) thread: Option<JoinHandle<()>>,
    /// Jobs for the thread ([`Hold::call`]); only a hold made by `hold_calling` has it.
    pub(super) calls: Option<UnboundedSender<Call>>,
}

impl Drop for Hold {
    fn drop(&mut self) {
        drop(self.calls.take());
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// No session: the connection is only kept (pings), nothing is subscribed on it.
struct NoSession;

impl Session<Client> for NoSession {
    fn attach(&self, _client: Client) -> LocalBoxFuture<'_, io::Result<Box<dyn Any>>> {
        Box::pin(async { Ok(Box::new(()) as Box<dyn Any>) })
    }
}

/// Turns the loop's hints into events, and tells `hold_with` about the first result.
struct Events<O> {
    observer: O,
    first: RefCell<Option<mpsc::Sender<io::Result<()>>>>,
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
    hold_with::<Client, NoSession>(spec, || NoSession, observer)
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
    start::<C, S>(spec, session, observer, None)
}

/// The one start of every hold; `calls` is the sender of `hold_calling` (the session owns the other end).
pub(super) fn start<C, S>(
    spec: HoldSpec,
    session: impl FnOnce() -> S + Send + 'static,
    observer: impl HoldObserver,
    calls: Option<UnboundedSender<Call>>,
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
        calls,
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
        first: RefCell::new(Some(ready)),
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
