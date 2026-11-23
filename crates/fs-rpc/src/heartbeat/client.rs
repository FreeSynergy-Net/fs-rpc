//! Asking a heartbeat: the other end, async, every step with a limit.
//!
//! Pattern: Request-reply on a plain line protocol; the wire (a Unix socket, a named pipe) is a Strategy of the system, in
//! `wire`. A heartbeat is only believed if it is of this user (`SO_PEERCRED` on Unix, the process behind the pipe on Windows).

use std::io;
use std::path::Path;
use std::time::Duration;

use super::wire::exchange;
use super::{Beat, Pulse, Saved, clean_key};

/// Why a ping got no pulse. Not an `io::Error`: the caller decides on the kind (a hung program, a refusal, a system without a
/// heartbeat channel), and none of them may look like health.
#[derive(Debug)]
pub enum BeatError {
    /// This system has neither a Unix socket nor a named pipe for the heartbeat; nobody can be asked here.
    Unsupported,
    /// Nobody answers: no heartbeat there (the name is not found, the connection is refused), no answer in the limit, or the
    /// other end hung up without a word.
    NoAnswer,
    /// Somebody answers, but is not believed: the heartbeat belongs to another user or to another process than the server's.
    Refused,
    /// The system failed in another way, or the answer was no heartbeat line.
    Io(io::Error),
}

impl std::fmt::Display for BeatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("this system has no heartbeat channel"),
            Self::NoAnswer => f.write_str("the heartbeat did not answer"),
            Self::Refused => f.write_str("the heartbeat is not of this user or process"),
            Self::Io(e) => write!(f, "the heartbeat failed: {e}"),
        }
    }
}

impl std::error::Error for BeatError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for BeatError {
    fn from(e: io::Error) -> Self {
        use io::ErrorKind as K;
        match e.kind() {
            K::Unsupported => Self::Unsupported,
            K::NotFound
            | K::ConnectionRefused
            | K::ConnectionReset
            | K::ConnectionAborted
            | K::BrokenPipe
            | K::UnexpectedEof
            | K::TimedOut => Self::NoAnswer,
            K::PermissionDenied => Self::Refused,
            _ => Self::Io(e),
        }
    }
}

/// Ping the heartbeat of the server at `socket` (the Unix socket, or the pipe name on Windows).
///
/// # Errors
/// [`BeatError`]: never a silent `Ok`. A system without both ways says `Unsupported`; no heartbeat or no answer in `limit` is
/// `NoAnswer`; one of another user or process is `Refused`.
pub async fn ask_beat(socket: &Path, limit: Duration) -> Result<Pulse, BeatError> {
    Ok(ask_beat_from(socket, limit, None).await?)
}

/// [`ask_beat`], believing only the process `pid` (the one that listens on the server's socket): a heartbeat answered by another
/// process is an error. `ensure_single` uses it, so a leftover file or a neighbour's heartbeat is never taken for a channel.
pub(crate) async fn ask_beat_from(
    socket: &Path,
    limit: Duration,
    pid: Option<i32>,
) -> io::Result<Pulse> {
    let answer = exchange(socket, "ping", limit, pid).await?;
    parse_pulse(&answer)
}

fn parse_pulse(answer: &str) -> io::Result<Pulse> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "not a heartbeat answer");
    let age = |t: &str| -> io::Result<Option<Duration>> {
        if t == "-" {
            return Ok(None);
        }
        Ok(Some(Duration::from_millis(t.parse().map_err(|_| bad())?)))
    };
    let mut parts = answer.splitn(4, ' ');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("alive"), Some(t), None, None) => Ok(Pulse {
            beat: Beat::Alive,
            tick_age: age(t)?,
        }),
        (Some("busy"), Some(n), Some(t), Some(what)) => Ok(Pulse {
            beat: Beat::Busy {
                progress: n.parse().map_err(|_| bad())?,
                what: clean_key(what),
            },
            tick_age: age(t)?,
        }),
        _ => Err(bad()),
    }
}

/// Ask the program to save what would be lost; waits at most `limit` for the hook.
///
/// # Errors
/// As for [`ask_beat`], as an `io::Error` (`Unsupported`, `NotFound`, `TimedOut`, `PermissionDenied`).
pub async fn ask_save(socket: &Path, limit: Duration) -> io::Result<Saved> {
    ask_save_from(socket, limit, None).await
}

/// [`ask_save`] with the process check of [`ask_beat_from`].
pub(crate) async fn ask_save_from(
    socket: &Path,
    limit: Duration,
    pid: Option<i32>,
) -> io::Result<Saved> {
    match exchange(socket, "save", limit, pid).await?.as_str() {
        "saved" => Ok(Saved::Saved),
        "nothing" => Ok(Saved::Nothing),
        "failed" => Ok(Saved::Failed),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a save answer",
        )),
    }
}
