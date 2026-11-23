//! The wire of the heartbeat client on a system with neither a Unix socket nor a named pipe: there is none, and it says so.
//!
//! Pattern: Null object (the wire of a system that has none).
//! (`heartbeat-unsupported`: the only case left for `Unsupported`.)

use std::io;
use std::path::Path;
use std::time::Duration;

pub(super) async fn exchange(
    _socket: &Path,
    _line: &str,
    _limit: Duration,
    _expect_pid: Option<i32>,
) -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this system has no heartbeat channel",
    ))
}
