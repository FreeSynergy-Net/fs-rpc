//! The stable codes.
//!
//! Pattern: none, an enum with total `match`es. A code is never renamed or reused once released: programs keep `MessageFormat` 2
//! messages under `message_key()`. A new problem gets a new variant (the enum is `non_exhaustive`).

use std::fmt;

/// What went wrong, without a sentence. Kebab-case name: [`RpcErrorCode::as_str`]; message key (`rpc-error-` and the name):
/// [`RpcErrorCode::message_key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RpcErrorCode {
    /// Nothing listens on the socket (no such file, or the connection was refused).
    NoServer,
    /// The socket exists but cannot be used (another user serves it, rights).
    SocketUnreachable,
    /// A server of this program already runs on the socket.
    AlreadyRunning,
    /// The server runs, but is not the version or protocol the client needs, and it is still not after a replacement.
    ContractMismatch,
    /// The running server could not be replaced (`step` says which step failed).
    CannotRestart,
    /// The socket is served by another program; it was left alone.
    OtherProgram,
    /// The running server is too old to be stopped by request, and its process is unknown.
    TooOldToStop,
    /// No answer within the deadline (a write may still have happened).
    Timeout,
    /// The connection ended: the server went away, or the connection thread is gone.
    ConnectionLost,
    /// A call ended without an answer (panic, or the connection was closed under it).
    Interrupted,
    /// "Not possible" (ADR 0045): the server does not know the method, or the provider says it cannot.
    Unimplemented,
    /// The request itself cannot be accepted (missing or malformed id, cursor, time or value).
    InvalidRequest,
    /// The request or its answer does not fit into one message.
    TooLarge,
    /// A problem of another type, or an answer that cannot be read; the problem type is the parameter `type`.
    Protocol,
    /// Any other failure of the system (file, socket, process); the parameter `kind` names the class.
    Io,
}

impl RpcErrorCode {
    /// Every code, in the order of the enum (the test of completeness and the catalog in `docs/errors.md` read it).
    pub const ALL: [Self; 15] = [
        Self::NoServer,
        Self::SocketUnreachable,
        Self::AlreadyRunning,
        Self::ContractMismatch,
        Self::CannotRestart,
        Self::OtherProgram,
        Self::TooOldToStop,
        Self::Timeout,
        Self::ConnectionLost,
        Self::Interrupted,
        Self::Unimplemented,
        Self::InvalidRequest,
        Self::TooLarge,
        Self::Protocol,
        Self::Io,
    ];

    /// The stable name in kebab-case; it is what a log or a wire carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoServer => "no-server",
            Self::SocketUnreachable => "socket-unreachable",
            Self::AlreadyRunning => "already-running",
            Self::ContractMismatch => "contract-mismatch",
            Self::CannotRestart => "cannot-restart",
            Self::OtherProgram => "other-program",
            Self::TooOldToStop => "too-old-to-stop",
            Self::Timeout => "timeout",
            Self::ConnectionLost => "connection-lost",
            Self::Interrupted => "interrupted",
            Self::Unimplemented => "unimplemented",
            Self::InvalidRequest => "invalid-request",
            Self::TooLarge => "too-large",
            Self::Protocol => "protocol",
            Self::Io => "io",
        }
    }

    /// The key of the program's `MessageFormat` 2 message: `rpc-error-` and the name (`rpc-error-no-server`).
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::NoServer => "rpc-error-no-server",
            Self::SocketUnreachable => "rpc-error-socket-unreachable",
            Self::AlreadyRunning => "rpc-error-already-running",
            Self::ContractMismatch => "rpc-error-contract-mismatch",
            Self::CannotRestart => "rpc-error-cannot-restart",
            Self::OtherProgram => "rpc-error-other-program",
            Self::TooOldToStop => "rpc-error-too-old-to-stop",
            Self::Timeout => "rpc-error-timeout",
            Self::ConnectionLost => "rpc-error-connection-lost",
            Self::Interrupted => "rpc-error-interrupted",
            Self::Unimplemented => "rpc-error-unimplemented",
            Self::InvalidRequest => "rpc-error-invalid-request",
            Self::TooLarge => "rpc-error-too-large",
            Self::Protocol => "rpc-error-protocol",
            Self::Io => "rpc-error-io",
        }
    }
}

impl fmt::Display for RpcErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
