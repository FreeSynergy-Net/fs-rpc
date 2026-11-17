//! One typed parameter of an error.
//!
//! Pattern: Value object. A parameter is a text or a number, never a sentence; `MessageFormat` 2 gets it as a string or a
//! number operand.

use std::fmt;

/// The names of the parameters this crate sets. Other names come from the extensions of a problem (`id`, `kind`, `issue`).
pub mod param_keys {
    /// The socket (path or pipe name); set by the program with [`RpcError::with_text`](crate::RpcError::with_text),
    /// the library does not know it at every place.
    pub const SOCKET: &str = "socket";
    /// The server as a client names it (`people`).
    pub const SERVER: &str = "server";
    /// Program name of the running server.
    pub const RUNNING_PROGRAM: &str = "running-program";
    /// Version of the running server; left out if it did not say.
    pub const RUNNING_VERSION: &str = "running-version";
    /// Protocol level of the running server.
    pub const RUNNING_PROTOCOL: &str = "running-protocol";
    /// Program name the client expects; left out if any.
    pub const EXPECTED_PROGRAM: &str = "expected-program";
    /// Version the client expects; left out if any.
    pub const EXPECTED_VERSION: &str = "expected-version";
    /// Protocol level the client expects.
    pub const EXPECTED_PROTOCOL: &str = "expected-protocol";
    /// The step of a replacement that failed: `ask`, `stop`, `terminate`, `not-ended`, `start`, `reach`.
    pub const STEP: &str = "step";
    /// Process number of an old server that could not be ended.
    pub const PID: &str = "pid";
    /// The deadline that ran out, in milliseconds.
    pub const LIMIT_MS: &str = "limit-ms";
    /// The class of a system failure (`permission-denied`, `invalid-input`, ...): the name of `std::io::ErrorKind`.
    pub const KIND: &str = "kind";
    /// Why a data folder is refused: `foreign-owner`, `mode`, `symlink`, `ancestor`, `not-directory`.
    pub const CAUSE: &str = "cause";
    /// Why a silent server is not ended: `itself`, `foreign-user`, `unreadable`, `pid-reused`, `other-program`, `unsupported`.
    pub const WHY: &str = "why";
    /// The program file of the process that holds the socket (its real path), if it could be read.
    pub const EXE: &str = "exe";
    /// The capability a "not possible" is about (`board.open`); from the problem extension `capability`.
    pub const CAPABILITY: &str = "capability";
    /// The problem type without the base URI (`people/not-found`).
    pub const TYPE: &str = "type";
    /// The measured value that crossed a limit (bytes); from the problem extension `size`.
    pub const SIZE: &str = "size";
    /// The limit that was crossed (bytes); from the problem extension `limit`.
    pub const LIMIT: &str = "limit";
}

/// A text or a number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Param {
    /// A name, a path, a token.
    Text(String),
    /// A count, a size, a duration.
    Number(u64),
}

impl Param {
    /// The text, if it is one.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(t) => Some(t),
            Self::Number(_) => None,
        }
    }

    /// The number, if it is one.
    #[must_use]
    pub fn as_number(&self) -> Option<u64> {
        match self {
            Self::Number(n) => Some(*n),
            Self::Text(_) => None,
        }
    }
}

impl fmt::Display for Param {
    /// Plain; a text with spaces, quotes or brackets is quoted so a log line stays one `key=value` list.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(t) if t.is_empty() || t.contains([' ', '"', '(', ')', ',']) => {
                write!(f, "{t:?}")
            }
            Self::Text(t) => f.write_str(t),
        }
    }
}
