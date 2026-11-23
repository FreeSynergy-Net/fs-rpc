//! The program's end of the heartbeat: one thread, one channel.
//!
//! Pattern: Active object + RAII. [`Heartbeat::start`] opens the channel next to `socket` (on Unix `<socket>.beat`, 0600; on
//! Windows the pipe `<pipe>.beat`, open to this user alone) and answers on a thread of its own, one short job per connection, so a
//! slow `save` does not block a ping. Dropping it stops the thread and removes the channel. Only this user is answered. The
//! lines on the wire are the same on every system (see the module).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use super::wire::{Guard, open};
use super::{Beat, SaveOnTerm, Saved, clean_key};

pub(super) struct Shared {
    beat: Mutex<Option<(String, u64)>>,
    hook: Mutex<Option<Arc<dyn SaveOnTerm>>>,
    tick: Mutex<Instant>,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    pub(super) fn new() -> Self {
        Self {
            beat: Mutex::new(None),
            hook: Mutex::new(None),
            tick: Mutex::new(Instant::now()),
        }
    }

    fn state(&self) -> Beat {
        match locked(&self.beat).clone() {
            Some((what, progress)) => Beat::Busy { what, progress },
            None => Beat::Alive,
        }
    }

    fn tick_text(&self) -> String {
        locked(&self.tick).elapsed().as_millis().to_string()
    }

    pub(super) fn save(&self) -> Saved {
        let hook = locked(&self.hook).clone();
        hook.map_or(Saved::Nothing, |h| h.save())
    }

    /// The answer line to a request line, or `None` for a line that is none (the connection is just closed).
    #[cfg_attr(not(any(unix, windows)), allow(dead_code))]
    pub(super) fn answer(&self, line: &str) -> Option<String> {
        match line.trim() {
            "ping" => Some(match self.state() {
                Beat::Alive => format!("alive {}", self.tick_text()),
                Beat::Busy { what, progress } => {
                    format!("busy {progress} {} {what}", self.tick_text())
                }
            }),
            "save" => Some(self.save().token().to_owned()),
            _ => None,
        }
    }
}

/// The heartbeat of a program; keep it as long as the program runs.
pub struct Heartbeat {
    shared: Arc<Shared>,
    path: PathBuf,
    _guard: Guard,
}

impl Heartbeat {
    /// Start the heartbeat next to `socket` (call it after the server's own socket is bound).
    ///
    /// # Errors
    /// Another heartbeat answers on the path (`AddrInUse`), the file system refuses, or the system has no heartbeat channel
    /// (`Unsupported`).
    pub fn start(socket: &Path) -> io::Result<Self> {
        let shared = Arc::new(Shared::new());
        let (path, guard) = open(socket, &shared)?;
        Ok(Self {
            shared,
            path,
            _guard: guard,
        })
    }

    /// The program is busy: `what` is a text key (`importing`), `progress` a number that grows with the work. Call it again
    /// with the new number as the work goes on; a number that stands still for the limit makes the program count as hanging.
    pub fn busy(&self, what: &str, progress: u64) {
        *locked(&self.shared.beat) = Some((clean_key(what), progress));
    }

    /// The program is not busy any more.
    pub fn idle(&self) {
        *locked(&self.shared.beat) = None;
    }

    /// The work thread (the window's loop, the server's loop) MUST call this at every turn: it says "I am not stuck". The age counts from
    /// `start`, so a program that never calls it stands still for the limit (10 s) as soon as its contract is silent. The heartbeat
    /// thread answers even if the work thread is blocked, so the age of the last tick is what tells a deadlock from health.
    pub fn tick(&self) {
        *locked(&self.shared.tick) = Instant::now();
    }

    /// What to save before the program is ended (also run when a client asks `save`).
    pub fn on_save(&self, hook: impl SaveOnTerm + 'static) {
        *locked(&self.shared.hook) = Some(Arc::new(hook));
    }

    /// Run the hook now (the program's own stop path calls it, so a clean stop and a take-over save the same).
    #[must_use]
    pub fn save_now(&self) -> Saved {
        self.shared.save()
    }

    /// The channel of the heartbeat: the socket file on Unix, the pipe name on Windows.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
impl Shared {
    /// The hook slot, for the tests that give a hook to a `Shared` without a `Heartbeat`.
    pub(super) fn hook_for_test(&self) -> MutexGuard<'_, Option<Arc<dyn SaveOnTerm>>> {
        locked(&self.hook)
    }
}
