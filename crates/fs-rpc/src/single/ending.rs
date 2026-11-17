//! Ending a server of this program that holds the socket and does not answer, safely (Linux).
//!
//! Pattern: Command guarded by `rule::decide`, over an `Env` (the system: facts, `pidfd`, heartbeat, log), so the order of the steps and
//! every guard are tested with a made-up system. The steps: look (rule) -> ask the program to save (heartbeat `save`) -> hold the
//! process with a `pidfd` -> look again (the number cannot mean another process now) -> SIGTERM -> wait -> only if it still
//! stands and no heartbeat answers: look once more, SIGKILL. A heartbeat that still answers is never killed. The socket file is
//! removed only if it is the very file the ended process listened on. Every step is written to `<socket>.takeover.log` (0600).

use std::io::Write;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::time::{Duration, Instant};

use rustix::process::{Pid, PidfdFlags, Signal, pidfd_open, pidfd_send_signal};

use super::error::{SingleError, Stuck, TakenOver};
use super::procfs::{self, PROC};
use super::rule::{Decision, Own, Peer, decide};
use crate::{Pulse, Saved};

/// The process that was seen: its number and its start time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub pid: i32,
    pub start: u64,
}

/// A process held through its `pidfd`: signals reach this process, whatever its number becomes.
pub(super) trait Held {
    fn term(&self);
    fn kill(&self);
}

/// What the ending needs from the system.
pub(super) trait Env {
    fn peer(&self, pid: i32, uid: u32) -> Peer;
    fn hold(&self, pid: i32) -> std::io::Result<Option<Box<dyn Held>>>;
    fn gone(&self, target: &Target) -> bool;
    async fn save(&self) -> Saved;
    async fn beat(&self) -> Option<Pulse>;
    fn log(&self, line: &str);
}

/// The waits of the ending.
#[derive(Clone, Copy, Debug)]
pub(super) struct Waits {
    pub term: Duration,
    pub kill: Duration,
}

fn stuck(peer: &Peer, why: &'static str, limit: Duration) -> SingleError {
    SingleError::Unresponsive(Stuck {
        pid: u32::try_from(peer.pid).ok(),
        why,
        exe: peer.exe.as_ref().map(|e| e.path.clone()),
        limit,
    })
}

/// Look at the process again and accept it for ending: this program of this user, the same process as seen. Another program is
/// never accepted.
fn accepted<E: Env>(
    env: &E,
    target: &Target,
    uid: u32,
    own: &Own,
    limit: Duration,
) -> Result<Peer, SingleError> {
    let peer = env.peer(target.pid, uid);
    match decide(Some(target.start), &peer, own) {
        Decision::TakeOver => Ok(peer),
        Decision::Refuse(why) => Err(stuck(&peer, why.code(), limit)),
    }
}

