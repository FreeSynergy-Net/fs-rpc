//! The server skeleton: the contract `layouts` over ANY port (`LayoutStore`, plus `LayoutAdmin` for the instance service).
//!
//! Pattern: Facade + Adapter (contract -> port). The port is async and `?Send`, so the service runs on a `LocalSet` (as fs-rpc
//! `serve`); each method is a small handler in `read`, `write`, `overlay` or `admin`, and `root` only lists them. What the port
//! cannot do is NOT silent: the methods of the exchange files, `repair` and `bind` answer `unsupported{what}` (provider rule, ADR
//! 0045), `admin` on the person service too. Time and the log are the store's; the service stamps nothing itself.

use std::rc::Rc;

use fs_forms_layout::{Coded, LayoutAdmin, LayoutCode, LayoutStore, Reference, Who};
use fs_rpc::{Caller, CallerError, PerConnection};

use super::listeners::Hub;
use super::listeners::Listeners;

use crate::limits::ServiceLimits;

/// Receives who asked (the caller of the connection, from the system) and the `by` that client claims in `admin(by)`.
pub type ClaimNote = Rc<dyn Fn(&Caller, &str)>;

/// The host's mapping from the caller of a connection to its actor: the ONLY source of the actor (`Caller::id`, from the system).
/// `None`: this caller is nobody the service knows; the connection gets a capability that answers `unknown{what=caller}`.
pub type ActorOf = Rc<dyn Fn(&Caller) -> Option<Reference>>;

/// Told when the system could not say who is on a connection (the connection was dropped, code `caller-credentials`).
pub type RefusedNote = Rc<dyn Fn(&CallerError)>;

/// The service: which port it serves, for whom, and under which limits.
pub struct LayoutService<S: LayoutStore + ?Sized> {
    /// The person's descriptions.
    pub store: Rc<S>,
    /// `Some` only for the instance service; `None`: `admin` answers `unsupported{what=admin}`.
    pub admin: Option<Rc<dyn LayoutAdmin>>,
    /// The person this service serves; `""` as author on the wire means this one.
    pub person: Reference,
    /// The actor of a connection, from its [`Caller`] and from nothing else (never from a field of the client). `new` lets in every
    /// caller as the person (fs-rpc lets in the same OS user only); the host that knows more (a bot user, another account) sets
    /// its own mapping from `Caller::id`.
    pub actors: ActorOf,
    /// Called with the caller and the `by` a client claims in `admin(by)`; it is ignored for everything else (the actor is the
    /// connection's). The program hosting the service writes it to its log: a forged claim is visible, never effective.
    pub claimed_by: Option<ClaimNote>,
    /// Called when the system does not tell who is on a connection.
    pub refused: Option<RefusedNote>,
    /// The limits of the service (settings; rule 57).
    pub limits: ServiceLimits,
}

impl<S: LayoutStore + ?Sized> LayoutService<S> {
    /// A service over `store` for `person`, every caller being the person, the declared limits.
    #[must_use]
    pub fn new(store: Rc<S>, person: Reference) -> Self {
        let actor = person.clone();
        Self {
            store,
            admin: None,
            person,
            actors: Rc::new(move |_| Some(actor.clone())),
            claimed_by: None,
            refused: None,
            limits: ServiceLimits::declared(),
        }
    }

    /// `page-size{size, max, by}` when a call asks for more than the service takes.
    pub(crate) fn check_page(&self, size: u32) -> Result<(), Coded<LayoutCode>> {
        if size > self.limits.page_max {
            Err(Coded::new(LayoutCode::PageSize)
                .with("size", size)
                .with("max", self.limits.page_max)
                .with("by", self.limits.by.as_str()))
        } else {
            Ok(())
        }
    }
}

/// What the capability of ONE connection shares: the service (the same for all), who is on this connection, and the hub.
pub(crate) struct Shared<S: LayoutStore + ?Sized> {
    pub(crate) svc: Rc<LayoutService<S>>,
    pub(crate) caller: Caller,
    pub(crate) actor: Reference,
    pub(crate) listeners: Rc<Listeners>,
}

impl<S: LayoutStore + ?Sized> Shared<S> {
    /// Builds `Who` from the wire `author`: `""` or the person is this service's person; another person is
    /// `unknown{what=author}` until fs-access, never silently replaced. The actor is the connection's.
    pub(crate) fn who(&self, author: &str) -> Result<Who, Coded<LayoutCode>> {
        if author.is_empty() || author == self.svc.person.0 {
            Ok(Who {
                actor: self.actor.clone(),
                author: self.svc.person.clone(),
            })
        } else {
            Err(Coded::new(LayoutCode::Unknown).with("what", "author"))
        }
    }
}

/// The program's side for `fs_rpc::serve_callers` / `run_until_signal_callers`: builds the capability of EACH connection from its
/// caller. Listeners and signals are shared through the [`Hub`]; nothing that knows a caller is shared.
pub struct Layouts<S: LayoutStore + ?Sized> {
    svc: Rc<LayoutService<S>>,
    hub: Hub,
}

impl<S: LayoutStore + ?Sized + 'static> LayoutService<S> {
    /// The factory of the capabilities, one per connection; the watchers live in `hub` (one hub per store: a write through one
    /// socket is heard by the watchers of every other).
    #[must_use]
    pub fn per_connection(self, hub: &Hub) -> Layouts<S> {
        Layouts {
            svc: Rc::new(self),
            hub: hub.clone(),
        }
    }
}

impl<S: LayoutStore + ?Sized + 'static> PerConnection for Layouts<S> {
    fn bootstrap(&mut self, caller: &Caller) -> capnp::capability::Client {
        match (self.svc.actors)(caller) {
            Some(actor) => {
                let shared = Rc::new(Shared {
                    svc: self.svc.clone(),
                    caller: caller.clone(),
                    actor,
                    listeners: self.hub.0.clone(),
                });
                capnp_rpc::new_client::<crate::layouts_capnp::layouts::Client, _>(
                    super::root::Root { shared },
                )
                .client
            }
            None => capnp_rpc::new_broken_cap(crate::problem::err(
                &Coded::new(LayoutCode::Unknown).with("what", "caller"),
            )),
        }
    }

    fn refused(&mut self, why: &CallerError) {
        if let Some(note) = &self.svc.refused {
            note(why);
        }
    }
}

impl<S: LayoutStore + ?Sized> Shared<S> {
    /// After a write that went through: tell the watchers the new stamp. If the store cannot say the stamp right after a write,
    /// the write stays answered (it happened); the watchers hear with the next change.
    pub(crate) async fn changed(&self) {
        self.listeners.heard(&self.svc.store.stamp().await);
    }
}
