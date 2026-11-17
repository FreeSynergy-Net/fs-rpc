//! The class of a system failure as a stable token.
//!
//! Pattern: none, a fixed table. The token is the parameter `kind` of [`RpcErrorCode::Io`](super::RpcErrorCode) and of a
//! failed replacement. It comes from this table, not from the `Debug` text of `io::ErrorKind` (which the standard library does not
//! promise); a class this table does not know is `other`.

use std::io::ErrorKind as K;

/// The token of an `io::ErrorKind` (`permission-denied`), or `other` for a class not in the table.
pub(super) const fn kind_token(kind: K) -> &'static str {
    match kind {
        K::NotFound => "not-found",
        K::PermissionDenied => "permission-denied",
        K::ConnectionRefused => "connection-refused",
        K::ConnectionReset => "connection-reset",
        K::HostUnreachable => "host-unreachable",
        K::NetworkUnreachable => "network-unreachable",
        K::ConnectionAborted => "connection-aborted",
        K::NotConnected => "not-connected",
        K::AddrInUse => "addr-in-use",
        K::AddrNotAvailable => "addr-not-available",
        K::NetworkDown => "network-down",
        K::BrokenPipe => "broken-pipe",
        K::AlreadyExists => "already-exists",
        K::WouldBlock => "would-block",
        K::NotADirectory => "not-a-directory",
        K::IsADirectory => "is-a-directory",
        K::DirectoryNotEmpty => "directory-not-empty",
        K::ReadOnlyFilesystem => "read-only-filesystem",
        K::StaleNetworkFileHandle => "stale-network-file-handle",
        K::InvalidInput => "invalid-input",
        K::InvalidData => "invalid-data",
        K::TimedOut => "timed-out",
        K::WriteZero => "write-zero",
        K::StorageFull => "storage-full",
        K::NotSeekable => "not-seekable",
        K::QuotaExceeded => "quota-exceeded",
        K::FileTooLarge => "file-too-large",
        K::ResourceBusy => "resource-busy",
        K::ExecutableFileBusy => "executable-file-busy",
        K::Deadlock => "deadlock",
        K::CrossesDevices => "crosses-devices",
        K::TooManyLinks => "too-many-links",
        K::InvalidFilename => "invalid-filename",
        K::ArgumentListTooLong => "argument-list-too-long",
        K::Interrupted => "interrupted",
        K::Unsupported => "unsupported",
        K::UnexpectedEof => "unexpected-eof",
        K::OutOfMemory => "out-of-memory",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_come_from_the_table() {
        assert_eq!(kind_token(K::PermissionDenied), "permission-denied");
        assert_eq!(kind_token(K::InvalidInput), "invalid-input");
        assert_eq!(kind_token(K::NotADirectory), "not-a-directory");
        assert_eq!(kind_token(K::UnexpectedEof), "unexpected-eof");
        assert_eq!(kind_token(K::Other), "other");
    }

    #[test]
    fn every_token_is_kebab_case_and_unique() {
        let all = [
            K::NotFound,
            K::PermissionDenied,
            K::ConnectionRefused,
            K::ConnectionReset,
            K::HostUnreachable,
            K::NetworkUnreachable,
            K::ConnectionAborted,
            K::NotConnected,
            K::AddrInUse,
            K::AddrNotAvailable,
            K::NetworkDown,
            K::BrokenPipe,
            K::AlreadyExists,
            K::WouldBlock,
            K::NotADirectory,
            K::IsADirectory,
            K::DirectoryNotEmpty,
            K::ReadOnlyFilesystem,
            K::StaleNetworkFileHandle,
            K::InvalidInput,
            K::InvalidData,
            K::TimedOut,
            K::WriteZero,
            K::StorageFull,
            K::NotSeekable,
            K::QuotaExceeded,
            K::FileTooLarge,
            K::ResourceBusy,
            K::ExecutableFileBusy,
            K::Deadlock,
            K::CrossesDevices,
            K::TooManyLinks,
            K::InvalidFilename,
            K::ArgumentListTooLong,
            K::Interrupted,
            K::Unsupported,
            K::UnexpectedEof,
            K::OutOfMemory,
            K::Other,
        ];
        let mut seen = std::collections::HashSet::new();
        for kind in all {
            let t = kind_token(kind);
            assert!(t.chars().all(|c| c.is_ascii_lowercase() || c == '-'), "{t}");
            assert!(seen.insert(t), "{t} twice");
            // The table agrees with the (unpromised) Debug name where both exist: a change of std shows up here.
            let debug = format!("{kind:?}").to_ascii_lowercase();
            assert_eq!(t.replace('-', ""), debug, "{t}");
        }
    }
}
