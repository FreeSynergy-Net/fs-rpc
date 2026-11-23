//! Rule 2.1 of the Windows check: the form of a path (a drive letter, no tricks) and the drive (a local fixed disk).
//! Pattern: Specification (a pure check over the text of the path and the system's view of the drive).

use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, Prefix};

use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumePathNameW, QueryDosDeviceW};

use super::acl::wide;
use crate::PlaceRefusal;

/// `GetDriveTypeW` of a local fixed disk.
const DRIVE_FIXED: u32 = 3;
/// What `QueryDosDeviceW` gives for a `subst` drive (a real drive gives `\Device\HarddiskVolumeN`).
const SUBST_TARGET: [u8; 4] = *b"\\??\\";

/// Rule 2.1, the form: a path with a drive letter, nothing else (no UNC or device path, no `:` after the drive, no `.` or `..`).
/// Gives the drive letter; that the drive is a local disk is [`local_disk`].
pub(super) fn shape(part: &Path) -> Result<u8, PlaceRefusal> {
    if !part.is_absolute() {
        return Err(PlaceRefusal::NotAbsolute);
    }
    let mut parts = part.components();
    let letter = match parts.next() {
        Some(Component::Prefix(p)) => match p.kind() {
            Prefix::Disk(l) | Prefix::VerbatimDisk(l) => l,
            _ => return Err(PlaceRefusal::NotAbsolute),
        },
        _ => return Err(PlaceRefusal::NotAbsolute),
    };
    for c in parts {
        match c {
            Component::RootDir => {}
            Component::Normal(n) if !n.encode_wide().any(|u| u == u16::from(b':')) => {}
            _ => return Err(PlaceRefusal::NotAbsolute),
        }
    }
    Ok(letter)
}

/// Rule 2.1, the drive: a local fixed disk, nothing else. A network drive (`net use`) would give the security descriptor of
/// another machine; a `subst` drive is an alias of any folder (also a foreign one); removable media can be swapped. So the volume
/// of the part must be `DRIVE_FIXED`, and the drive letter must not be a `subst` alias (its target begins with `\??\`).
pub(super) fn local_disk(part: &Path, letter: u8) -> Result<(), PlaceRefusal> {
    const SIZE: u32 = 1024;
    let mut root = vec![0_u16; SIZE as usize];
    let found =
        unsafe { GetVolumePathNameW(wide(part.as_os_str()).as_ptr(), root.as_mut_ptr(), SIZE) };
    if found == 0 || unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_FIXED {
        return Err(PlaceRefusal::NotLocalDisk);
    }
    let device = [u16::from(letter), u16::from(b':'), 0];
    let mut target = vec![0_u16; SIZE as usize];
    let len = unsafe { QueryDosDeviceW(device.as_ptr(), target.as_mut_ptr(), SIZE) };
    let subst = target.starts_with(&SUBST_TARGET.map(u16::from));
    if len == 0 || subst {
        return Err(PlaceRefusal::NotLocalDisk);
    }
    Ok(())
}
