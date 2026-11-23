//! The two parts of rule 2.1 for the drive, each on its own with the values the system gives (literals, not taken from the code
//! under test). The system's own `subst` drive fails both parts at once on the runner, so only values show each part alone.
//! Pattern: Test.

use super::form::{is_fixed, is_subst};

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
/// Skipped where `subst` cannot be made (then the literal tests above are all there is). Pattern: Test.
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
