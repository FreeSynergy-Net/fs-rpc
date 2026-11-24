//! The loop of the heartbeat thread: faults the machine cannot be made to give are given by the arguments.
//!
//! Pattern: Test. Mutants (each turns its test red): M4 every poll error ends the wait (the old `Err(_) => return false`):
//! `a_transient_poll_error_does_not_end_the_wait`; M5 a transient error is not paused (busy loop): same test counts the pause;
//! M6 a fatal poll error is retried instead of ending: `a_fatal_poll_error_ends_the_wait`; M7 an accept error is not paused (the
//! old `while let Ok`): `an_accept_error_is_followed_by_a_pause`.

use std::io;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicUsize, Ordering};

use rustix::event::PollFd;
use rustix::io::Errno;

use super::{drain_accepts, system_poll, wait_for_client};

fn pair() -> (
    UnixListener,
    rustix::fd::OwnedFd,
    rustix::fd::OwnedFd,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(dir.path().join("l")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (hangup, wake) = rustix::pipe::pipe_with(rustix::pipe::PipeFlags::CLOEXEC).unwrap();
    (listener, hangup, wake, dir)
}

static CALLS: AtomicUsize = AtomicUsize::new(0);
static BAD_CALLS: AtomicUsize = AtomicUsize::new(0);

fn nomem_once_then_system(fds: &mut [PollFd<'_>]) -> rustix::io::Result<usize> {
    if CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
        Err(Errno::NOMEM)
    } else {
        system_poll(fds)
    }
}

fn bad_fd(fds: &mut [PollFd<'_>]) -> rustix::io::Result<usize> {
    // after two faults the real poll answers, so a loop that retries a fatal error ends in a failed assert, not in a hang
    if BAD_CALLS.fetch_add(1, Ordering::SeqCst) < 2 {
        Err(Errno::BADF)
    } else {
        system_poll(fds)
    }
}

#[test]
fn a_transient_poll_error_does_not_end_the_wait() {
    let (listener, hangup, _wake, dir) = pair();
    let _client = UnixStream::connect(dir.path().join("l")).unwrap();
    CALLS.store(0, Ordering::SeqCst);
    let mut paused = 0;
    let got = wait_for_client(&listener, &hangup, nomem_once_then_system, &mut || {
        paused += 1;
    });
    assert!(got, "ENOMEM once must not end the heartbeat thread");
    assert_eq!(paused, 1, "the transient error is waited out, not spun on");
}

#[test]
fn a_fatal_poll_error_ends_the_wait() {
    let (listener, hangup, _wake, dir) = pair();
    let _client = UnixStream::connect(dir.path().join("l")).unwrap();
    BAD_CALLS.store(0, Ordering::SeqCst);
    let got = wait_for_client(&listener, &hangup, bad_fd, &mut || {});
    assert!(!got);
    assert_eq!(
        BAD_CALLS.load(Ordering::SeqCst),
        1,
        "no retry of a fatal error"
    );
}

#[test]
fn an_accept_error_is_followed_by_a_pause() {
    let mut paused = 0;
    let mut served = 0;
    let mut calls = 0;
    drain_accepts(
        || {
            calls += 1;
            Err::<(), _>(io::Error::from_raw_os_error(Errno::MFILE.raw_os_error()))
        },
        |()| served += 1,
        &mut || paused += 1,
    );
    assert_eq!((paused, served, calls), (1, 0, 1));
}

#[test]
fn no_client_waiting_and_an_interrupt_are_no_faults() {
    let mut paused = 0;
    let mut step = 0;
    drain_accepts(
        || {
            step += 1;
            match step {
                1 => Ok(1),
                2 => Err(io::Error::from(io::ErrorKind::Interrupted)),
                3 => Ok(2),
                _ => Err(io::Error::from(io::ErrorKind::WouldBlock)),
            }
        },
        |_: i32| {},
        &mut || paused += 1,
    );
    assert_eq!((paused, step), (0, 4));
}
