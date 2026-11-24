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

/// A `subst` drive for the length of a test (an alias of a folder, as a drive letter), removed when dropped. Both tests below use it:
/// they run in parallel, so a letter that is taken by the other test (`subst` fails) is skipped and the next one tried, there is no
/// "look, then act" race. Fails where no `subst` can be made (the real answer is then NOT proven). Pattern: RAII.
struct Subst(String);

impl Subst {
    fn new(dir: &std::path::Path) -> Self {
        for letter in ('R'..='Z').rev() {
            let drive = format!("{letter}:");
            if std::path::Path::new(&format!("{drive}\\")).exists() {
                continue;
            }
            let made = std::process::Command::new("subst")
                .arg(&drive)
                .arg(dir)
                .output()
                .unwrap();
            if made.status.success() {
                return Self(drive);
            }
        }
        panic!("subst did not work on any letter: the real answer is NOT proven");
    }

    fn letter(&self) -> u8 {
        self.0.as_bytes()[0]
    }
}

impl Drop for Subst {
    fn drop(&mut self) {
        let _ = std::process::Command::new("subst")
            .arg(&self.0)
            .arg("/d")
            .output();
    }
}

/// The real answer of the system for a `subst` drive (no `GetDriveTypeW`): it must begin with the DOS prefix `is_subst` looks for.
/// Pattern: Test.
#[test]
fn the_system_answer_for_a_real_subst_drive_is_a_subst_alias() {
    let dir = tempfile::tempdir().unwrap();
    let alias = Subst::new(dir.path());
    let target =
        dos_target(alias.letter()).expect("QueryDosDeviceW gave nothing for a subst drive");
    assert!(is_subst(&target), "{}", String::from_utf16_lossy(&target));
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

/// The wire of the answer: the volume is given as fixed, the target is the REAL `QueryDosDeviceW` answer. For a `subst` drive the
/// refusal can then only come from that answer, so a `local_disk` that hands on a fixed device instead of it (mutant M-nosubst)
/// turns this red; for the drive of the temp folder (real) it must say yes. Pattern: Test.
#[test]
fn local_disk_hands_the_real_drive_answer_to_the_judge() {
    let dir = tempfile::tempdir().unwrap();
    let real_letter = dir.path().to_string_lossy().as_bytes()[0].to_ascii_uppercase();
    assert_eq!(
        local_disk_with(Some(3), real_letter, dos_target),
        Ok(()),
        "a real drive"
    );
    let alias = Subst::new(dir.path());
    assert_eq!(
        local_disk_with(Some(3), alias.letter(), dos_target),
        Err(PlaceRefusal::NotLocalDisk)
    );
}
