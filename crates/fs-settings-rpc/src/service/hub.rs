//! The service struct and the hub capability `SettingsHub`.
//!
//! Pattern: Facade (see the module above).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use fs_rpc::{Caller, CallerError, PerConnection};
use fs_settings::{Actor, Catalog, InstanceStore, ProgramName, SettingsStore};
use fs_types::Utc;

use super::guard::Guard;
use super::listeners::Listeners;
use super::{admin, store};
use crate::actor::{ActorOfConnection, FailClosed, OsUserIsPerson};
use crate::problem::to_problem;
use crate::settings_capnp::settings_hub;
use crate::wire::{write_lock, write_snapshot};

/// What the service is told about a `by` a client claims in `admin(by)`.
pub type ClaimNote = Arc<dyn Fn(&str) + Send + Sync>;

/// How the service names the OS user of a caller for the log.
pub type OsUserOf = Arc<dyn Fn(&Caller) -> Option<String> + Send + Sync>;

/// The service: which port it serves, and the clock, the OS user and the actor it knows.
pub struct SettingsService<S: ?Sized, I: ?Sized> {
    /// The person's settings.
    pub person: Arc<S>,
    /// `Some` only for `serve --instance` (admin calls); `None`: `admin` answers "not possible".
    pub instance: Option<Arc<I>>,
    /// The OS user of the caller for the log: the NAME the system has for `Caller` (display), the number (`Caller::id`) when it has none. One
    /// form. The caller comes from the system per connection ([`fs_rpc::Caller`]), never from a field of the client.
    pub os_user: OsUserOf,
    /// The actor of every admin call: derived from `Caller::id` ONLY (default `urn:fs:os-user:<id>`); never the `by` of the client, not the
    /// name (display), not the environment. An AI seat would be its own caller later.
    pub actor_of: Arc<dyn Fn(&Caller) -> fs_types::Reference + Send + Sync>,
    /// Called with the code (`caller-credentials`) of a connection the system could not name: nothing is served to it. Default: standard
    /// error.
    pub refused_note: Option<ClaimNote>,
    /// Called with the `by` a client claims in `admin(by)` (also an empty one). It is never the actor. The default writes the claim
    /// to standard error ([`crate::log_claim`]), so a forged claim is visible and never effective ([AKTEUR-AUS-VERBINDUNG]; `by`
    /// leaves the contract in 0.3); a host replaces it with its own log, `None` is silent (tests only).
    pub claimed_by: Option<ClaimNote>,
    /// The clock of the service (UTC ms); tests inject one.
    pub now: Arc<dyn Fn() -> Utc + Send + Sync>,
    /// Who acts on a connection, from the connection alone ([`ActorOfConnection`]). Default: [`FailClosed`], every connection is a bot, so a key
    /// declared `human_only` is invisible and untouchable on it (every way goes through `fs_settings::Surface`, in `service/guard.rs`).
    pub actors: Arc<dyn ActorOfConnection>,
    /// What the keys are (their `human_only`): the module's `Surface` decides with it. Without it a bot sees no key at all.
    pub catalog: Catalog,
}

impl<S: ?Sized, I: ?Sized> SettingsService<S, I> {
    /// [`Self::new`] for a host that says on purpose: every connection of the service's own OS user is a person ([`OsUserIsPerson`]; the
    /// transport admits that user only, ADR 0054 open point 1).
    #[must_use]
    pub fn new_os_user_is_person(person: Arc<S>, instance: Option<Arc<I>>) -> Self {
        Self {
            actors: Arc::new(OsUserIsPerson),
            ..Self::new(person, instance)
        }
    }

    /// A service over `person` (and `instance` when it is the instance service), with the system clock and the own OS user. No connection is a
    /// person ([`FailClosed`]): set `actors` (e.g. [`OsUserIsPerson`]) and `catalog`.
    #[must_use]
    pub fn new(person: Arc<S>, instance: Option<Arc<I>>) -> Self {
        Self {
            actors: Arc::new(FailClosed),
            catalog: Catalog::new(),
            person,
            instance,
            os_user: Arc::new(crate::identity::os_user_of),
            actor_of: Arc::new(crate::identity::actor_of),
            refused_note: Some(Arc::new(crate::identity::log_refused)),
            claimed_by: Some(Arc::new(crate::identity::log_claim)),
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
    pub(crate) guard: Arc<Guard>,
    pub(crate) listeners: Listeners,
    next: Cell<u64>,
}

impl<S, I> SettingsService<S, I>
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    /// What to hand to `fs_rpc::serve_callers` / `run_until_signal_callers`: a `SettingsHub` for EACH connection, built with the caller the
    /// system named. No capability that carries a caller is kept in state the connections share (fs-rpc H2).
    #[must_use]
    pub fn callers(self) -> SettingsCallers<S, I> {
        SettingsCallers {
            shared: Rc::new(Shared {
                guard: Arc::new(Guard::new(self.catalog.clone())),
                svc: self,
                listeners: Listeners::default(),
                next: Cell::new(1),
            }),
        }
    }
}

/// The program's side for fs-rpc: one hub capability per connection ([`PerConnection`]).
pub struct SettingsCallers<S: ?Sized, I: ?Sized> {
    shared: Rc<Shared<S, I>>,
}

impl<S, I> PerConnection for SettingsCallers<S, I>
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    fn bootstrap(&mut self, caller: &Caller) -> capnp::capability::Client {
        // A caller whose id cannot make an actor is REFUSED, with the code `caller-unknown` on every call and a line in the log (never served
        // under `unknown`, never under the name).
        if !crate::identity::id_usable(caller.id()) {
            if let Some(note) = &self.shared.svc.refused_note {
                note(crate::problem::CALLER_UNKNOWN);
            }
            return capnp_rpc::new_client::<settings_hub::Client, _>(super::refused::Refused)
                .client;
        }
        capnp_rpc::new_client::<settings_hub::Client, _>(Hub {
            actor: self.shared.svc.actors.actor(caller),
            shared: self.shared.clone(),
            caller: caller.clone(),
        })
        .client
    }

    fn refused(&mut self, why: &CallerError) {
        if let Some(note) = &self.shared.svc.refused_note {
            note(why.code());
        }
    }
}

struct Hub<S: ?Sized, I: ?Sized> {
    shared: Rc<Shared<S, I>>,
    /// The caller of THIS connection (the hub was built for it).
    caller: Caller,
    /// Who is on THIS connection, as the port `actors` said when the hub was built (never changes).
    actor: Actor,
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
        let access = store::Access::Own(name);
        let store = store::StoreImpl::new(self.shared.clone(), access, self.actor);
        results.get().set_store(capnp_rpc::new_client(store));
        Ok(())
    }

    async fn manage(
        self: Rc<Self>,
        _: settings_hub::ManageParams,
        mut results: settings_hub::ManageResults,
    ) -> Result<(), capnp::Error> {
        let store = store::StoreImpl::new(self.shared.clone(), store::Access::All, self.actor);
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
        let actor = (self.shared.svc.actor_of)(&self.caller);
        let os_user = (self.shared.svc.os_user)(&self.caller);
        results
            .get()
            .set_admin(capnp_rpc::new_client(admin::AdminImpl::new(
                self.shared.clone(),
                instance,
                (actor, os_user),
                self.actor,
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
        let held = self.shared.guard.instance(self.actor, held);
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
