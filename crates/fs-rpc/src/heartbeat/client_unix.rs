//! The wire of the heartbeat client on Unix: the socket `<socket>.beat`, the process behind it looked at with `SO_PEERCRED`.
//!
//! Pattern: Strategy (the wire of the heartbeat client on Unix).

use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::companion_path;

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "the heartbeat did not answer")
}

/// Send `line`, read the one line that answers; every step within `limit`.
pub(super) async fn exchange(
    socket: &Path,
    line: &str,
    limit: Duration,
    expect_pid: Option<i32>,
) -> io::Result<String> {
    let work = async {
        let stream = UnixStream::connect(companion_path(socket, "beat")).await?;
        let cred = stream.peer_cred()?;
        if let Some(pid) = expect_pid
            && cred.pid() != Some(pid)
        {
            // A heartbeat of another process (or a leftover file) is no channel of this server.
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the heartbeat belongs to another process",
            ));
        }
        if cred.uid() != rustix::process::geteuid().as_raw() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "served by another user",
            ));
        }
        let (r, mut w) = stream.into_split();
        w.write_all(format!("{line}\n").as_bytes()).await?;
        let mut answer = String::new();
        BufReader::new(r.take(256)).read_line(&mut answer).await?;
        if answer.is_empty() {
            // The server hung up without a line (a client that is not believed is dropped so): no answer.
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the heartbeat hung up without an answer",
            ));
        }
        Ok(answer.trim().to_owned())
    };
    tokio::time::timeout(limit, work)
        .await
        .map_err(|_| timed_out())?
}
