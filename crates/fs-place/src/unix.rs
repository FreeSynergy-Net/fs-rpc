//! The Unix version of the check (moved from fs-starter `system.rs`): no symlink, owned by a trusted uid, not writable by group or others.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::{Level, PlaceRefusal};

/// A user id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Owner(u32);

impl Owner {
    /// The owner with this uid.
    #[must_use]
    pub fn uid(uid: u32) -> Self {
        Self(uid)
    }
}

pub(crate) fn check(owners: &[Owner], part: &Path, _level: Level) -> Result<(), PlaceRefusal> {
    let m = std::fs::symlink_metadata(part).map_err(|_| PlaceRefusal::Unreadable)?;
    if m.file_type().is_symlink() {
        return Err(PlaceRefusal::Symlink);
    }
    if !owners.iter().any(|o| o.0 == m.uid()) {
        return Err(PlaceRefusal::WrongOwner);
    }
    if m.mode() & 0o022 != 0 {
        return Err(PlaceRefusal::Writable);
    }
    Ok(())
}
