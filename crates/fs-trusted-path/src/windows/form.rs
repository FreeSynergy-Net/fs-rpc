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
    local_disk_with(part, letter, drive_type_of, dos_target)
}

/// [`local_disk`] with the two system questions as arguments (a seam, so a test can give one and keep the other real: the real
/// `subst` answer is then the ONLY reason for a refusal, and a wire that does not pass the answer on turns the test red).
pub(super) fn local_disk_with(
    part: &Path,
    letter: u8,
    drive_type: fn(&Path) -> Option<u32>,
    target: fn(u8) -> Option<Vec<u16>>,
) -> Result<(), PlaceRefusal> {
    judge(drive_type(part), target(letter).as_deref())
}

/// `GetDriveTypeW` of the volume of the path; `None` when the volume is not known.
pub(super) fn drive_type_of(part: &Path) -> Option<u32> {
    const SIZE: u32 = 1024;
    let mut root = vec![0_u16; SIZE as usize];
    let found =
        unsafe { GetVolumePathNameW(wide(part.as_os_str()).as_ptr(), root.as_mut_ptr(), SIZE) };
    (found != 0).then(|| unsafe { GetDriveTypeW(root.as_ptr()) })
}

/// `QueryDosDeviceW` of the drive letter: what the letter points to (`\Device\...` for a real drive, `\??\...` for a `subst`
/// alias); `None` when the letter is not known.
pub(super) fn dos_target(letter: u8) -> Option<Vec<u16>> {
    const SIZE: u32 = 1024;
    let device = [u16::from(letter), u16::from(b':'), 0];
    let mut target = vec![0_u16; SIZE as usize];
    let len = unsafe { QueryDosDeviceW(device.as_ptr(), target.as_mut_ptr(), SIZE) };
    (len != 0).then_some(target)
}

/// The decision of [`local_disk`] over what the system said, apart from the system calls (so each part is tested alone with literal
/// values): `drive_type` is `None` when the volume of the path is not known, `target` is `None` when the drive letter is not known.
/// Refused when the volume is unknown or not fixed, when the letter is unknown, or when the letter is a `subst` alias.
pub(super) fn judge(drive_type: Option<u32>, target: Option<&[u16]>) -> Result<(), PlaceRefusal> {
    if !drive_type.is_some_and(is_fixed) {
        return Err(PlaceRefusal::NotLocalDisk);
    }
    match target {
        Some(t) if !is_subst(t) => Ok(()),
        _ => Err(PlaceRefusal::NotLocalDisk),
    }
}

/// The answer of `GetDriveTypeW` for a volume the system calls a fixed disk (a network drive, a CD or a card gives another value).
pub(super) fn is_fixed(drive_type: u32) -> bool {
    drive_type == DRIVE_FIXED
}

/// Whether the target `QueryDosDeviceW` gave for a drive letter is a `subst` alias (a real drive gives `\Device\...`).
pub(super) fn is_subst(target: &[u16]) -> bool {
    target.starts_with(&SUBST_TARGET.map(u16::from))
}
