//! Why a running server could not be replaced.
//!
//! Pattern: Value object (typed instead of a sentence: v0.0.3 replaces the `reason: String` of
//! `HandshakeError::CannotRestart`). `Display` keeps the former English text as a fallback for logs; a window shows the
//! [`RpcError`](crate::RpcError) instead.

use std::fmt;
use std::io;

/// The step of the replacement that failed, with what it found.
#[derive(Debug)]
#[non_exhaustive]
pub enum RestartCause {
    /// The socket is served by another program, which is left alone.
    OtherProgram {
        /// The program that answered.
        found: String,
        /// The program that was expected.
        wanted: String,
    },
    /// The running server could not be asked.
    CannotAsk(io::Error),
    /// The server is too old to stop by request and its process is unknown.
    TooOldToStop,
    /// The old server (process `pid`) could not be ended by the system.
    CannotTerminate {
        /// The process of the old server.
        pid: u32,
        /// What the system said.
        source: io::Error,
    },
    /// The running server did not accept the stop request.
    StopRefused(io::Error),
    /// The old server did not end within the wait.
    NotEnded,
    /// The program could not start a new server.
    NotStarted(io::Error),
    /// The new server cannot be reached.
    NewUnreachable(io::Error),
}

impl RestartCause {
    /// The name of the step, as the parameter `step` of the error.
    #[must_use]
    pub fn step(&self) -> &'static str {
        match self {
            Self::OtherProgram { .. } => "other-program",
            Self::CannotAsk(_) => "ask",
            Self::TooOldToStop => "too-old",
            Self::CannotTerminate { .. } => "terminate",
            Self::StopRefused(_) => "stop",
            Self::NotEnded => "not-ended",
            Self::NotStarted(_) => "start",
            Self::NewUnreachable(_) => "reach",
        }
    }

    /// The system failure behind the step, if there is one.
    #[must_use]
    pub fn io(&self) -> Option<&io::Error> {
        match self {
            Self::CannotAsk(e)
            | Self::StopRefused(e)
            | Self::NotStarted(e)
            | Self::NewUnreachable(e)
            | Self::CannotTerminate { source: e, .. } => Some(e),
            _ => None,
        }
    }
}

impl fmt::Display for RestartCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OtherProgram { found, wanted } => write!(
                f,
                "the socket is served by {found}, not by {wanted}: check the --socket argument"
            ),
            Self::CannotAsk(e) => write!(f, "the running server cannot be asked: {e}"),
            Self::TooOldToStop => f.write_str(
                "the running server is too old to be stopped by request, and its process is unknown: end it by hand",
            ),
            Self::CannotTerminate { pid, source } => {
                write!(f, "the old server (process {pid}) could not be ended: {source}")
            }
            Self::StopRefused(e) => write!(f, "the running server did not accept the stop: {e}"),
            Self::NotEnded => f.write_str("the old server did not end in time"),
            Self::NotStarted(e) => write!(f, "the server did not start: {e}"),
            Self::NewUnreachable(e) => write!(f, "the new server cannot be reached: {e}"),
        }
    }
}
