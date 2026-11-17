//! The heartbeat: a program says "I am alive" or "I am busy with this" on a channel of its own, answered by a thread of its own.
//!
//! Pattern: Active object (the thread) + Observer (the client asks) + Strategy (`SaveOnTerm`: what the program saves when it is
//! about to be ended). Why: the control channel of the contract (`Control.version`) is answered by the thread that does the
//! work, so a program that works for a long time and a program that hangs look the same from outside. The heartbeat is answered
//! by its own thread (a plain Unix socket `<socket>.beat`, 0600, only this user, no Cap'n Proto), so a blocked work thread does not
//! silence it. A program that is busy says so with [`Heartbeat::busy`] (what: a text key like `importing`, progress: a number
//! that grows); [`ensure_single`](crate::ensure_single) never ends such a program while its progress moves. Unix only.
//! Lines on the wire: `ping` is answered `alive <tick-age-ms|->` or `busy <progress> <tick-age-ms|-> <what>`; `save` runs the hook and answers `saved`,
//! `nothing` (no hook) or `failed`.

mod client;
mod server;
mod types;
mod watch;

pub use client::{ask_beat, ask_save};
pub use server::Heartbeat;
pub use watch::{HangTimes, Hung, Verdict, Watch};

pub(crate) use crate::saved::Saved;
pub(crate) use types::clean_key;
pub use types::{Beat, Pulse, SaveOnTerm};
