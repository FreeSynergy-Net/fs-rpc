//! The words of the heartbeat: the answer to a ping, the pulse with the tick age, the save hook.
//!
//! Pattern: Value objects + Strategy (`SaveOnTerm`).

use crate::saved::Saved;

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
