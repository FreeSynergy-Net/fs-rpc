//! The two parts of rule 2.1 for the drive, each on its own with the values the system gives (literals, not taken from the code
//! under test). The system's own `subst` drive fails both parts at once on the runner, so only values show each part alone.
//! Pattern: Test.

use super::form::{dos_target, is_fixed, is_subst, judge, local_disk_with};
use crate::PlaceRefusal;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[test]
fn only_the_drive_type_fixed_is_a_fixed_disk() {
    assert!(is_fixed(3), "DRIVE_FIXED");
    // DRIVE_UNKNOWN, DRIVE_NO_ROOT_DIR, DRIVE_REMOVABLE, DRIVE_REMOTE, DRIVE_CDROM, DRIVE_RAMDISK
    for other in [0, 1, 2, 4, 5, 6] {
        assert!(!is_fixed(other), "{other}");
    }
}

#[test]
fn a_target_that_begins_with_the_dos_prefix_is_a_subst_alias() {
    assert!(is_subst(&wide(r"\??\C:\folder")));
    assert!(is_subst(&wide(r"\??\UNC\server\share")));
}

#[test]
fn the_target_of_a_real_drive_is_no_subst_alias() {
    assert!(!is_subst(&wide(r"\Device\HarddiskVolume3")));
    assert!(!is_subst(&wide(
        r"\Device\LanmanRedirector\;Z:0\localhost\C$"
    )));
    assert!(
        !is_subst(&[0_u16; 16]),
        "an empty answer is no alias (the length decides)"
    );
}

/// The real answer of the system for a `subst` drive (no `GetDriveTypeW`): it must begin with the DOS prefix `is_subst` looks for.
/// Fails where `subst` cannot be made (it is not skipped: then the real answer is not proven). Pattern: Test.
#[test]
fn the_system_answer_for_a_real_subst_drive_is_a_subst_alias() {
    use std::path::Path;
    use std::process::Command;
    use windows_sys::Win32::Storage::FileSystem::QueryDosDeviceW;
    let dir = tempfile::tempdir().unwrap();
    let letter = ('R'..='Z')
        .rev()
        .find(|l| !Path::new(&format!("{l}:\\")).exists())
        .expect("no free drive letter");
    let drive = format!("{letter}:");
    let made = Command::new("subst")
        .arg(&drive)
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "subst did not work: the real answer is NOT proven"
    );
    let device = wide(&drive);
    let mut target = vec![0_u16; 1024];
    let len = unsafe { QueryDosDeviceW(device.as_ptr(), target.as_mut_ptr(), 1024) };
    let _ = Command::new("subst").arg(&drive).arg("/d").output();
    assert_ne!(len, 0);
    assert!(
        is_subst(&target),
        "{}",
        String::from_utf16_lossy(&target[..len as usize])
    );
}

/// What `judge` decides, each part alone: the other part is always fine (a fixed volume, a real drive) so only one part can refuse.
/// A mutant that drops either part of `local_disk` turns exactly one of these red. Pattern: Test.
#[test]
fn a_fixed_volume_on_a_real_drive_is_a_local_disk() {
    assert_eq!(
        judge(Some(3), Some(&wide(r"\Device\HarddiskVolume3"))),
        Ok(())
    );
}

#[test]
fn a_volume_that_is_not_fixed_is_refused_although_the_letter_is_no_alias() {
    let real = wide(r"\Device\LanmanRedirector\;Z:0\localhost\C$");
    for other in [0, 1, 2, 4, 5, 6] {
        assert_eq!(
            judge(Some(other), Some(&real)),
            Err(PlaceRefusal::NotLocalDisk),
            "{other}"
        );
    }
    assert_eq!(
        judge(None, Some(&real)),
        Err(PlaceRefusal::NotLocalDisk),
        "volume unknown"
    );
}

#[test]
fn a_subst_alias_is_refused_although_the_volume_is_fixed() {
    assert_eq!(
        judge(Some(3), Some(&wide(r"\??\C:\folder"))),
        Err(PlaceRefusal::NotLocalDisk)
    );
}

#[test]
fn an_unknown_drive_letter_is_refused_although_the_volume_is_fixed() {
    assert_eq!(judge(Some(3), None), Err(PlaceRefusal::NotLocalDisk));
}

fn fixed(_: &std::path::Path) -> Option<u32> {
    Some(3)
}

/// The wire of the answer: the volume is given as fixed, the target is the REAL `QueryDosDeviceW` answer. For a `subst` drive the
/// refusal can then only come from that answer, so a `local_disk` that hands on a fixed device instead of it (mutant M-nosubst)
/// turns this red; for the drive of the temp folder (real) it must say yes. Fails where `subst` cannot be made. Pattern: Test.
#[test]
fn local_disk_hands_the_real_drive_answer_to_the_judge() {
    use std::path::Path;
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let real_letter = dir.path().to_string_lossy().as_bytes()[0].to_ascii_uppercase();
    assert_eq!(
        local_disk_with(dir.path(), real_letter, fixed, dos_target),
        Ok(()),
        "a real drive"
    );
    let letter = ('R'..='Z')
        .rev()
        .find(|l| !Path::new(&format!("{l}:\\")).exists())
        .expect("no free drive letter");
    let drive = format!("{letter}:");
    let made = Command::new("subst")
        .arg(&drive)
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        made.status.success(),
        "subst did not work: the wire is NOT proven"
    );
    let verdict = local_disk_with(dir.path(), letter as u8, fixed, dos_target);
    let _ = Command::new("subst").arg(&drive).arg("/d").output();
    assert_eq!(verdict, Err(PlaceRefusal::NotLocalDisk));
}
