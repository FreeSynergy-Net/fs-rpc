//! Why `ensure_single` did not give a connection, and what it did on the way.
//!
//! Pattern: Value objects, typed (a code and parameters, ADR 0057): `From<&SingleError> for RpcError` makes `server-unresponsive`,
//! `server-busy`, or the code of the handshake failure.

use std::io;
use std::time::Duration;

use crate::code::param_keys as k;
use crate::{HandshakeError, RpcError, RpcErrorCode};

/// A server that holds the socket, does not answer and was not ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stuck {
    /// Its process, if known.
    pub pid: Option<u32>,
    /// Why it was not ended (`itself`, `foreign-user`, `unreadable`, `pid-reused`, `other-program`, `beating`, `unsupported`,
    /// `not-ending`): another program is only reported.
    pub why: &'static str,
    /// The real path of its program file, if it could be read.
    pub exe: Option<String>,
    /// How long the wait was.
    pub limit: Duration,
}

/// A process of this program that was ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TakenOver {
    /// The ended process.
    pub pid: u32,
    /// What its save before the end came to.
    pub saved: crate::Saved,
    /// How it ended: `gone` (it was gone already), `sigterm`, `sigkill`.
    pub how: &'static str,
}

/// Why no connection.
#[derive(Debug)]
#[non_exhaustive]
pub enum SingleError {
    /// The handshake failed (a replacement failed, another program answers, a mismatch, another user).
    Handshake(HandshakeError),
    /// A server holds the socket and does not answer, and it may not be ended (or did not end).
    Unresponsive(Stuck),
    /// A server does not answer its contract but is busy and moves: it is never ended; try again later.
    Busy {
        /// Its process.
        pid: Option<u32>,
        /// What it does (text key).
        what: String,
        /// How far it is.
        progress: u64,
    },
    /// The whole attempt took longer than its limit.
    TooLong(Duration),
}

fn millis(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}

impl From<&SingleError> for RpcError {
    fn from(e: &SingleError) -> Self {
        match e {
            SingleError::Handshake(h) => Self::from(h),
            SingleError::Unresponsive(s) => {
                let mut e = Self::new(RpcErrorCode::ServerUnresponsive);
                if let Some(pid) = s.pid {
                    e = e.with_number(k::PID, u64::from(pid));
                }
                e = e.with_text(k::WHY, s.why);
                if let Some(exe) = &s.exe {
                    e = e.with_text(k::EXE, exe.as_str());
                }
                e.with_number(k::LIMIT_MS, millis(s.limit))
            }
            SingleError::Busy {
                pid,
                what,
                progress,
            } => {
                let mut e = Self::new(RpcErrorCode::ServerBusy);
                if let Some(pid) = pid {
                    e = e.with_number(k::PID, u64::from(*pid));
                }
                e.with_text(k::WHAT, what.as_str())
                    .with_number(k::PROGRESS, *progress)
            }
            SingleError::TooLong(limit) => {
                Self::new(RpcErrorCode::Timeout).with_number(k::LIMIT_MS, millis(*limit))
            }
        }
    }
}

impl std::fmt::Display for SingleError {
    /// An English text for logs; a window shows the [`RpcError`] instead.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handshake(h) => write!(f, "{h}"),
            Self::Unresponsive(s) => {
                write!(f, "a server does not answer and was not ended ({})", s.why)
            }
            Self::Busy { what, progress, .. } => {
                write!(f, "the server is busy: {what} ({progress})")
            }
            Self::TooLong(limit) => write!(f, "no connection within {limit:?}"),
        }
    }
}

impl std::error::Error for SingleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Handshake(h) => Some(h),
            _ => None,
        }
    }
}

/// Whether a handshake failure means "the server holds the socket and did not answer in time".
pub(super) fn is_silent(e: &HandshakeError) -> bool {
    use crate::RestartCause as C;
    let timed_out = |io: &io::Error| io.kind() == io::ErrorKind::TimedOut;
    match e {
        HandshakeError::Io(io) => timed_out(io),
        HandshakeError::CannotRestart { cause, .. } => {
            matches!(&**cause, C::CannotAsk(io) | C::StopRefused(io) if timed_out(io))
        }
        HandshakeError::ServerMismatch { .. } => false,
    }
}
