//! The Windows check of the drive (rule 2.1) on the runner: a `subst` drive and a network drive, made by the test itself.
//! The two parts of the rule (fixed disk, no alias) are tested each on its own with literal values in `form_tests.rs`.
//! Pattern: Test.
#![cfg(windows)]

mod common;

use std::path::Path;
use std::process::Command;

use common::file_with;
use fs_trusted_path::{Level, PlaceRefusal, system_owned};

/// A `subst` drive for the length of a test (an alias of a folder, as a drive letter): removed when dropped.
struct Subst(String);

impl Subst {
    fn new(dir: &Path) -> Self {
        for letter in ('R'..='Z').rev() {
            let drive = format!("{letter}:");
            if Path::new(&format!("{drive}\\")).exists() {
                continue;
            }
            let made = Command::new("subst").arg(&drive).arg(dir).output().unwrap();
            if made.status.success() {
                return Self(drive);
            }
        }
        panic!("no free drive letter for subst");
    }
}

impl Drop for Subst {
    fn drop(&mut self) {
        let _ = Command::new("subst").arg(&self.0).arg("/d").output();
    }
}

#[test]
fn a_subst_drive_is_refused_as_not_a_local_disk_although_the_file_is_the_systems() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = file_with(dir.path(), &[("S-1-5-18", "F"), ("S-1-5-32-544", "F")]);
    // the same file, by its real path, is the systems: owner and ACL are fine
    assert_eq!(system_owned(&file, Level::Anchor), Ok(()));
    let alias = Subst::new(dir.path());
    let by_alias = format!(r"{}\a.json", alias.0);
    assert!(Path::new(&by_alias).exists(), "{by_alias}");
    assert_eq!(
        system_owned(Path::new(&by_alias), Level::Anchor),
        Err(PlaceRefusal::NotLocalDisk),
        "{by_alias}"
    );
    let verbatim = format!(r"\\?\{by_alias}");
    assert_eq!(
        system_owned(Path::new(&verbatim), Level::Anchor),
        Err(PlaceRefusal::NotLocalDisk),
        "{verbatim}"
    );
}

/// A network drive for the length of a test: `net use` on the administrative share of this machine. `None` when `net use` fails (the
/// test then fails).
struct NetDrive(String);

impl NetDrive {
    fn new(share: char) -> Option<Self> {
        for letter in ('R'..='Y').rev() {
            let drive = format!("{letter}:");
            if Path::new(&format!("{drive}\\")).exists() {
                continue;
            }
            let made = Command::new("net")
                .args(["use", &drive, &format!(r"\\localhost\{share}$")])
                .output()
                .unwrap();
            let said = String::from_utf8_lossy(&made.stdout).to_string()
                + &String::from_utf8_lossy(&made.stderr);
            eprintln!("NETUSE {drive} rc={:?} {}", made.status.code(), said.trim());
            return made.status.success().then_some(Self(drive));
        }
        None
    }
}

impl Drop for NetDrive {
    fn drop(&mut self) {
        let _ = Command::new("net")
            .args(["use", &self.0, "/delete", "/y"])
            .output();
    }
}

#[test]
fn a_network_drive_is_refused_as_not_a_local_disk_although_the_file_is_the_systems() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = file_with(dir.path(), &[("S-1-5-18", "F"), ("S-1-5-32-544", "F")]);
    assert_eq!(system_owned(&file, Level::Anchor), Ok(()));
    let share = file.to_str().unwrap().chars().next().unwrap();
    // Not silent: if `net use` fails the test FAILS (the runner can do it, proven in the mutant run). A changed runner image must
    // not turn this test into an empty green one.
    let net = NetDrive::new(share)
        .expect("net use did not work on this machine: the network drive is NOT proven");
    // the same file by the network drive: D:\a\b -> Z:\a\b (the share D$ is the whole drive D:)
    let tail = file.to_str().unwrap().get(3..).unwrap();
    let by_net = format!(r"{}\{tail}", net.0);
    assert!(Path::new(&by_net).exists(), "{by_net}");
    assert_eq!(
        system_owned(Path::new(&by_net), Level::Anchor),
        Err(PlaceRefusal::NotLocalDisk),
        "{by_net}"
    );
    eprintln!("NETUSE-TESTED: {by_net} refused");
}
