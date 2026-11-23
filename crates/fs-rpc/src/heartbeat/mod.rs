//! The heartbeat: a program says "I am alive" or "I am busy with this" on a channel of its own, answered by a thread of its own.
//!
//! Pattern: Active object (the thread) + Observer (the client asks) + Strategy (`SaveOnTerm`: what the program saves when it is
//! about to be ended; the wire of the system). Why: the control channel of the contract (`Control.version`) is answered by the thread that does the
//! work, so a program that works for a long time and a program that hangs look the same from outside. The heartbeat is answered
//! by its own thread, so a blocked work thread does not silence it: on Unix a plain socket `<socket>.beat` (0600, only this user),
//! on Windows its own pipe `<pipe>.beat` (open to this user alone, a `current_thread` runtime of its own); no Cap'n Proto. A program that
//! is busy says so with [`Heartbeat::busy`] (what: a text key like `importing`, progress: a number that grows);
//! [`ensure_single`](crate::ensure_single) never ends such a program while its progress moves. A system with neither
//! way answers [`BeatError::Unsupported`], never a silent `Ok`.
//! Lines on the wire (the same everywhere): `ping` is answered `alive <tick-age-ms|->` or `busy <progress> <tick-age-ms|-> <what>`; `save` runs the hook and answers `saved`,
//! `nothing` (no hook) or `failed`.

mod client;
#[cfg(not(any(unix, windows)))]
mod client_other;
#[cfg(unix)]
mod client_unix;
#[cfg(windows)]
mod client_windows;
mod server;
#[cfg(not(any(unix, windows)))]
mod server_other;
#[cfg(unix)]
mod server_unix;
#[cfg(windows)]
mod server_windows;
mod types;
mod watch;

/// The wire of this system: the client's `exchange` and the server's `open` with its `Guard`.
mod wire {
    #[cfg(not(any(unix, windows)))]
    pub(super) use super::{
        client_other::exchange,
        server_other::{Guard, open},
    };
    #[cfg(unix)]
    pub(super) use super::{
        client_unix::exchange,
        server_unix::{Guard, open},
    };
    #[cfg(windows)]
    pub(super) use super::{
        client_windows::exchange,
        server_windows::{Guard, open},
    };
}

pub use client::{BeatError, ask_beat, ask_save};
pub(crate) use client::{ask_beat_from, ask_save_from};
pub use server::Heartbeat;
pub use watch::{HangTimes, Hung, Verdict, Watch};

pub(crate) use crate::saved::Saved;
pub(crate) use types::clean_key;
pub use types::{Beat, Pulse, SaveOnTerm};
