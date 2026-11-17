//! The stable codes.
//!
//! Pattern: none, an enum with total `match`es. A code is never renamed or reused once released: programs keep `MessageFormat` 2
//! messages under `message_key()`. A new problem gets a new variant (the enum is `non_exhaustive`).

use std::fmt;

/// The enum and its list `ALL` come from ONE list, so a code cannot be missing from `ALL`. A code missing from the list is
/// missing from the enum, and then the total `match`es below and the test no longer compile.
macro_rules! codes {
    ($($(#[$doc:meta])* $variant:ident,)+) => {
        /// What went wrong, without a sentence. Kebab-case name: [`RpcErrorCode::as_str`]; message key (`rpc-error-` and the name):
        /// [`RpcErrorCode::message_key`].
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum RpcErrorCode {
            $($(#[$doc])* $variant,)+
        }

        impl RpcErrorCode {
            /// Every code, in the order of the enum (the catalog in `docs/errors.md` reads it).
            pub const ALL: [Self; { [$(Self::$variant),+].len() }] = [$(Self::$variant),+];
        }
    };
}

codes! {
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
    /// The data folder cannot be used: it is not private to this user (the parameter `cause` names why).
    DataDirNotPrivate,
    /// A server holds the socket and does not answer, and it was not ended (the parameters say which process and why).
    ServerUnresponsive,
}

impl RpcErrorCode {
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
            Self::DataDirNotPrivate => "data-dir-not-private",
            Self::ServerUnresponsive => "server-unresponsive",
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
            Self::DataDirNotPrivate => "rpc-error-data-dir-not-private",
            Self::ServerUnresponsive => "rpc-error-server-unresponsive",
        }
    }
}

impl fmt::Display for RpcErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::RpcErrorCode as C;

    /// The position of each code in the list. The `match` has no `_`: a new variant does not compile until it is named here.
    fn position(code: C) -> usize {
        match code {
            C::NoServer => 0,
            C::SocketUnreachable => 1,
            C::AlreadyRunning => 2,
            C::ContractMismatch => 3,
            C::CannotRestart => 4,
            C::OtherProgram => 5,
            C::TooOldToStop => 6,
            C::Timeout => 7,
            C::ConnectionLost => 8,
            C::Interrupted => 9,
            C::Unimplemented => 10,
            C::InvalidRequest => 11,
            C::TooLarge => 12,
            C::Protocol => 13,
            C::Io => 14,
            C::DataDirNotPrivate => 15,
            C::ServerUnresponsive => 16,
        }
    }

    #[test]
    fn all_is_complete_and_in_the_order_of_the_enum() {
        assert_eq!(C::ALL.len(), position(C::ServerUnresponsive) + 1);
        for (i, code) in C::ALL.into_iter().enumerate() {
            assert_eq!(position(code), i, "{code}");
        }
    }
}
