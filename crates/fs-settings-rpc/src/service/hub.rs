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

/// What the service is told about a `by` a client claims in `admin(by)`.
pub type ClaimNote = Arc<dyn Fn(&str) + Send + Sync>;

/// The service: which port it serves, and the clock, the OS user and the actor it knows.
pub struct SettingsService<S: ?Sized, I: ?Sized> {
    /// The person's settings.
    pub person: Arc<S>,
    /// `Some` only for `serve --instance` (admin calls); `None`: `admin` answers "not possible".
    pub instance: Option<Arc<I>>,
    /// The OS user of the caller, as a NAME (the number when the system has no name; one form, see [`crate::os_user_name`]). fs-rpc
    /// admits only a peer whose effective user is the service's own (`SO_PEERCRED`, `geteuid` semantics), so it is this
    /// user; only for the log.
    pub os_user: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    /// The actor of every admin call: the identity of the CONNECTION (default `urn:fs:os-user:<name>`), never a field of the
    /// client. Until fs-access it is the OS user; an AI seat would be its own actor.
    pub actor_of_connection: Arc<dyn Fn() -> fs_types::Reference + Send + Sync>,
    /// Called with the `by` a client claims in `admin(by)` (also an empty one). It is never the actor: the host writes it to its
    /// log, so a forged claim is visible and never effective ([AKTEUR-AUS-VERBINDUNG]; `by` leaves the contract in 0.3).
    pub claimed_by: Option<ClaimNote>,
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
            os_user: Arc::new(|| Some(crate::identity::os_user_name())),
            actor_of_connection: Arc::new(crate::identity::actor_of_os_user),
            claimed_by: None,
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
pub(crate) fn err(c: &crate::Coded<fs_settings::SettingCode>) -> capnp::Error {
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
        // `by` is what the client CLAIMS: it goes to the host's record of claims and is ignored otherwise.
        let claimed = params.get()?.get_by()?.to_str()?.to_owned();
        if let Some(note) = &self.shared.svc.claimed_by {
            note(&claimed);
        }
        let Some(instance) = self.shared.svc.instance.clone() else {
            return Err(fs_rpc::unsupported(&fs_interfaces::Unsupported::new(
                "settings.admin",
                "this is the person's service; the administrator's commands go to the instance service",
            )));
        };
        let actor = (self.shared.svc.actor_of_connection)();
        results
            .get()
            .set_admin(capnp_rpc::new_client(admin::AdminImpl::new(
                self.shared.clone(),
                instance,
                actor,
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
