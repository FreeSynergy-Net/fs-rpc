//! `watch @23`: tell the host when the stamp changed, without polling. The listener lives on the client's thread; dropping
//! [`Watching`] lets go of the subscription AND the response that holds it (fs-rpc: both must go), which ends it on the
//! service's side (its `Subscription` removes the listener when dropped).
//!
//! Pattern: Observer. The subscription is not mended by itself after a lost connection: the host calls `watch` again and then
//! reads the stamp (feed rule of fs-rpc: subscribe first, then read).

use std::any::Any;
use std::cell::Cell;
use std::rc::Rc;

use fs_forms_layout::{Coded, LayoutCode, Stamp};

use super::RpcLayouts;
use crate::layouts_capnp::layouts::listener;

/// The listener lives as long as the connection or the [`Watching`]. If it goes away while the `Watching` is still held, the
/// connection ended: `on_end` tells the host (never a silently lost subscription).
struct Listener {
    on_change: Rc<dyn Fn(Stamp)>,
    on_end: Rc<dyn Fn()>,
    released: Rc<Cell<bool>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        if !self.released.get() {
            (self.on_end)();
        }
    }
}

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
impl listener::Server for Listener {
    async fn changed(self: Rc<Self>, params: listener::ChangedParams) -> Result<(), capnp::Error> {
        (self.on_change)(Stamp(params.get()?.get_stamp()));
        Ok(())
    }
}

/// A running subscription; dropping it ends it.
pub struct Watching {
    released: Rc<Cell<bool>>,
    _held: Box<dyn Any>,
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.released.set(true);
    }
}

impl RpcLayouts {
    /// Calls `on_change(stamp)` (on the client's thread; keep it short) whenever the descriptions changed.
    ///
    /// # Errors
    /// `unreachable{socket}` when no service answers; `too-many{what=watchers,...}` when the service keeps no more listeners.
    pub async fn watch(
        &self,
        on_change: Rc<dyn Fn(Stamp)>,
        on_end: Rc<dyn Fn()>,
    ) -> Result<Watching, Coded<LayoutCode>> {
        // `released` is true until the service has accepted the subscription: a refused `watch` never reports an end
        let released = Rc::new(Cell::new(true));
        let flag = released.clone();
        let held = self
            .ctx
            .call(async move |c| {
                let mut req = c.watch_request();
                req.get().set_listener(capnp_rpc::new_client(Listener {
                    on_change,
                    on_end,
                    released: flag,
                }));
                Ok(req.send().promise.await?)
            })
            .await?;
        released.set(false);
        Ok(Watching {
            released,
            _held: Box::new(held),
        })
    }
}
