//! The silent server at a socket: who is it, does it work, may it be ended (Linux).
//!
//! Pattern: Probe loop with a verdict (`heartbeat::Watch`) in front of the guarded ending. Another program is reported at once.
//! A program of this kind is asked at the pace of `hang.ping`: it answers its contract (then it is simply there), it is gone,
//! it is busy and moves (it is never ended, the busy state is handed up), or it hangs: then it is saved, ended and replaced.

use std::cell::RefCell;
use std::path::Path;
use std::time::Instant;

use super::SingleTimes;
use super::ending::{Real, Target, Waits, end_process};
use super::error::{SingleError, Stuck, TakenOver};
use super::procfs;
use super::rule::{Decision, decide};
use crate::ask_beat;
use crate::handshake::{ask, is_absent};
use crate::heartbeat::{Verdict, Watch};

/// What `free_socket` found.
pub(super) enum Freed {
    /// The server answers now.
    Ready,
    /// The socket is free; `Some` if a process was ended for it.
    Free(Option<TakenOver>),
}

/// A busy server seen on the way (what, progress, pid), for the failure when the time runs out.
pub(super) type BusySeen = RefCell<Option<(String, u64, Option<u32>)>>;

enum Listener {
    Gone,
    Unknown,
    Process(i32, u32),
}

/// Who listens at `socket`: asked through a connection (the kernel tells the process that called `listen`).
async fn listener(socket: &Path, call: std::time::Duration) -> Listener {
    match tokio::time::timeout(call, tokio::net::UnixStream::connect(socket)).await {
        Err(_) => Listener::Unknown,
        Ok(Err(_)) => Listener::Gone,
        Ok(Ok(stream)) => match stream.peer_cred() {
            Ok(cred) => cred
                .pid()
                .map_or(Listener::Unknown, |pid| Listener::Process(pid, cred.uid())),
            Err(_) => Listener::Unknown,
        },
    }
}

fn unknown(times: &SingleTimes) -> SingleError {
    SingleError::Unresponsive(Stuck {
        pid: None,
        why: "unreadable",
        exe: None,
        limit: times.overall,
    })
}

/// Look at the silent server and, if it is this program of this user and hangs, save it, end it and remove its socket file.
///
/// # Errors
/// `Unresponsive` when it may not be ended, `Busy` is told through `busy` (the caller reports it when its time runs out).
pub(super) async fn free_socket(
    socket: &Path,
    times: &SingleTimes,
    exe: Option<&Path>,
    busy: &BusySeen,
) -> Result<Freed, SingleError> {
    let call = times.handshake.call;
    let (pid, uid) = match listener(socket, call).await {
        Listener::Gone => return Ok(Freed::Free(None)),
        Listener::Unknown => return Err(unknown(times)),
        Listener::Process(pid, uid) => (pid, uid),
    };
    let Some(own) = procfs::own(exe) else {
        return Err(unknown(times));
    };
    let first = procfs::peer_in(Path::new(procfs::PROC), pid, uid);
    let Some(start) = first.start else {
        return Err(unknown(times));
    };
    // Another program, another user, an unreadable one: reported at once, nothing else is done (no waiting, no `save`, no signal).
    if let Decision::Refuse(why) = decide(Some(start), &first, &own) {
        return Err(SingleError::Unresponsive(Stuck {
            pid: u32::try_from(pid).ok(),
            why: why.code(),
            exe: first.exe.as_ref().map(|e| e.path.clone()),
            limit: times.overall,
        }));
    }
    let socket_ino = std::fs::symlink_metadata(socket)
        .ok()
        .map(|m| std::os::unix::fs::MetadataExt::ino(&m));
    // Does the program have a heartbeat channel at all? Without one it is never killed (only asked to end, then reported).
    let channel = crate::companion_path(socket, "beat").exists();
    let mut watch = Watch::new(times.hang, Instant::now());
    loop {
        match ask::<capnp::capability::Client>(socket, call).await {
            Ok(_) => return Ok(Freed::Ready),
            Err(e) if is_absent(&e) => return Ok(Freed::Free(None)),
            Err(_) => {}
        }
        let pulse = ask_beat(socket, call).await.ok();
        match watch.observe(Instant::now(), pulse.as_ref()) {
            Verdict::Hung(_) => break,
            Verdict::Busy { what, progress } => {
                *busy.borrow_mut() = Some((what, progress, u32::try_from(pid).ok()));
            }
            Verdict::Waiting | Verdict::Alive => {}
        }
        tokio::time::sleep(times.hang.ping).await;
    }
    let real = Real {
        socket,
        call,
        save: times.save,
    };
    let waits = Waits {
        term: times.term,
        kill: times.kill,
    };
    let taken = end_process(
        &real,
        &Target { pid, start },
        uid,
        &own,
        waits,
        times.overall,
        channel,
    )
    .await?;
    super::ending::remove_if_same(socket, socket_ino);
    Ok(Freed::Free(Some(taken)))
}
