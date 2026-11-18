//! Who wants to hear about a change (`watch @23`). A subscription ends when its capability is dropped (the client drops it
//! together with the response it came in; fs-rpc `lib.rs`). The number of listeners is a setting (`watchers_max`).
//!
//! Pattern: Observer; the subscription removes its own listener on drop.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::layouts_capnp::layouts::listener;

/// The listeners of one service.
#[derive(Default)]
pub(crate) struct Listeners {
    list: RefCell<Vec<(u64, String, listener::Client)>>,
    next: Cell<u64>,
    count: Arc<AtomicUsize>,
    missed: Arc<AtomicUsize>,
}

impl Listeners {
    pub(crate) fn len(&self) -> usize {
        self.list.borrow().len()
    }

    /// How many listeners the caller `owner` (`Caller::id`) holds, over all connections.
    pub(crate) fn count_of(&self, owner: &str) -> usize {
        self.list
            .borrow()
            .iter()
            .filter(|(_, o, _)| o == owner)
            .count()
    }

    /// Adds a listener of the caller `owner` and returns its id.
    pub(crate) fn add(&self, owner: &str, client: listener::Client) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.list.borrow_mut().push((id, owner.to_owned(), client));
        self.count.fetch_add(1, Ordering::SeqCst);
        id
    }

    pub(crate) fn remove(&self, id: u64) {
        let before = self.list.borrow().len();
        self.list.borrow_mut().retain(|(i, _, _)| *i != id);
        if self.list.borrow().len() < before {
            self.count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// The count, readable from any thread.
    pub(crate) fn counter(&self) -> Arc<AtomicUsize> {
        self.count.clone()
    }

    /// Tells every listener the new stamp (a stream: no answer is waited for). A listener that cannot be told is dead (its
    /// connection is gone): it is removed at once, not kept until the subscription is dropped.
    pub(crate) fn tell(self: &Rc<Self>, stamp: u64) {
        for (id, _, l) in self.list.borrow().iter() {
            let mut req = l.changed_request();
            req.get().set_stamp(stamp);
            let (me, id) = (self.clone(), *id);
            tokio::task::spawn_local(async move {
                if req.send().await.is_err() {
                    me.remove(id);
                }
            });
        }
    }

    /// After a write: tell the watchers the store's stamp. A store that cannot say its stamp right after a write is counted
    /// (`Watchers::missed`), never ignored; the write itself stays answered, the watchers hear with the next change.
    pub(crate) fn heard<E>(self: &Rc<Self>, stamp: &Result<fs_forms_layout::Stamp, E>) {
        match stamp {
            Ok(s) => self.tell(s.0),
            Err(_) => {
                self.missed.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    pub(crate) fn missed_counter(&self) -> Arc<AtomicUsize> {
        self.missed.clone()
    }
}

/// The listeners of ONE database, shared by EVERY service (socket, connection) that serves it: what is written through one socket
/// is heard by the watchers of every other. Make one `Hub` per store, hand it to [`LayoutService::bootstrap_in`](super::LayoutService::bootstrap_in)
/// for each socket. Not `Send`: it lives on the thread of the `LocalSet` that serves.
#[derive(Clone, Default)]
pub struct Hub(pub(crate) Rc<Listeners>);

impl Hub {
    /// A hub without listeners.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The counters of this hub (readable from any thread).
    #[must_use]
    pub fn watchers(&self) -> Watchers {
        Watchers(self.0.counter(), self.0.missed_counter())
    }
}

/// How many watchers a service has right now (for tests and for the program's own status); readable from any thread.
#[derive(Clone)]
pub struct Watchers(pub(crate) Arc<AtomicUsize>, pub(crate) Arc<AtomicUsize>);

impl Watchers {
    /// The count.
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    /// How many times the store could not say its stamp after a write (the watchers were not told then).
    #[must_use]
    pub fn missed(&self) -> usize {
        self.1.load(Ordering::SeqCst)
    }
}

/// The capability a watcher holds: dropping it removes the listener.
pub(crate) struct Subscription {
    pub(crate) id: u64,
    pub(crate) listeners: Rc<Listeners>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.listeners.remove(self.id);
    }
}

impl fs_interfaces::common_capnp::subscription::Server for Subscription {}
