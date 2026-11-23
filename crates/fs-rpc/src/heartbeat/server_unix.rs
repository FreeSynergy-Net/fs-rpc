//! The heartbeat's channel on Unix: the socket `<socket>.beat` (0600, in the socket's folder, which is private), one thread that
//!
//! Pattern: Active object + RAII (the thread and the socket file of the heartbeat on Unix).
//! accepts, one short thread per connection. Only this user is answered (`SO_PEERCRED`).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use super::server::Shared;
use crate::companion_path;

fn is_me(stream: &UnixStream) -> bool {
    rustix::net::sockopt::socket_peercred(stream)
        .is_ok_and(|c| c.uid.as_raw() == rustix::process::geteuid().as_raw())
}

fn serve_one(shared: &Shared, stream: UnixStream, admit: Admit) {
    if !admit(&stream) {
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
    if let Some(answer) = shared.answer(&line) {
        let _ = writeln!(out, "{answer}");
    }
}

/// Holds the thread and the file: dropping it stops the thread and removes the file.
pub(super) struct Guard {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// A check of the process on the other end of a connection: `true` if it is this user.
pub(super) type Admit = fn(&UnixStream) -> bool;

pub(super) fn open(socket: &Path, shared: &Arc<Shared>) -> io::Result<(PathBuf, Guard)> {
    open_with(socket, shared, is_me)
}

/// [`open`] with the check of the client as an argument (the test of the refusal gives a check that says no).
pub(super) fn open_with(
    socket: &Path,
    shared: &Arc<Shared>,
    admit: Admit,
) -> io::Result<(PathBuf, Guard)> {
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
                            .spawn(move || serve_one(&shared, stream, admit));
                    }
                }
            })?
    };
    Ok((
        path.clone(),
        Guard {
            path,
            stop,
            thread: Some(thread),
        },
    ))
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.path); // wakes the thread
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "server_unix_tests.rs"]
mod tests;
