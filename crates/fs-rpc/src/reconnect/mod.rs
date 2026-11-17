//! Staying connected: after a lost connection, a restarted server, an update or a take-over the client connects again,
//! subscribes again and tells the program that it must load everything anew.
//!
//! Pattern: Facade (declarations). The mechanism lives in [`keep_connected`] (Template method over the program's [`Session`]
//! and [`Resync`], with a [`Backoff`] between tries); what the program does with the hint is its own business (it compares
//! the `generation()` of its contract, then reloads or not).

mod backoff;
mod keep;

pub use backoff::{Backoff, ReconnectTimes};
pub use keep::{ReconnectEnd, Reconnected, Resync, Session, keep_connected};