async fn gone_within<E: Env>(env: &E, target: &Target, limit: Duration) -> bool {
    let until = Instant::now() + limit;
    loop {
        if env.gone(target) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// End the process `target`. `uid` is what the socket said about its user; `limit` only names the wait in a failure. `channel`
/// says that the program has a heartbeat channel at all: a program without one (older, or a window before it has a control
/// channel) gets NO signal at all (nothing could be saved first): the failure says "please close it" (`no-control`).
///
/// # Errors
/// The process is not one that may be ended (`Unresponsive` with the reason), still answers its heartbeat, or does not end.
pub(super) async fn end_process<E: Env>(
    env: &E,
    target: &Target,
    uid: u32,
    own: &Own,
    waits: Waits,
    limit: Duration,
    channel: bool,
) -> Result<TakenOver, SingleError> {
    let pid = u32::try_from(target.pid).unwrap_or(0);
    let first = accepted(env, target, uid, own, limit)?;
    if !channel {
        // No heartbeat channel: nothing can be saved first, and a signal would lose work. Only the report.
        env.log(&format!(
            "pid={pid} has no heartbeat channel: no signal sent, the person is told"
        ));
        return Err(stuck(&first, "no-control", limit));
    }
    // First what would be lost: the program saves, on the thread of its heartbeat. No signal has been sent yet.
    let saved = env.save().await;
    let Some(held) = env
        .hold(target.pid)
        .map_err(|_| stuck(&env.peer(target.pid, uid), "unreadable", limit))?
    else {
        env.log(&format!(
            "pid={pid} gone before it was held; saved={}",
            saved.token()
        ));
        return Ok(TakenOver {
            pid,
            saved,
            how: "gone",
        });
    };
    // Looked at again with the descriptor in hand: from now on the number cannot mean another process.
    let peer = accepted(env, target, uid, own, limit)?;
    let exe = peer.exe.as_ref().map_or("?", |e| e.path.as_str());
    env.log(&format!(
        "take-over pid={pid} start={} uid={} exe={exe} saved={}",
        target.start,
        peer.uid,
        saved.token()
    ));
    held.term();
    let how = if gone_within(env, target, waits.term).await {
        "sigterm"
    } else {
        // Still there. The facts again, and the heartbeat: a program that still answers (or reports progress) is not killed.
        accepted(env, target, uid, own, limit)?;
        if env.beat().await.is_some() {
            env.log(&format!(
                "pid={pid} did not end after SIGTERM but its heartbeat answers: not killed"
            ));
            return Err(stuck(&peer, "beating", limit));
        }
        held.kill();
        if !gone_within(env, target, waits.kill).await {
            return Err(stuck(&peer, "not-ending", limit));
        }
        "sigkill"
    };
    env.log(&format!("ended pid={pid} by {how}"));
    Ok(TakenOver { pid, saved, how })
}

/// Remove the socket file if it is the very file `ino` that the ended process listened on; a successor's socket stays.
pub(super) fn remove_if_same(socket: &Path, ino: Option<u64>) -> bool {
    let Some(ino) = ino else { return false };
    match std::fs::symlink_metadata(socket) {
        Ok(meta) if meta.file_type().is_socket() && meta.ino() == ino => {
            std::fs::remove_file(socket).is_ok()
        }
        _ => false,
    }
}

/// The real system.
pub(super) struct Real<'a> {
    pub socket: &'a Path,
    /// The process that listens on the socket: the only one whose heartbeat is believed.
    pub pid: i32,
    pub call: Duration,
    pub save: Duration,
}

struct Fd(std::os::fd::OwnedFd);

impl Held for Fd {
    fn term(&self) {
        let _ = pidfd_send_signal(&self.0, Signal::TERM);
    }
    fn kill(&self) {
        let _ = pidfd_send_signal(&self.0, Signal::KILL);
    }
}

impl Env for Real<'_> {
    fn peer(&self, pid: i32, uid: u32) -> Peer {
        procfs::peer_in(Path::new(PROC), pid, uid)
    }

    fn hold(&self, pid: i32) -> std::io::Result<Option<Box<dyn Held>>> {
        let Some(pid) = Pid::from_raw(pid) else {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput));
        };
        match pidfd_open(pid, PidfdFlags::empty()) {
            Ok(fd) => Ok(Some(Box::new(Fd(fd)))),
            Err(rustix::io::Errno::SRCH) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn gone(&self, target: &Target) -> bool {
        procfs::is_gone(Path::new(PROC), target.pid, target.start)
    }

    async fn save(&self) -> Saved {
        crate::heartbeat::ask_save_from(self.socket, self.save, Some(self.pid))
            .await
            .unwrap_or(Saved::Nothing)
    }

    async fn beat(&self) -> Option<Pulse> {
        crate::heartbeat::ask_beat_from(self.socket, self.call, Some(self.pid))
            .await
            .ok()
    }

    fn log(&self, line: &str) {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true).mode(0o600);
        if let Ok(mut file) = options.open(self.socket.with_extension("takeover.log")) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let _ = writeln!(file, "{} {line}", procfs::utc(now));
        }
    }
}
