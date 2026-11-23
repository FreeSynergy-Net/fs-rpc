//! The wire of the heartbeat client on Windows: the pipe `<pipe>.beat`; the process behind it must be this user's.

use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::transport::beat;

/// Send `line`, read the one line that answers; every step within `limit`. `expect_pid` is a Unix notion (a pipe knows its server
/// by user, and the server's process is checked as such), so it is not used here.
pub(super) async fn exchange(
    socket: &Path,
    line: &str,
    limit: Duration,
    _expect_pid: Option<i32>,
) -> io::Result<String> {
    exchange_with(socket, line, limit, beat::SERVER_IS_ME).await
}

/// [`exchange`] with the check of the server's process as an argument (the test of the refusal gives a check that says no).
pub(super) async fn exchange_with(
    socket: &Path,
    line: &str,
    limit: Duration,
    server_ok: beat::Admit,
) -> io::Result<String> {
    let work = async {
        let mut stream = beat::connect(socket, server_ok).await?;
        stream.write_all(format!("{line}\n").as_bytes()).await?;
        let mut answer = String::new();
        BufReader::new((&mut stream).take(256))
            .read_line(&mut answer)
            .await?;
        if answer.is_empty() {
            // The server hung up without a line: no answer.
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the heartbeat hung up without an answer",
            ));
        }
        Ok(answer.trim().to_owned())
    };
    tokio::time::timeout(limit, work)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "the heartbeat did not answer"))?
}
