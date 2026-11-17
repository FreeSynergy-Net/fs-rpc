//! The data folder of a program: private to its user, corrected if it was made too open, never taken from somebody else.
//!
//! Pattern: Policy + Guard — [`decide`] is a pure rule over the facts of a folder ([`Meta`]), so every case is tested with made-up
//! facts; `prepare_data_dir` (Unix: `transport/unix.rs`) applies it to a real folder through a file descriptor. Up to v0.0.2 a
//! folder that others could read was refused, and a window that made the folder with the default mode (0755 by the usual umask)
//! before its server ran stopped the program. Now a folder of this user that is too open loses the group and other bits
//! (nothing is ever added); a folder of somebody else, a symbolic link or a file is refused; so is one below a directory that
//! others could swap. The error is typed ([`DataDirError`]): code `data-dir-not-private` with the parameter `cause` (ADR 0057),
//! the path is added by the program (`RpcError::with_text`).

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Why a data folder is not private. The token is the parameter `cause`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataDirCause {
    /// The folder belongs to another user.
    ForeignOwner,
    /// Others can reach the folder and the correction did not hold (a file system without modes, or an ACL).
    Mode,
    /// The path itself is a symbolic link.
    Symlink,
    /// A folder above it can be changed by others, so a path component could be swapped.
    Ancestor,
    /// The path is not a folder.
    NotDirectory,
}

impl DataDirCause {
    /// The stable token (`foreign-owner`, `mode`, `symlink`, `ancestor`, `not-directory`).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::ForeignOwner => "foreign-owner",
            Self::Mode => "mode",
            Self::Symlink => "symlink",
            Self::Ancestor => "ancestor",
            Self::NotDirectory => "not-directory",
        }
    }
}

/// The data folder cannot be used.
#[derive(Debug)]
#[non_exhaustive]
pub enum DataDirError {
    /// The folder is not private to this user, and was not made so.
    NotPrivate {
        /// Why.
        cause: DataDirCause,
        /// The folder that is the problem (for `Ancestor`, the folder above).
        path: PathBuf,
    },
    /// The file system refused (make, look at, change).
    Io(io::Error),
}

impl DataDirError {
    /// The cause, if the folder is not private.
    #[must_use]
    pub fn cause(&self) -> Option<DataDirCause> {
        match self {
            Self::NotPrivate { cause, .. } => Some(*cause),
            Self::Io(_) => None,
        }
    }

    /// The `io::ErrorKind` this error had as an `io::Error` up to v0.0.2: `PermissionDenied` when not private.
    #[must_use]
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            Self::NotPrivate { .. } => io::ErrorKind::PermissionDenied,
            Self::Io(e) => e.kind(),
        }
    }
}

impl fmt::Display for DataDirError {
    /// An English text for logs; a window shows [`RpcError`](crate::RpcError) instead.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotPrivate { cause, path } => write!(
                f,
                "{} must belong to this user alone ({})",
                path.display(),
                cause.token()
            ),
            Self::Io(e) => write!(f, "data folder: {e}"),
        }
    }
}

impl std::error::Error for DataDirError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::NotPrivate { .. } => None,
        }
    }
}

impl From<io::Error> for DataDirError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<DataDirError> for io::Error {
    /// For a caller that still wants an `io::Error` (`?` in a function that answers one): the kind stays.
    fn from(e: DataDirError) -> Self {
        match e {
            DataDirError::Io(io) => io,
            other => Self::new(other.kind(), other.to_string()),
        }
    }
}

/// The facts about a folder that the rule needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Meta {
    /// It is a directory.
    pub is_dir: bool,
    /// The path itself is a symbolic link.
    pub is_symlink: bool,
    /// The user id of the owner.
    pub uid: u32,
    /// The mode bits (only the lowest twelve are looked at).
    pub mode: u32,
}

/// What to do with a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Leave it: it is private already.
    Keep,
    /// Take bits away: the new mode (never more than the old one).
    Tighten(u32),
    /// Refuse it.
    Refuse(DataDirCause),
}

/// The rule: only a folder of `me` that is no link is changed, and only by taking the group and other bits away.
#[must_use]
pub fn decide(meta: &Meta, me: u32) -> Action {
    if meta.is_symlink {
        Action::Refuse(DataDirCause::Symlink)
    } else if !meta.is_dir {
        Action::Refuse(DataDirCause::NotDirectory)
    } else if meta.uid != me {
        Action::Refuse(DataDirCause::ForeignOwner)
    } else if meta.mode & 0o077 != 0 {
        Action::Tighten(meta.mode & 0o700)
    } else {
        Action::Keep
    }
}

#[cfg(test)]
#[path = "datadir_tests.rs"]
mod tests;
