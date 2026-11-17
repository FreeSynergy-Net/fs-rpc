//! Asking a heartbeat: the other end, async, every step with a limit.
//!
//! Pattern: Request-reply on a plain line protocol. Only a socket of this user is believed (`SO_PEERCRED`).

use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::{Beat, Pulse, Saved, clean_key};
use crate::companion_path;

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "the heartbeat did not answer")
}

async fn exchange(socket: &Path, line: &str, limit: Duration) -> io::Result<String> {
    let work = async {
        let stream = UnixStream::connect(companion_path(socket, "beat")).await?;
        if stream.peer_cred()?.uid() != rustix::process::geteuid().as_raw() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "served by another user",
            ));
        }
        let (r, mut w) = stream.into_split();
        w.write_all(format!("{line}\n").as_bytes()).await?;
        let mut answer = String::new();
        BufReader::new(r.take(256)).read_line(&mut answer).await?;
        Ok(answer.trim().to_owned())
    };
    tokio::time::timeout(limit, work)
        .await
        .map_err(|_| timed_out())?
}

/// Ping the heartbeat of the server at `socket`.
///
/// # Errors
/// No heartbeat there (`NotFound`, `ConnectionRefused`), no answer in `limit` (`TimedOut`), or an answer that is none.
pub async fn ask_beat(socket: &Path, limit: Duration) -> io::Result<Pulse> {
    let answer = exchange(socket, "ping", limit).await?;
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
/// As for [`ask_beat`].
pub async fn ask_save(socket: &Path, limit: Duration) -> io::Result<Saved> {
    match exchange(socket, "save", limit).await?.as_str() {
        "saved" => Ok(Saved::Saved),
        "nothing" => Ok(Saved::Nothing),
        "failed" => Ok(Saved::Failed),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a save answer",
        )),
    }
}
