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
mod watch;

pub use client::{ask_beat, ask_save};
pub use server::Heartbeat;
pub use watch::{HangTimes, Hung, Verdict, Watch};

/// The answer to a ping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Beat {
    /// The program is alive and not busy.
    Alive,
    /// The program is busy and says with what; it is not hanging while `progress` moves.
    Busy {
        /// What it does: a text key (`[a-z0-9-]`, at most 64 characters), never a sentence; the program words it (MF2).
        what: String,
        /// A number that grows with the work (items done, bytes read).
        progress: u64,
    },
}

/// A ping's answer with the last beat of the program's own work: `tick_age` is how long ago the work thread (the window's or
/// the server's loop) called [`Heartbeat::tick`]; `None` if the program never ticks (then only the other rules judge it). A heartbeat
/// thread that answers while the work thread stands still is how a deadlock looks: `Alive` with a tick age that grows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pulse {
    /// Alive or busy.
    pub beat: Beat,
    /// Time since the last tick of the work thread, if the program ticks.
    pub tick_age: Option<std::time::Duration>,
}

pub(crate) use crate::saved::Saved;

/// What a program saves before it is ended: open drafts, the session. It runs on the heartbeat thread, so it must work from
/// state that is shared (a `Mutex` around the drafts), never from the thread that may be blocked. Give it a short run.
pub trait SaveOnTerm: Send + Sync {
    /// Save what would be lost.
    fn save(&self) -> Saved;
}

impl<F: Fn() -> Saved + Send + Sync> SaveOnTerm for F {
    fn save(&self) -> Saved {
        self()
    }
}

/// A text key as the wire carries it: lower-case letters, digits and `-`, at most 64 characters; anything else becomes `-`.
pub(crate) fn clean_key(text: &str) -> String {
    let key: String = text
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect();
    if key.is_empty() { "-".to_owned() } else { key }
}
