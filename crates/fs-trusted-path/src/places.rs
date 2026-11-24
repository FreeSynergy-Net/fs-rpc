//! The system places and the check "this came from the package manager / the installer" (moved from fs-starter `system.rs`): below one of
//! the places, no link anywhere on the way, owned by the system, and writable by nobody else. Unix (rpm world): `/usr/bin`, the modules
//! in `/usr/libexec/freesynergy/modules`, `/usr/share/freesynergy`; the owner is root. Windows: ONE place, the anchor
//! `<FOLDERID_ProgramFiles>\FreeSynergy`, owned by SYSTEM, Administrators or `TrustedInstaller`. dnf and the installer check the
//! signature of the package; there is no second signature check.
//!
//! The check of ONE part of the way is [`Trust::system_owned`]. This file decides only WHICH places count (the policy) and walks the
//! way from the top.
//!
//! Pattern: Specification (a check over the file system metadata; no state) + Value (the refusal says which border was hit).

use std::fmt;
use std::path::{Component, Path, PathBuf};

use crate::{Level, PlaceRefusal, Trust};

/// Why a path does not count as packaged (one border each; the first one hit is told).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemRefusal {
    /// The path is not absolute, or has `.` or `..` in it.
    NotAbsolute,
    /// The path is not below one of the system places.
    OutsideSystem,
    /// A part of the way is refused by [`Trust::system_owned`] (link, owner, rights, disk, unreadable).
    Part(PlaceRefusal),
}

impl From<PlaceRefusal> for SystemRefusal {
    fn from(why: PlaceRefusal) -> Self {
        Self::Part(why)
    }
}

impl fmt::Display for SystemRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAbsolute => f.write_str("not-absolute"),
            Self::OutsideSystem => f.write_str("outside-system"),
            Self::Part(why) => why.fmt(f),
        }
    }
}

/// The system places and who counts as the system.
#[derive(Clone, Debug)]
pub struct SystemPlaces {
    roots: Vec<PathBuf>,
    trust: Trust,
    /// Check every folder from the top down (real system; above the first place with the rule "only what can swap the child"); else
    /// from the place itself down (a test folder under a user's home).
    from_fs_root: bool,
}

impl SystemPlaces {
    /// The real places, the real system owners, checked from the top. Where the system has no places (no known folder, other system)
    /// there are none: every path is outside.
    #[must_use]
    pub fn standard() -> Self {
        #[cfg(unix)]
        let roots: Vec<PathBuf> = [crate::STARTER_DIR, crate::MODULES_DIR, crate::SHARE_DIR]
            .iter()
            .map(PathBuf::from)
            .collect();
        #[cfg(windows)]
        let roots: Vec<PathBuf> = crate::known_folder::anchor().into_iter().collect();
        #[cfg(not(any(unix, windows)))]
        let roots: Vec<PathBuf> = Vec::new();
        Self {
            roots,
            trust: Trust::system(),
            from_fs_root: true,
        }
    }

    /// Other places and another circle of trust, checked from the place itself down. Only for tests (a test folder is never the
    /// system's); behind the feature `test-support`, which no program switches on.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    #[must_use]
    pub fn for_test(roots: Vec<PathBuf>, trust: Trust) -> Self {
        Self {
            roots,
            trust,
            from_fs_root: false,
        }
    }

    /// Does `path` count as packaged? Below a system place, no link, owned by the system, not writable by anybody else, on every part of
    /// the way (also the folders above, for the real places; above the place only what can swap the child counts on Windows).
    ///
    /// # Errors
    /// The first border that is hit, see [`SystemRefusal`].
    pub fn check(&self, path: &Path) -> Result<(), SystemRefusal> {
        if !path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::CurDir | Component::ParentDir))
        {
            return Err(SystemRefusal::NotAbsolute);
        }
        let root = self
            .roots
            .iter()
            .find(|r| path.starts_with(r))
            .ok_or(SystemRefusal::OutsideSystem)?;
        let root_parts = root.components().count();
        let start = if self.from_fs_root { 0 } else { root_parts };
        let mut walk = PathBuf::new();
        for (i, part) in path.components().enumerate() {
            walk.push(part);
            // `C:` alone is no folder (the current folder of the drive): the first part is the root `C:\`
            if !walk.has_root() {
                continue;
            }
            // the places of a test start below the anchor: the anchor itself is checked too (index start - 1)
            if i + 1 >= start.max(1) {
                let level = if i + 1 < root_parts {
                    Level::AboveAnchor
                } else {
                    Level::Anchor
                };
                self.trust.system_owned(&walk, level)?;
            }
        }
        Ok(())
    }
}
