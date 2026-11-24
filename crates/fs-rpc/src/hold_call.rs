//! Calling on a held connection: a job runs on the thread of the hold with the capability of the CURRENT connection, and what
//! must happen on every new connection (introduce, watch) is registered once and runs again after each reconnect.
//!
//! Pattern: Command (a job for the thread of the connection, like `client::Worker::call`, but on the HELD connection) +
//! Template Method (`OnAttach` is what happens first on every new connection). Deviation from `Worker::call`: between two
//! connections nothing is buffered: `NotConnected` at once, never silently later.

use std::any::Any;
use std::cell::RefCell;
use std::io;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use capnp::capability::Client;
use futures::future::LocalBoxFuture;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::hold::start;
use super::{Hold, HoldObserver, HoldSpec, Session};

/// What the owner of the thread may wait on top of the job's own limit (the answer travels back through a channel).
const ANSWER_MARGIN: Duration = Duration::from_secs(2);

/// A job for the hold: gets the capability of the current connection and runs on the thread of the hold (it may be `!Send` inside).
pub type Job<T> = Box<dyn FnOnce(Client) -> LocalBoxFuture<'static, io::Result<T>> + Send>;

/// The job with its answer channel behind it; `None` = no connection at the moment.
pub(super) type Call = Box<dyn FnOnce(Option<Client>) -> LocalBoxFuture<'static, ()> + Send>;

/// Pattern: Template Method - what happens FIRST on every new connection (`introduce`, `watch`); the hook is `attach`.
///
/// Runs on the thread of the hold, again after every reconnect. The answer is what keeps the registration alive: dropping it
/// ends it (the connection is gone or the hold ended).
pub trait OnAttach: 'static {
    /// Register on the new connection. `client` is its root capability.
    fn attach(&self, client: Client) -> LocalBoxFuture<'_, io::Result<Box<dyn Any>>>;
}

impl Hold {
    /// Runs `job` on the held connection and waits for its answer, at most `limit`.
    ///
    /// # Errors
    /// `NotConnected` at once when no connection stands (between two connections, or the hold ended), `TimedOut` after `limit`,
    /// `Unsupported` when the hold was made by [`hold`](super::hold) or [`hold_with`](super::hold_with) (nothing can be called);
    /// otherwise what the job says.
    pub fn call<T: Send + 'static>(&self, job: Job<T>, limit: Duration) -> io::Result<T> {
        let calls = self.calls.as_ref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "this hold cannot call (use hold_calling)",
            )
        })?;
        let (answer, wait) = mpsc::channel();
        let call: Call = Box::new(move |client| {
            Box::pin(async move {
                let result = match client {
                    None => Err(io::Error::new(
                        io::ErrorKind::NotConnected,
                        "no connection at the moment",
                    )),
                    Some(client) => tokio::time::timeout(limit, job(client))
                        .await
                        .unwrap_or_else(|_| Err(io::ErrorKind::TimedOut.into())),
                };
                let _ = answer.send(result);
            })
        });
        calls
            .send(call)
            .map_err(|_| io::Error::new(io::ErrorKind::NotConnected, "the hold ended"))?;
        match wait.recv_timeout(limit + ANSWER_MARGIN) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(io::ErrorKind::TimedOut.into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "the hold ended",
            )),
        }
    }
}

type Slot = Rc<RefCell<Option<Client>>>;

/// The registration of one connection: `on_attach`'s answer, and the slot that is emptied when the connection goes.
struct Attached {
    _kept: Box<dyn Any>,
    slot: Slot,
}

impl Drop for Attached {
    fn drop(&mut self) {
        self.slot.borrow_mut().take();
    }
}

struct CallSession {
    on_attach: Box<dyn OnAttach>,
    slot: Slot,
    calls: RefCell<Option<UnboundedReceiver<Call>>>,
}

impl Session<Client> for CallSession {
    fn attach(&self, client: Client) -> LocalBoxFuture<'_, io::Result<Box<dyn Any>>> {
        Box::pin(async move {
            if let Some(calls) = self.calls.borrow_mut().take() {
                tokio::task::spawn_local(serve_calls(calls, self.slot.clone()));
            }
            // First the registration, then the connection counts as usable: no call overtakes `introduce`.
            let kept = self.on_attach.attach(client.clone()).await?;
            *self.slot.borrow_mut() = Some(client);
            Ok(Box::new(Attached {
                _kept: kept,
                slot: self.slot.clone(),
            }) as Box<dyn Any>)
        })
    }
}

/// Ends when the handle drops (the sender with it).
async fn serve_calls(mut calls: UnboundedReceiver<Call>, slot: Slot) {
    while let Some(call) = calls.recv().await {
        let current = slot.borrow().clone();
        tokio::task::spawn_local(call(current));
    }
}

/// [`hold_with`](super::hold_with) with calls: `on_attach` (built on the new thread, so it may be `!Send`) runs first on every
/// connection, also after a reconnect; [`Hold::call`] runs jobs on the current connection. Blocks until the first connection
/// stands and `on_attach` ran on it.
///
/// # Errors
/// As for [`hold`](super::hold).
pub fn hold_calling(
    spec: HoldSpec,
    on_attach: impl FnOnce() -> Box<dyn OnAttach> + Send + 'static,
    observer: impl HoldObserver,
) -> io::Result<Hold> {
    let (sender, calls): (UnboundedSender<Call>, _) = unbounded_channel();
    let session = move || CallSession {
        on_attach: on_attach(),
        slot: Rc::new(RefCell::new(None)),
        calls: RefCell::new(Some(calls)),
    };
    start::<Client, _>(spec, session, observer, Some(sender))
}
