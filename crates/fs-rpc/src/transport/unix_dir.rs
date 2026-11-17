//! The data folder on Unix: private to its user, corrected if it was open, with the folders above it checked.
//!
//! Pattern: Guard (the rule itself is `datadir::decide`; this applies it to a real folder through its descriptor).

use std::fs::File;
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use super::unix::create_private;
use crate::datadir::{Action, DataDirCause, DataDirError, Meta, decide};

/// Nobody else may be able to swap anything up the path: each directory above is this user's or
/// root's, and only its owner writes there, or it is sticky like `/tmp`, where others cannot rename
/// what is ours. `dir` must be canonical, so a symlink in the path cannot hide one.
pub(super) fn check_ancestors(dir: &Path) -> Result<(), DataDirError> {
    let me = rustix::process::geteuid().as_raw();
    for ancestor in dir.ancestors().skip(1) {
        let meta = std::fs::metadata(ancestor)?;
        let foreign = meta.uid() != me && meta.uid() != 0;
        if foreign || (meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0) {
            return Err(DataDirError::NotPrivate {
                cause: DataDirCause::Ancestor,
                path: ancestor.to_path_buf(),
            });
        }
    }
    Ok(())
}

/// Make the data folder private to this user and answer the canonical path. A folder of this user that is too open is
/// corrected through its descriptor (bits are only taken away); a link, a file or somebody else's folder is refused.
pub(super) fn prepare_private(dir: &Path) -> Result<PathBuf, DataDirError> {
    let refused = |cause| DataDirError::NotPrivate {
        cause,
        path: dir.to_path_buf(),
    };
    if std::fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(refused(DataDirCause::Symlink));
    }
    create_private(dir)?;
    let dir = dir.canonicalize()?;
    let me = rustix::process::geteuid().as_raw();
    // The folder itself, opened once: what is looked at and what is changed cannot be swapped in between.
    let file = File::open(&dir)?;
    let facts = |f: &File| -> io::Result<Meta> {
        let m = f.metadata()?;
        Ok(Meta {
            is_dir: m.is_dir(),
            is_symlink: false,
            uid: m.uid(),
            mode: m.mode() & 0o7777,
        })
    };
    let refused = |cause| DataDirError::NotPrivate {
        cause,
        path: dir.clone(),
    };
    match decide(&facts(&file)?, me) {
        Action::Keep => {}
        Action::Refuse(cause) => return Err(refused(cause)),
        Action::Tighten(mode) => {
            file.set_permissions(PermissionsExt::from_mode(mode))?;
            if facts(&file)?.mode & 0o077 != 0 {
                return Err(refused(DataDirCause::Mode));
            }
        }
    }
    check_ancestors(&dir)?;
    Ok(dir)
}
