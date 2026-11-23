//! The heartbeat's channel on a system with neither a Unix socket nor a named pipe: there is none; starting says so.
//!
//! Pattern: Null object (the channel of a system that has none).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::server::Shared;

pub(super) struct Guard;

pub(super) fn open(_socket: &Path, _shared: &Arc<Shared>) -> io::Result<(PathBuf, Guard)> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this system has no heartbeat channel",
    ))
}
