//! What the connection tells about the caller: ports of the wrapper.
//!
//! Pattern: Strategy (the ports) + Null object that fails closed. The answers come from the CONNECTION (the system says who),
//! never from an argument of a call.

use fs_interfaces::module::Actor;
use fs_interfaces::owner::{LocalOwner, OwnerProblem};
use fs_types::PersonRef;

use crate::transport::Caller;

/// Pattern: Strategy - the person a connection acts for, from the connection alone.
pub trait PersonOfConnection {
    /// The person of `caller`'s connection, or `None` if it is not known: the connection then gets nothing (fail closed).
    fn person(&self, caller: &Caller) -> Option<PersonRef>;
}

impl<F: Fn(&Caller) -> Option<PersonRef>> PersonOfConnection for F {
    fn person(&self, caller: &Caller) -> Option<PersonRef> {
        self(caller)
    }
}

/// Pattern: Strategy - who acts on a connection: a person or a bot, from the connection alone.
pub trait ActorOfConnection {
    /// Anything that is not proven a person must be [`Actor::Bot`].
    fn actor(&self, caller: &Caller) -> Actor;
}

impl<F: Fn(&Caller) -> Actor> ActorOfConnection for F {
    fn actor(&self, caller: &Caller) -> Actor {
        self(caller)
    }
}

/// The default: no connection is proven a person, so every one is a bot (fail closed).
#[derive(Clone, Copy, Debug, Default)]
pub struct AllBots;

impl ActorOfConnection for AllBots {
    fn actor(&self, _: &Caller) -> Actor {
        Actor::Bot
    }
}

/// The ports of a [`Guarded`](super::Guarded) module.
#[non_exhaustive]
pub struct Ports {
    /// Person or bot.
    pub actor: Box<dyn ActorOfConnection>,
    /// The person behind the connection (a single place: the owner of the device; later from a token).
    pub person: Box<dyn PersonOfConnection>,
}

impl Ports {
    /// Ports from an actor source and a person source.
    #[must_use]
    pub fn new(
        actor: impl ActorOfConnection + 'static,
        person: impl PersonOfConnection + 'static,
    ) -> Self {
        Self {
            actor: Box::new(actor),
            person: Box::new(person),
        }
    }
}

/// Pattern: Strategy (a constant) - the person is the same for every connection: a module that is held in the program itself
/// ([`hold_in_process`](crate::hold_in_process)) has one caller, the person of the device. Deviation: not read from the
/// connection, because there is no socket to read it from; the person is fixed by the program that builds the ports.
#[derive(Clone, Debug)]
pub struct FixedPerson(pub PersonRef);

impl PersonOfConnection for FixedPerson {
    fn person(&self, _: &Caller) -> Option<PersonRef> {
        Some(self.0.clone())
    }
}

/// Pattern: Adapter (`LocalOwner` -> `PersonOfConnection`) - on a single place (ADR 0090) every connection the transport admits acts for
/// the one owner of the device. The owner is read ONCE, in the constructor; no owner is no start (the constructor says why). Open card:
/// uid <-> anchor before V1 (then the person comes from the connection, not from the place).
#[derive(Clone, Debug)]
pub struct LocalPerson(PersonRef);

impl LocalPerson {
    /// Read the owner of `owner` once.
    ///
    /// # Errors
    /// The [`OwnerProblem`] of the source (no "me", unreachable, invalid): the program must not start then.
    pub fn new(owner: &dyn LocalOwner) -> Result<Self, OwnerProblem> {
        owner.owner().map(Self)
    }
}

impl PersonOfConnection for LocalPerson {
    fn person(&self, _: &Caller) -> Option<PersonRef> {
        Some(self.0.clone())
    }
}

/// Is the id of a caller something an actor can be made of: not empty, and only letters, digits, `-`, `_`, `.` (a Unix user id, a Windows
/// SID)?
#[must_use]
pub fn id_usable(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Pattern: Strategy - a connection the transport admitted is the service's own OS user, so a person; one whose id cannot make an
/// actor ([`id_usable`]) is a bot. Not the default (that is [`AllBots`]): the host chooses it on purpose (ADR 0054 open point 1: a
/// program that runs as that user is a person for the module, not prevented and not claimed).
#[derive(Clone, Copy, Debug, Default)]
pub struct OsUserIsPerson;

impl ActorOfConnection for OsUserIsPerson {
    fn actor(&self, caller: &Caller) -> Actor {
        if id_usable(caller.id()) {
            Actor::Person
        } else {
            Actor::Bot
        }
    }
}
