//! Why a part of a way is not the system's, and where it lies. Pattern: Value (the refusal says which border was hit).

use std::fmt;

/// Why a part of the way does not count as the system's (one border each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceRefusal {
    /// Not an absolute path of a drive letter (Windows: a UNC or device path; a `:` after the drive, `.` or `..`).
    NotAbsolute,
    /// Windows: the drive is not a local fixed disk: a network drive or a `subst` alias, or whatever the system does not call a fixed disk (USB hard disks often do call themselves fixed).
    NotLocalDisk,
    /// A link (Unix: symlink; Windows: any reparse point: symlink, junction, mount point).
    Symlink,
    /// Not owned by a trusted owner.
    WrongOwner,
    /// Somebody who is not trusted can write it, or swap what is in it.
    Writable,
    /// Cannot be read, or is not there.
    Unreadable,
    /// A system with neither of the two versions.
    NotSupported,
}

impl fmt::Display for PlaceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAbsolute => "not-absolute",
            Self::NotLocalDisk => "not-local-disk",
            Self::Symlink => "symlink",
            Self::WrongOwner => "wrong-owner",
            Self::Writable => "writable",
            Self::Unreadable => "unreadable",
            Self::NotSupported => "not-supported",
        })
    }
}

/// Where a part of the way lies. Windows only: above the anchor (`C:\`) only what can swap the child counts, see rule 2.5; Unix has
/// no such step and ignores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// The anchor itself or below it: every right to write counts.
    Anchor,
    /// Above the anchor: only a right to delete, replace or re-own the child counts.
    AboveAnchor,
}
