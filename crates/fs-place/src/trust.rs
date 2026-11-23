//! Who counts as "the system" for a check, and the entry of the check. Pattern: Specification (a pure check over the metadata of
//! the file system; no state), one version per system (cfg split).

use std::path::Path;

use crate::{Level, PlaceRefusal};

#[cfg(unix)]
use crate::unix::{Owner, check};
#[cfg(windows)]
use crate::windows::{check::check, sid::Sid as Owner};

#[cfg(not(any(unix, windows)))]
mod other {
    use std::path::Path;

    use crate::{Level, PlaceRefusal};

    /// Never a trusted owner here.
    #[derive(Clone, Debug)]
    pub struct Owner;

    pub(super) fn check(_: &[Owner], _: &Path, _: Level) -> Result<(), PlaceRefusal> {
        Err(PlaceRefusal::NotSupported)
    }
}
#[cfg(not(any(unix, windows)))]
pub use other::Owner;
#[cfg(not(any(unix, windows)))]
use other::check;

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
#[cfg(windows)]
fn system_owners() -> Vec<Owner> {
    crate::windows::check::system_owners()
}
#[cfg(not(any(unix, windows)))]
fn system_owners() -> Vec<Owner> {
    Vec::new()
}
