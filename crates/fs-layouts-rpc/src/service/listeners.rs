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
    list: RefCell<Vec<(u64, listener::Client)>>,
    next: Cell<u64>,
    count: Arc<AtomicUsize>,
}

impl Listeners {
    pub(crate) fn len(&self) -> usize {
        self.list.borrow().len()
    }

    /// Adds a listener and returns its id.
    pub(crate) fn add(&self, client: listener::Client) -> u64 {
        let id = self.next.get() + 1;
        self.next.set(id);
        self.list.borrow_mut().push((id, client));
        self.count.fetch_add(1, Ordering::SeqCst);
        id
    }

    pub(crate) fn remove(&self, id: u64) {
        let before = self.list.borrow().len();
        self.list.borrow_mut().retain(|(i, _)| *i != id);
        if self.list.borrow().len() < before {
            self.count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// The count, readable from any thread.
    pub(crate) fn counter(&self) -> Arc<AtomicUsize> {
        self.count.clone()
    }

    /// Tells every listener the new stamp (a stream: no answer is waited for).
    pub(crate) fn tell(&self, stamp: u64) {
        for (_, l) in self.list.borrow().iter() {
            let mut req = l.changed_request();
            req.get().set_stamp(stamp);
            tokio::task::spawn_local(async move {
                let _ = req.send().await;
            });
        }
    }
}

/// How many watchers a service has right now (for tests and for the program's own status); readable from any thread.
#[derive(Clone)]
pub struct Watchers(pub(crate) Arc<AtomicUsize>);

impl Watchers {
    /// The count.
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
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
