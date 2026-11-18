//! Who wants to hear about a change (`Store.watch`). Pattern: Observer; a subscription ends when its capability is dropped.

use std::cell::RefCell;

use crate::settings_capnp::store::listener;

/// The listeners of one service.
#[derive(Default)]
pub(crate) struct Listeners {
    list: RefCell<Vec<(u64, listener::Client)>>,
}

impl Listeners {
    pub(crate) fn add(&self, id: u64, client: listener::Client) {
        self.list.borrow_mut().push((id, client));
    }

    pub(crate) fn remove(&self, id: u64) {
        self.list.borrow_mut().retain(|(i, _)| *i != id);
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

/// The capability a watcher holds: dropping it removes the listener.
pub(crate) struct Subscription<F: Fn(u64)> {
    pub(crate) id: u64,
    pub(crate) remove: F,
}

impl<F: Fn(u64)> Drop for Subscription<F> {
    fn drop(&mut self) {
        (self.remove)(self.id);
    }
}
