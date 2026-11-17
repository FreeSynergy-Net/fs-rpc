//! The service struct and the hub capability `SettingsHub`.
//!
//! Pattern: Facade (see the module above).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use fs_settings::{InstanceStore, ProgramName, SettingsStore};
use fs_types::Utc;

use super::listeners::Listeners;
use super::{admin, store};
use crate::problem::to_problem;
use crate::settings_capnp::settings_hub;
use crate::wire::{write_lock, write_snapshot};

/// The service: which port it serves, and the clock and OS user it knows.
pub struct SettingsService<S: ?Sized, I: ?Sized> {
    /// The person's settings.
    pub person: Arc<S>,
    /// `Some` only for `serve --instance` (admin calls); `None`: `admin` answers "not possible".
    pub instance: Option<Arc<I>>,
    /// The OS user of the caller as the system says (fs-rpc lets only the SAME OS user connect, so it is the service's own);
    /// only for the log.
    pub os_user: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    /// The clock of the service (UTC ms); tests inject one.
    pub now: Arc<dyn Fn() -> Utc + Send + Sync>,
}

impl<S: ?Sized, I: ?Sized> SettingsService<S, I> {
    /// A service over `person` (and `instance` when it is the instance service), with the system clock and the own OS user.
    #[must_use]
    pub fn new(person: Arc<S>, instance: Option<Arc<I>>) -> Self {
        Self {
            person,
            instance,
            os_user: Arc::new(own_os_user),
            now: Arc::new(system_now),
        }
    }
}

fn system_now() -> Utc {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    Utc::from_millis(ms)
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)] // the same signature as on other systems, where there is no answer
fn own_os_user() -> Option<String> {
    Some(rustix::process::getuid().as_raw().to_string())
}

#[cfg(not(unix))]
fn own_os_user() -> Option<String> {
    None
}

/// What every capability of one service shares.
pub(crate) struct Shared<S: ?Sized, I: ?Sized> {
    pub(crate) svc: SettingsService<S, I>,
    pub(crate) listeners: Listeners,
    next: Cell<u64>,
}

impl<S, I> SettingsService<S, I>
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    /// The root capability to hand to `fs_rpc::run_until_signal_as` / `serve`: `SettingsHub`.
    #[must_use]
    pub fn bootstrap(self) -> capnp::capability::Client {
        let shared = Rc::new(Shared {
            svc: self,
            listeners: Listeners::default(),
            next: Cell::new(1),
        });
        capnp_rpc::new_client::<settings_hub::Client, _>(Hub { shared }).client
    }
}

struct Hub<S: ?Sized, I: ?Sized> {
    shared: Rc<Shared<S, I>>,
}

/// A `Coded` as the RPC error a client reads back.
pub(crate) fn err(c: &fs_settings::Coded<fs_settings::SettingCode>) -> capnp::Error {
    to_problem(c).into_error()
}

/// Runs a sync port call on the blocking pool.
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, capnp::Error> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| fs_rpc::failed(format!("port call: {e}")))
}

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
impl<S, I> settings_hub::Server for Hub<S, I>
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    async fn open(
        self: Rc<Self>,
        params: settings_hub::OpenParams,
        mut results: settings_hub::OpenResults,
    ) -> Result<(), capnp::Error> {
        let name = ProgramName::new(params.get()?.get_program()?.to_str()?).map_err(|c| err(&c))?;
        let store = store::StoreImpl::new(self.shared.clone(), store::Access::Own(name));
        results.get().set_store(capnp_rpc::new_client(store));
        Ok(())
    }

    async fn manage(
        self: Rc<Self>,
        _: settings_hub::ManageParams,
        mut results: settings_hub::ManageResults,
    ) -> Result<(), capnp::Error> {
        let store = store::StoreImpl::new(self.shared.clone(), store::Access::All);
        results.get().set_store(capnp_rpc::new_client(store));
        Ok(())
    }

    async fn admin(
        self: Rc<Self>,
        params: settings_hub::AdminParams,
        mut results: settings_hub::AdminResults,
    ) -> Result<(), capnp::Error> {
        let by = params.get()?.get_by()?.to_str()?.trim().to_owned();
        let Some(instance) = self.shared.svc.instance.clone() else {
            return Err(fs_rpc::unsupported(&fs_interfaces::Unsupported::new(
                "settings.admin",
                "this is the person's service; the administrator's commands go to the instance service",
            )));
        };
        if by.is_empty() {
            return Err(fs_rpc::invalid_request("by (the actor) is mandatory"));
        }
        results
            .get()
            .set_admin(capnp_rpc::new_client(admin::AdminImpl::new(
                self.shared.clone(),
                instance,
                by,
            )));
        Ok(())
    }

    async fn instance(
        self: Rc<Self>,
        _: settings_hub::InstanceParams,
        mut results: settings_hub::InstanceResults,
    ) -> Result<(), capnp::Error> {
        let Some(instance) = self.shared.svc.instance.clone() else {
            // The person's service in v1: the instance layer is empty (c0 decision 5).
            results.get().init_values();
            results.get().init_locks(0);
            return Ok(());
        };
        let held = blocking(move || instance.instance()).await?;
        let mut out = results.get();
        write_snapshot(out.reborrow().init_values(), &held.values);
        let mut locks = out.init_locks(u32::try_from(held.locks.len()).unwrap_or(0));
        for (i, (key, lock)) in (0..).zip(&held.locks) {
            let mut e = locks.reborrow().get(i);
            e.set_key(key.as_str());
            if let Some(v) = held.values.values.get(key) {
                crate::wire::write_raw(e.reborrow().init_value(), v);
            }
            write_lock(e.init_lock(), lock);
        }
        Ok(())
    }
}

impl<S: ?Sized, I: ?Sized> Shared<S, I> {
    pub(crate) fn next_id(&self) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        id
    }
}
