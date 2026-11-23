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

/// Rule 2.1, the drive: the system must call the volume a fixed disk ([`is_fixed`]) and the drive letter must not be a `subst`
/// alias ([`is_subst`]). Both are asked, each on its own; each has its own test. What the system calls fixed is all that is known:
/// a USB hard disk often says `DRIVE_FIXED` too, so this is no proof that the disk cannot be unplugged.
pub(super) fn local_disk(part: &Path, letter: u8) -> Result<(), PlaceRefusal> {
    const SIZE: u32 = 1024;
    let mut root = vec![0_u16; SIZE as usize];
    let found =
        unsafe { GetVolumePathNameW(wide(part.as_os_str()).as_ptr(), root.as_mut_ptr(), SIZE) };
    if found == 0 || !is_fixed(unsafe { GetDriveTypeW(root.as_ptr()) }) {
        return Err(PlaceRefusal::NotLocalDisk);
    }
    let device = [u16::from(letter), u16::from(b':'), 0];
    let mut target = vec![0_u16; SIZE as usize];
    let len = unsafe { QueryDosDeviceW(device.as_ptr(), target.as_mut_ptr(), SIZE) };
    if len == 0 || is_subst(&target) {
        return Err(PlaceRefusal::NotLocalDisk);
    }
    Ok(())
}

/// The answer of `GetDriveTypeW` for a volume the system calls a fixed disk (a network drive, a CD or a card gives another value).
pub(super) fn is_fixed(drive_type: u32) -> bool {
    drive_type == DRIVE_FIXED
}

/// Whether the target `QueryDosDeviceW` gave for a drive letter is a `subst` alias (a real drive gives `\Device\...`).
pub(super) fn is_subst(target: &[u16]) -> bool {
    target.starts_with(&SUBST_TARGET.map(u16::from))
}
