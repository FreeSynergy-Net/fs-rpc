//! The heartbeat's channel on Unix: the socket `<socket>.beat` (0600, in the socket's folder, which is private), one thread that
//! accepts, one short thread per connection. Only this user is answered (`SO_PEERCRED`).
//!
//! Pattern: Active object + RAII (the thread and the socket file of the heartbeat on Unix). The thread is ended by a pipe it polls
//! together with the listener (Self-pipe trick, instead of a connect to the file that fails when the file is gone), so `drop` never
//! waits without end.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    let _ = stream.set_nonblocking(false); // some systems hand on the flag of the listener
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

/// Waits until a client is there (`true`) or the guard is dropped (`false`: the write end of `hangup` is closed).
fn wait_for_client(listener: &UnixListener, hangup: &OwnedFd) -> bool {
    use rustix::event::{PollFd, PollFlags, poll};
    loop {
        let mut fds = [
            PollFd::new(listener, PollFlags::IN),
            PollFd::new(hangup, PollFlags::IN),
        ];
        match poll(&mut fds, None) {
            Ok(_) if !fds[1].revents().is_empty() => return false,
            Ok(_) => return true,
            Err(rustix::io::Errno::INTR) => {}
            Err(_) => return false,
        }
    }
}

/// Holds the thread and the file: dropping it stops the thread and removes the file.
pub(super) struct Guard {
    path: PathBuf,
    wake: Option<OwnedFd>, // the write end of the pipe the thread polls: closing it ends the thread
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
    listener.set_nonblocking(true)?;
    let (hangup, wake) = rustix::pipe::pipe()?;
    let thread = {
        let shared = shared.clone();
        std::thread::Builder::new()
            .name("fs-heartbeat".into())
            .spawn(move || {
                while wait_for_client(&listener, &hangup) {
                    while let Ok((stream, _)) = listener.accept() {
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
            wake: Some(wake),
            thread: Some(thread),
        },
    ))
}

impl Drop for Guard {
    fn drop(&mut self) {
        // Closing the write end ends the poll of the thread: no connect to the file is needed (the file may be gone or foreign).
        drop(self.wake.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
#[path = "server_unix_tests.rs"]
mod tests;
