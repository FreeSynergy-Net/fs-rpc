//! Every error type of this crate, and `capnp::Error`, as an [`RpcError`].
//!
//! Pattern: Adapter (`From` for each source). Nothing here reads a sentence: a code comes from a variant, a kind or the
//! problem type, a parameter from a field or a problem extension. The texts (`title`, `detail`, the system's own message) never
//! become parameters, they are English and for logs.

use std::io;

use capnp::Error as CapnpError;

use super::param::param_keys as k;
use super::{RpcError, RpcErrorCode};
use crate::client::problems::{Failure, local_type};
use crate::client::{Fault, connection_lost};
use crate::{Expected, HandshakeError, Problem, ServerInfo};

impl RpcError {
    /// The error of a failed call: a problem inside is read by its type (`RpcError::from(&Problem)`), `unimplemented` and a
    /// lost connection by their kind, anything else is `protocol`.
    #[must_use]
    pub fn of(error: &CapnpError) -> Self {
        if connection_lost(error) {
            return Self::new(RpcErrorCode::ConnectionLost);
        }
        Self::from(&Failure::of(error))
    }
}

impl From<&Failure> for RpcError {
    fn from(f: &Failure) -> Self {
        match f {
            Failure::Problem(p) => Self::from(p),
            Failure::Unimplemented => Self::new(RpcErrorCode::Unimplemented),
            Failure::Other(_) => Self::new(RpcErrorCode::Protocol),
        }
    }
}

impl From<&Problem> for RpcError {
    fn from(p: &Problem) -> Self {
        let local = local_type(p);
        let mut e = if p.is_unsupported() || p.status == 501 {
            let mut e = Self::new(RpcErrorCode::Unimplemented);
            if let Some((capability, _)) = p.detail.split_once(": ") {
                e = e.with_text(k::CAPABILITY, capability);
            }
            e
        } else {
            match local {
                "invalid-request" => Self::new(RpcErrorCode::InvalidRequest),
                "too-large" => Self::new(RpcErrorCode::TooLarge),
                other => Self::new(RpcErrorCode::Protocol).with_text(k::TYPE, other),
            }
        };
        for (key, value) in &p.extensions {
            e = match p.ext_u64(key) {
                Some(n) => e.with_number(key, n),
                None => e.with_text(key, value.as_str()),
            };
        }
        e
    }
}

impl From<&Fault> for RpcError {
    fn from(f: &Fault) -> Self {
        match f {
            Fault::Gone { server } => {
                Self::new(RpcErrorCode::ConnectionLost).with_text(k::SERVER, *server)
            }
            Fault::Interrupted { server } => {
                Self::new(RpcErrorCode::Interrupted).with_text(k::SERVER, *server)
            }
            Fault::TimedOut { server, limit } => Self::new(RpcErrorCode::Timeout)
                .with_text(k::SERVER, *server)
                .with_number(
                    k::LIMIT_MS,
                    u64::try_from(limit.as_millis()).unwrap_or(u64::MAX),
                ),
        }
    }
}

/// The class name of an `io::ErrorKind` as a stable token (`permission-denied`).
fn kind_token(kind: io::ErrorKind) -> String {
    let debug = format!("{kind:?}");
    let mut out = String::with_capacity(debug.len() + 4);
    for (i, c) in debug.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

impl From<&io::Error> for RpcError {
    fn from(e: &io::Error) -> Self {
        use io::ErrorKind as K;
        let code = match e.kind() {
            K::NotFound | K::ConnectionRefused => RpcErrorCode::NoServer,
            K::PermissionDenied => RpcErrorCode::SocketUnreachable,
            K::AddrInUse => RpcErrorCode::AlreadyRunning,
            K::TimedOut => RpcErrorCode::Timeout,
            K::ConnectionReset | K::ConnectionAborted | K::BrokenPipe => {
                RpcErrorCode::ConnectionLost
            }
            K::Unsupported => RpcErrorCode::Unimplemented,
            _ => RpcErrorCode::Io,
        };
        let error = Self::new(code);
        match code {
            RpcErrorCode::Io => error.with_text(k::KIND, kind_token(e.kind())),
            _ => error,
        }
    }
}

fn running(mut e: RpcError, info: &ServerInfo) -> RpcError {
    if !info.program.is_empty() {
        e = e.with_text(k::RUNNING_PROGRAM, info.program.as_str());
    }
    if !info.version.is_empty() {
        e = e.with_text(k::RUNNING_VERSION, info.version.as_str());
    }
    e.with_number(k::RUNNING_PROTOCOL, u64::from(info.protocol))
}

fn expecting(mut e: RpcError, want: &Expected) -> RpcError {
    if !want.program().is_empty() {
        e = e.with_text(k::EXPECTED_PROGRAM, want.program());
    }
    if !want.version.is_empty() {
        e = e.with_text(k::EXPECTED_VERSION, want.version.as_str());
    }
    e.with_number(k::EXPECTED_PROTOCOL, u64::from(want.protocol))
}

impl From<&HandshakeError> for RpcError {
    fn from(h: &HandshakeError) -> Self {
        use crate::RestartCause as C;
        match h {
            HandshakeError::ServerMismatch {
                running: r,
                expected,
            } => expecting(
                running(Self::new(RpcErrorCode::ContractMismatch), r),
                expected,
            ),
            HandshakeError::CannotRestart {
                running: r,
                expected,
                cause,
            } => {
                let cause = &**cause;
                let code = match cause {
                    C::OtherProgram { .. } => RpcErrorCode::OtherProgram,
                    C::TooOldToStop => RpcErrorCode::TooOldToStop,
                    _ => RpcErrorCode::CannotRestart,
                };
                let mut e = Self::new(code);
                if let Some(info) = r {
                    e = running(e, info);
                }
                e = expecting(e, expected);
                if code == RpcErrorCode::CannotRestart {
                    e = e.with_text(k::STEP, cause.step());
                }
                if let C::CannotTerminate { pid, .. } = cause {
                    e = e.with_number(k::PID, u64::from(*pid));
                }
                if let Some(io) = cause.io() {
                    e = e.with_text(k::KIND, kind_token(io.kind()));
                }
                e
            }
            HandshakeError::Io(io) => Self::from(io),
        }
    }
}

impl From<&CapnpError> for RpcError {
    fn from(e: &CapnpError) -> Self {
        Self::of(e)
    }
}

impl RpcError {
    /// Whether the person may try again: no server, a lost connection, a timeout, a failed replacement.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        use RpcErrorCode as C;
        matches!(
            self.code(),
            C::NoServer | C::ConnectionLost | C::Timeout | C::CannotRestart | C::Interrupted
        )
    }
}
