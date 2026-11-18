//! `watch @23`: tell the host when the stamp changed, without polling. The listener lives on the client's thread; dropping
//! [`Watching`] lets go of the subscription AND the response that holds it (fs-rpc: both must go), which ends it on the
//! service's side (its `Subscription` removes the listener when dropped).
//!
//! Pattern: Observer. The subscription is not mended by itself after a lost connection: the host calls `watch` again and then
//! reads the stamp (feed rule of fs-rpc: subscribe first, then read).

use std::any::Any;
use std::rc::Rc;

use fs_forms_layout::{Coded, LayoutCode, Stamp};

use super::RpcLayouts;
use crate::layouts_capnp::layouts::listener;

struct Listener(Rc<dyn Fn(Stamp)>);

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
impl listener::Server for Listener {
    async fn changed(self: Rc<Self>, params: listener::ChangedParams) -> Result<(), capnp::Error> {
        (self.0)(Stamp(params.get()?.get_stamp()));
        Ok(())
    }
}

/// A running subscription; dropping it ends it.
pub struct Watching {
    _held: Box<dyn Any>,
}

impl RpcLayouts {
    /// Calls `on_change(stamp)` (on the client's thread; keep it short) whenever the descriptions changed.
    ///
    /// # Errors
    /// `unreachable{socket}` when no service answers; `too-many{what=watchers,...}` when the service keeps no more listeners.
    pub async fn watch(&self, on_change: Rc<dyn Fn(Stamp)>) -> Result<Watching, Coded<LayoutCode>> {
        let held = self
            .ctx
            .call(async move |c| {
                let mut req = c.watch_request();
                req.get()
                    .set_listener(capnp_rpc::new_client(Listener(on_change)));
                Ok(req.send().promise.await?)
            })
            .await?;
        Ok(Watching {
            _held: Box::new(held),
        })
    }
}
