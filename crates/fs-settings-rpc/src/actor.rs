//! Who acts on a connection: a person or a bot ([`Actor`] of fs-settings), told by the CONNECTION and by nothing else
//! ([AKTEUR-AUS-VERBINDUNG]): never a field of the client, never an argument or the environment. The service asks the port
//! [`ActorOfConnection`] once per connection and hands the answer to every capability of that connection.
//!
//! Today `fs_rpc::Caller` has an id and a name and nothing that tells a bot (a bot is told apart by its key later, fs-rpc
//! `transport/peer.rs`). So the default, [`FailClosed`], names EVERY connection a bot: a key declared
//! `Declaration::human_only` does not exist for it, in every way ([`crate::SettingsService`] leads each of them through
//! `fs_settings::Surface`). A host that can tell a person says so by its own port, e.g. [`OsUserIsPerson`] (the transport admits the
//! service's own OS user only; a program that runs as that user is a person for the module, ADR 0054 open point 1: not prevented
//! and not claimed) or, later, one that reads the bot key.
//!
//! Pattern: Strategy (the port) + Null object that fails closed.

use fs_rpc::Caller;
pub use fs_settings::Actor;

/// The actor of a connection, from the connection alone.
pub trait ActorOfConnection: Send + Sync {
    /// Who is on the other end of `caller`'s connection. Anything that is not proven a person must be [`Actor::Bot`].
    fn actor(&self, caller: &Caller) -> Actor;
}

/// The default: no connection is proven a person, so every one is a bot (fail closed).
#[derive(Clone, Copy, Debug, Default)]
pub struct FailClosed;

impl ActorOfConnection for FailClosed {
    fn actor(&self, _: &Caller) -> Actor {
        Actor::Bot
    }
}

/// A connection of the service's own OS user is a person; one whose id cannot make an actor is a bot. Not the default: the host chooses
/// it on purpose.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsUserIsPerson;

impl ActorOfConnection for OsUserIsPerson {
    fn actor(&self, caller: &Caller) -> Actor {
        if crate::identity::id_usable(caller.id()) {
            Actor::Person
        } else {
            Actor::Bot
        }
    }
}

impl<F: Fn(&Caller) -> Actor + Send + Sync> ActorOfConnection for F {
    fn actor(&self, caller: &Caller) -> Actor {
        self(caller)
    }
}
