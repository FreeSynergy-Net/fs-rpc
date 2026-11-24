//! The loop of the heartbeat thread on Unix: wait for a client, accept it. What goes wrong is said (stderr) and never ends the
//! thread without a word nor spins: a transient poll error is waited out, a fatal one is named, an accept error gets a pause.
//!
//! Pattern: Self-pipe trick (the poll) + Strategy (the poll, the accept and the pause are arguments, so the tests give faults that
//! cannot be made on a test machine, like `ENOMEM` and `EMFILE`).

use std::io;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixListener;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags};
use rustix::io::Errno;

/// The pause after a transient fault (poll `ENOMEM`/`EAGAIN`, accept `EMFILE`): short, so the end of the guard is still seen at once.
pub(super) const PAUSE: Duration = Duration::from_millis(100);

/// The system's poll without a timeout.
pub(super) fn system_poll(fds: &mut [PollFd<'_>]) -> rustix::io::Result<usize> {
    rustix::event::poll(fds, None)
}

/// Whether a poll error is transient (try again after a pause): out of memory or "try again". Others (`EBADF`, `EINVAL`, ...) are
/// the program's own bug and end the thread, said aloud.
const fn transient(e: Errno) -> bool {
    matches!(e, Errno::NOMEM | Errno::AGAIN)
}

/// Waits until a client is there (`true`) or the guard is dropped (`false`: the write end of `hangup` is closed, or the poll fails
/// for good, which is said on stderr).
pub(super) fn wait_for_client(
    listener: &UnixListener,
    hangup: &OwnedFd,
    poll: fn(&mut [PollFd<'_>]) -> rustix::io::Result<usize>,
    pause: &mut dyn FnMut(),
) -> bool {
    loop {
        let mut fds = [
            PollFd::new(listener, PollFlags::IN),
            PollFd::new(hangup, PollFlags::IN),
        ];
        match poll(&mut fds) {
            Ok(_) if !fds[1].revents().is_empty() => return false,
            Ok(_) => return true,
            Err(Errno::INTR) => {}
            Err(e) if transient(e) => pause(),
            Err(e) => {
                eprintln!("fs-rpc heartbeat: poll failed ({e}), the heartbeat ends");
                return false;
            }
        }
    }
}

/// Takes clients until none waits. An error other than "none waits" or an interrupt is said once and followed by a pause (a
/// dead-end like `EMFILE` would else make the poll return at once, for ever).
pub(super) fn drain_accepts<S>(
    mut accept: impl FnMut() -> io::Result<S>,
    mut serve: impl FnMut(S),
    pause: &mut dyn FnMut(),
) {
    loop {
        match accept() {
            Ok(stream) => serve(stream),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return,
            Err(e) => {
                eprintln!("fs-rpc heartbeat: accept failed ({e}), waiting before the next try");
                pause();
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "server_unix_loop_tests.rs"]
mod tests;
