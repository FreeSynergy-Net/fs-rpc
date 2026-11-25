//! The hub of the watchers: every change of the module goes to every `watch`, filtered by the rules per subscriber.
//!
//! Pattern: Observer (the hub is the subject; the module's feed sink calls `publish`). The sink runs SYNCHRONOUSLY inside the
//! change: `publish` holds no borrow while it asks the rules, never blocks (`try_send`) and calls back into the module only for `follow.admits` and `rules.may`.
//! A subscriber that falls too far behind gets what is queued and then `ended` (it reads anew); a feed that lost changes or ended
//! (`end_all`, from `ModuleCore::on_feed_break`) ends every subscriber the same way; nothing is missed silently.
//!
//! The callbacks into the module are SYNCHRONOUS: `follow.admits` and `rules.may` run inside `publish`, in the change of the module. The
//! module must not hold a `borrow_mut` of its own state while it reports a change (the contract of `ChangeFeed` and `Follow::by`).

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use capnp_rpc::new_client;
use fs_interfaces::common_capnp::subscription;
use fs_interfaces::formsource_capnp::change_sink;
use fs_interfaces::module::{Ask, ChangeRef, Context, FeedLoss, Follow, RightsRules};
use fs_types::Utc;
use tokio::sync::mpsc::{self, Receiver, Sender, error::TrySendError};
use tokio::task::JoinHandle;

use super::WATCH_COMMAND;
use super::wire::write_change_ref;
use crate::feed::{FEED_CAPACITY, FEED_ENDED_PROBLEM_TYPE};
use crate::problem::Problem;

struct Sub {
    id: u64,
    who: Context,
    /// Which changes this subscriber follows (form and scope of its `watch`); `!Send`, so it stays here in the `LocalSet`.
    follow: Follow,
    tx: RefCell<Option<Sender<ChangeRef>>>,
    /// Why the queue was closed by the hub (overflow, lost feed); `None` = it was not (the source of the hub closed).
    why: Cell<Option<&'static str>>,
}

/// Pattern: Observer - the subject of all `watch` subscriptions of one module.
#[derive(Default)]
pub struct WatchHub {
    subs: RefCell<Vec<Rc<Sub>>>,
    next: Cell<u64>,
    /// The feed of the module is gone ([`FeedLoss::Closed`]): a new `watch` is refused.
    closed: Cell<bool>,
}

impl WatchHub {
    /// An empty hub.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many subscriptions are open.
    #[must_use]
    pub fn watchers(&self) -> usize {
        self.subs.borrow().len()
    }

    /// Tell `change` to every subscriber that follows it (`Follow::admits`) and that the rules allow to see it (`WATCH_COMMAND` on the record, at `at`).
    /// Synchronous, no borrow held while the rules are asked.
    pub fn publish(&self, rules: &dyn RightsRules, at: Utc, change: &ChangeRef) {
        let subs: Vec<Rc<Sub>> = self.subs.borrow().clone();
        for sub in subs {
            if !sub.follow.admits(change) {
                continue;
            }
            let ask = Ask::new(&sub.who, WATCH_COMMAND, Some(change.object.as_str()), at);
            if !rules.may(&ask).is_allowed() {
                continue;
            }
            let full = match sub.tx.borrow().as_ref() {
                Some(tx) => matches!(tx.try_send(change.clone()), Err(TrySendError::Full(_))),
                None => false,
            };
            if full {
                // Close the queue: what is in it is still delivered, then the subscriber hears `ended`.
                sub.why.set(Some("the consumer fell too far behind"));
                sub.tx.borrow_mut().take();
            }
        }
    }

    /// The feed of the module lost changes or ended (what `ModuleCore::on_feed_break` reports): every queue is closed, what is in it is
    /// still delivered, then every subscriber hears `ended` 410 with the reason (it reads anew, `changes`, and subscribes again). After
    /// `FeedLoss::Closed` no change will come any more: a new `watch` is refused (503). After `Lagged` a new `watch` hears again.
    /// Synchronous, no borrow held while the queues close.
    pub fn end_all(&self, loss: FeedLoss) {
        let why = match loss {
            FeedLoss::Closed => {
                self.closed.set(true);
                "the source closed"
            }
            _ => "the source lagged",
        };
        let subs: Vec<Rc<Sub>> = self.subs.borrow().clone();
        for sub in subs {
            if sub.tx.borrow_mut().take().is_some() {
                sub.why.set(Some(why));
            }
        }
    }

    /// Whether the feed of the module is gone (`FeedLoss::Closed`); a new `watch` is refused then.
    pub(super) fn is_closed(&self) -> bool {
        self.closed.get()
    }

    /// Open a subscription for `who`: the changes go to `sink` until the returned capability is dropped.
    pub(super) fn subscribe(
        self: &Rc<Self>,
        who: Context,
        follow: Follow,
        sink: change_sink::Client,
    ) -> subscription::Client {
        let id = self.next.get();
        self.next.set(id + 1);
        let (tx, rx) = mpsc::channel(FEED_CAPACITY);
        let sub = Rc::new(Sub {
            id,
            who,
            follow,
            tx: RefCell::new(Some(tx)),
            why: Cell::new(None),
        });
        self.subs.borrow_mut().push(sub.clone());
        let task = tokio::task::spawn_local(deliver(rx, sink, sub));
        new_client(Subscribed {
            task: RefCell::new(Some(task)),
            hub: Rc::downgrade(self),
            id,
        })
    }

    fn remove(&self, id: u64) {
        self.subs.borrow_mut().retain(|s| s.id != id);
    }
}

async fn deliver(mut rx: Receiver<ChangeRef>, sink: change_sink::Client, sub: Rc<Sub>) {
    let why = loop {
        let Some(change) = rx.recv().await else {
            break sub.why.get().unwrap_or("the source closed");
        };
        let mut request = sink.changed_request();
        if write_change_ref(request.get().init_change(), &change).is_err() {
            break "a change could not be told";
        }
        if request.send().await.is_err() {
            return; // the consumer failed: nobody to tell
        }
    };
    let mut request = sink.ended_request();
    Problem::new(FEED_ENDED_PROBLEM_TYPE, "The feed ended", why, 410)
        .write(request.get().init_problem());
    let _ = request.send().promise.await;
}

struct Subscribed {
    task: RefCell<Option<JoinHandle<()>>>,
    hub: Weak<WatchHub>,
    id: u64,
}

impl subscription::Server for Subscribed {}

impl Drop for Subscribed {
    fn drop(&mut self) {
        if let Some(task) = self.task.borrow_mut().take() {
            task.abort();
        }
        if let Some(hub) = self.hub.upgrade() {
            hub.remove(self.id);
        }
    }
}
