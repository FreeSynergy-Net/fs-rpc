//! `Store.watch`: tell the program when the stamp changed, without polling.
//!
//! Pattern: Observer. The listener lives on the worker thread; dropping [`Watching`] lets go of the subscription (and the response
//! that holds it), which ends it on the service's side.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use fs_settings::{Coded, SettingCode, Stamp};

use super::RpcSettingsStore;
use super::worker::{CallError, Handle};
use crate::settings_capnp::store::listener;

static NEXT: AtomicU64 = AtomicU64::new(1);

struct Listener(Arc<dyn Fn(Stamp) + Send + Sync>);

impl listener::Server for Listener {
    async fn changed(self: Rc<Self>, params: listener::ChangedParams) -> Result<(), capnp::Error> {
        (self.0)(Stamp(params.get()?.get_stamp()));
        Ok(())
    }
}

/// A running subscription; dropping it ends it.
pub struct Watching {
    id: u64,
    handle: Handle,
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.handle.forget(self.id);
    }
}

impl RpcSettingsStore {
    /// Calls `on_change(stamp)` (on the client's own thread, keep it short) whenever another writer changed the settings.
    ///
    /// # Errors
    /// `store-unreachable` when no service answers; the subscription is not mended by itself after a lost connection.
    pub fn watch(
        &self,
        on_change: Arc<dyn Fn(Stamp) + Send + Sync>,
    ) -> Result<Watching, Coded<SettingCode>> {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let handle = self
            .worker
            .handle()
            .ok_or_else(|| super::unreachable_at(&self.socket))?;
        self.worker.exec(move |ctx| {
            Box::pin(async move {
                let held = ctx
                    .run(move |l| {
                        Box::pin(async move {
                            let mut req = l.store.watch_request();
                            req.get()
                                .set_listener(capnp_rpc::new_client(Listener(on_change)));
                            Ok::<_, CallError>(req.send().promise.await?)
                        })
                    })
                    .await?;
                ctx.held.borrow_mut().insert(id, Box::new(held));
                Ok(())
            })
        })?;
        Ok(Watching { id, handle })
    }
}
