//! Rights in the module (Stufe 4, part 2): the wrapper [`Guarded`] that asks the rules of a module BEFORE every change.
//! Pattern: Facade (declarations); the pattern of the parts stands in their own files.

mod blob;
mod conn;
mod hub;
mod ports;
mod steps;
mod wire;
mod wrapper;

use wrapper::Clock;

pub use hub::WatchHub;
pub use ports::{ActorOfConnection, AllBots, FixedPerson, PersonOfConnection, Ports};
pub use wrapper::{Guarded, REVERT_COMMAND, WATCH_COMMAND};
