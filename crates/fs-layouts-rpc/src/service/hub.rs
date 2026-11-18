//! The server skeleton: the contract `layouts` over ANY port (`LayoutStore`, plus `LayoutAdmin` for the instance service).
//!
//! Pattern: Facade + Adapter (contract -> port). The port is async and `?Send`, so the service runs on a `LocalSet` (as fs-rpc
//! `serve`); each method is a small handler in `read`, `write`, `overlay` or `admin`, and `root` only lists them. What the port
//! cannot do is NOT silent: the methods of the exchange files, `repair` and `bind` answer `unsupported{what}` (provider rule, ADR
//! 0045), `admin` on the person service too. Time and the log are the store's; the service stamps nothing itself.

use std::rc::Rc;

use fs_forms_layout::{Coded, LayoutAdmin, LayoutCode, LayoutStore, Reference, Who};

use super::listeners::Listeners;
use super::listeners::{Hub, Watchers};

use crate::limits::ServiceLimits;

/// Receives the `by` a client claims in `admin(by)`.
pub type ClaimNote = Rc<dyn Fn(&str)>;

/// The service: which port it serves, for whom, and under which limits.
pub struct LayoutService<S: LayoutStore + ?Sized> {
    /// The person's descriptions.
    pub store: Rc<S>,
    /// `Some` only for the instance service; `None`: `admin` answers `unsupported{what=admin}`.
    pub admin: Option<Rc<dyn LayoutAdmin>>,
    /// The person this service serves; `""` as author on the wire means this one.
    pub person: Reference,
    /// The actor of the connection: until fs-access it is the person (the transport lets in the same OS user only); an AI seat
    /// later is its own actor.
    pub actor_of_connection: Rc<dyn Fn() -> Reference>,
    /// Called with the `by` a client claims in `admin(by)`; it is ignored for everything else (the actor is the connection's). The
    /// program hosting the service writes it to its log: a forged claim is visible, never effective.
    pub claimed_by: Option<ClaimNote>,
    /// The limits of the service (settings; rule 57).
    pub limits: ServiceLimits,
}

impl<S: LayoutStore + ?Sized> LayoutService<S> {
    /// A service over `store` for `person`, the actor being the person, the declared limits.
    #[must_use]
    pub fn new(store: Rc<S>, person: Reference) -> Self {
        let actor = person.clone();
        Self {
            store,
            admin: None,
            person,
            actor_of_connection: Rc::new(move || actor.clone()),
            claimed_by: None,
            limits: ServiceLimits::declared(),
        }
    }

    /// Builds `Who` from the wire `author`: `""` or the person is this service's person; another person is
    /// `unknown{what=author}` until fs-access, never silently replaced.
    pub(crate) fn who(&self, author: &str) -> Result<Who, Coded<LayoutCode>> {
        if author.is_empty() || author == self.person.0 {
            Ok(Who {
                actor: (self.actor_of_connection)(),
                author: self.person.clone(),
            })
        } else {
            Err(Coded::new(LayoutCode::Unknown).with("what", "author"))
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

/// What every capability of one service shares.
pub(crate) struct Shared<S: LayoutStore + ?Sized> {
    pub(crate) svc: LayoutService<S>,
    pub(crate) listeners: Rc<Listeners>,
}

impl<S: LayoutStore + ?Sized + 'static> LayoutService<S> {
    /// The root capability to hand to `fs_rpc::serve`: `Layouts`, and the handle that counts its watchers. The service has a hub
    /// of its own; for several sockets over ONE store use [`Self::bootstrap_in`] with one shared [`Hub`].
    #[must_use]
    pub fn bootstrap(self) -> (capnp::capability::Client, Watchers) {
        let hub = Hub::new();
        let watchers = hub.watchers();
        (self.bootstrap_in(&hub), watchers)
    }

    /// Like [`Self::bootstrap`], but the watchers live in `hub`: every service of the same `hub` tells its watchers about every
    /// write, whichever socket it came through. The limit `watchers_max` counts the watchers of the whole hub.
    #[must_use]
    pub fn bootstrap_in(self, hub: &Hub) -> capnp::capability::Client {
        let shared = Rc::new(Shared {
            svc: self,
            listeners: hub.0.clone(),
        });
        capnp_rpc::new_client::<crate::layouts_capnp::layouts::Client, _>(super::root::Root {
            shared,
        })
        .client
    }
}

impl<S: LayoutStore + ?Sized> Shared<S> {
    /// After a write that went through: tell the watchers the new stamp. If the store cannot say the stamp right after a write,
    /// the write stays answered (it happened); the watchers hear with the next change.
    pub(crate) async fn changed(&self) {
        self.listeners.heard(&self.svc.store.stamp().await);
    }
}
