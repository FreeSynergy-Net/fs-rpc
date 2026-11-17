//! The program's end of the heartbeat: one thread, one socket.
//!
//! Pattern: Active object + RAII. [`Heartbeat::start`] binds `<socket>.beat` (0600, in the socket's folder, which is private) and
//! answers on a thread of its own, one short thread per connection, so a slow `save` does not block a ping. Dropping it stops the
//! thread and removes the file. Only this user is answered (`SO_PEERCRED`).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::{Beat, SaveOnTerm, Saved, clean_key};
use crate::companion_path;

#[derive(Default)]
struct Shared {
    beat: Mutex<Option<(String, u64)>>,
    hook: Mutex<Option<Arc<dyn SaveOnTerm>>>,
    tick: Mutex<Option<Instant>>,
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    fn state(&self) -> Beat {
        match locked(&self.beat).clone() {
            Some((what, progress)) => Beat::Busy { what, progress },
            None => Beat::Alive,
        }
    }

    fn tick_text(&self) -> String {
        locked(&self.tick).map_or_else(|| "-".to_owned(), |t| t.elapsed().as_millis().to_string())
    }

    fn save(&self) -> Saved {
        let hook = locked(&self.hook).clone();
        hook.map_or(Saved::Nothing, |h| h.save())
    }
}

/// The heartbeat of a program; keep it as long as the program runs.
pub struct Heartbeat {
    shared: Arc<Shared>,
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn is_me(stream: &UnixStream) -> bool {
    rustix::net::sockopt::socket_peercred(stream)
        .is_ok_and(|c| c.uid.as_raw() == rustix::process::geteuid().as_raw())
}

fn serve_one(shared: &Shared, stream: UnixStream) {
    if !is_me(&stream) {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut line = String::new();
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    if BufReader::new(stream.take(64))
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let answer = match line.trim() {
        "ping" => match shared.state() {
            Beat::Alive => format!("alive {}", shared.tick_text()),
            Beat::Busy { what, progress } => {
                format!("busy {progress} {} {what}", shared.tick_text())
            }
        },
        "save" => shared.save().token().to_owned(),
        _ => return,
    };
    let _ = writeln!(out, "{answer}");
}

impl Heartbeat {
    /// Start the heartbeat next to `socket` (call it after the server's own socket is bound).
    ///
    /// # Errors
    /// Another heartbeat answers on the path, or the file system refuses.
    pub fn start(socket: &Path) -> io::Result<Self> {
        let path = companion_path(socket, "beat");
        if path.exists() {
            if UnixStream::connect(&path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    "a heartbeat answers already",
                ));
            }
            std::fs::remove_file(&path)?; // nobody answers: a leftover of an ended process
        }
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, PermissionsExt::from_mode(0o600))?;
        let shared = Arc::new(Shared::default());
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (shared, stop) = (shared.clone(), stop.clone());
            std::thread::Builder::new()
                .name("fs-heartbeat".into())
                .spawn(move || {
                    for stream in listener.incoming() {
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                        if let Ok(stream) = stream {
                            let shared = shared.clone();
                            let _ = std::thread::Builder::new()
                                .name("fs-heartbeat-call".into())
                                .spawn(move || serve_one(&shared, stream));
                        }
                    }
                })?
        };
        Ok(Self {
            shared,
            path,
            stop,
            thread: Some(thread),
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

    /// The work thread (the window's loop, the server's loop) calls this at every turn: it says "I am not stuck". The heartbeat
    /// thread answers even if the work thread is blocked, so the age of the last tick is what tells a deadlock from health.
    pub fn tick(&self) {
        *locked(&self.shared.tick) = Some(Instant::now());
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

    /// The socket file of the heartbeat.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Heartbeat {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.path); // wakes the thread
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}
