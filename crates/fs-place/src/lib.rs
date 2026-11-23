//! Is this part of a path the system's? The check behind "this came from the package manager / the installer": nobody but the
//! system owner owns it, nobody else can write it, and there is no link on the way. Unix: owner and mode. Windows: owner and
//! the ACL (decision of the plan WIN-HOST, rules 2.1 to 2.6; the plan's threat model: an ordinary local user, not an administrator).
//! Also holds the Windows identity and rights basics (`windows`), shared with fs-rpc.
//!
//! Security module. No relaxation: a doubt is a refusal.
//!
//! Pattern: Specification (a pure check over the metadata of the file system) + Value ([`PlaceRefusal`] names the border hit).

use std::fmt;
use std::path::Path;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
pub mod windows;

#[cfg(unix)]
pub use unix::Owner;
#[cfg(windows)]
pub use windows::sid::Sid as Owner;

/// Why a part of the way does not count as the system's (one border each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceRefusal {
    /// Not an absolute path of a local disk (Windows: a UNC, network or device path; a `:` after the drive, `.` or `..`).
    NotAbsolute,
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

/// Who counts as "the system" for a check.
#[derive(Clone, Debug)]
pub struct Trust {
    owners: Vec<Owner>,
}

impl Trust {
    /// The real system owners: Unix root (uid 0); Windows SYSTEM, Administrators and `TrustedInstaller`.
    #[must_use]
    pub fn system() -> Self {
        Self {
            owners: system_owners(),
        }
    }

    /// Other owners. Only for tests that need a folder of their own (a test folder is never the system's).
    #[doc(hidden)]
    #[must_use]
    pub fn for_test(owners: Vec<Owner>) -> Self {
        Self { owners }
    }

    /// Does this one part of a way count as the system's? The caller walks the parts from the top and sets the level.
    ///
    /// # Errors
    /// The first border that is hit, see [`PlaceRefusal`].
    pub fn system_owned(&self, part: &Path, level: Level) -> Result<(), PlaceRefusal> {
        check(&self.owners, part, level)
    }
}

/// [`Trust::system_owned`] with the real system owners.
///
/// # Errors
/// The first border that is hit, see [`PlaceRefusal`].
pub fn system_owned(part: &Path, level: Level) -> Result<(), PlaceRefusal> {
    Trust::system().system_owned(part, level)
}

#[cfg(unix)]
fn system_owners() -> Vec<Owner> {
    vec![Owner::uid(0)]
}
#[cfg(unix)]
use unix::check;

#[cfg(windows)]
fn system_owners() -> Vec<Owner> {
    windows::check::system_owners()
}
#[cfg(windows)]
use windows::check::check;

#[cfg(not(any(unix, windows)))]
mod other {
    /// Never a trusted owner here.
    #[derive(Clone, Debug)]
    pub struct Owner;
}
#[cfg(not(any(unix, windows)))]
pub use other::Owner;
#[cfg(not(any(unix, windows)))]
fn system_owners() -> Vec<Owner> {
    Vec::new()
}
#[cfg(not(any(unix, windows)))]
fn check(_: &[Owner], _: &Path, _: Level) -> Result<(), PlaceRefusal> {
    Err(PlaceRefusal::NotSupported)
}
