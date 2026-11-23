//! The Windows check against real folders of the runner (their owners and ACLs are the ones WH0 recorded). The negative cases
//! need no `for_test`. Pattern: Test.
#![cfg(windows)]

use std::fs;
use std::path::Path;
use std::process::Command;

use fs_place::windows::sid::my_owners;
use fs_place::{Level, PlaceRefusal, Trust, system_owned};

fn p(text: &str) -> &Path {
    Path::new(text)
}

#[test]
fn program_files_is_the_systems_and_the_drive_above_it_is_too() {
    // owner TrustedInstaller, users only read; above the anchor "create folder" on `C:\` does not count (rule 2.5)
    assert_eq!(system_owned(p(r"C:\Program Files"), Level::Anchor), Ok(()));
    assert_eq!(system_owned(p(r"C:\"), Level::AboveAnchor), Ok(()));
}

#[test]
fn the_drive_itself_is_not_closed_below_the_anchor_rule() {
    // `Authenticated Users` may create a folder in `C:\`: as an anchor that is a right to write
    assert_eq!(
        system_owned(p(r"C:\"), Level::Anchor),
        Err(PlaceRefusal::Writable)
    );
}

#[test]
fn a_folder_users_can_write_is_refused_without_for_test() {
    // `C:\ProgramData`: owner SYSTEM, Users may create files
    assert_eq!(
        system_owned(p(r"C:\ProgramData"), Level::Anchor),
        Err(PlaceRefusal::Writable)
    );
}

#[test]
fn a_folder_of_this_test_run_is_not_the_systems_without_for_test() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = dir.path().join("a.json");
    fs::write(&file, "{}").unwrap();
    // owned by this user or its group, and the user has full control: neither is the system
    let refused = system_owned(&file, Level::Anchor).unwrap_err();
    assert!(
        matches!(refused, PlaceRefusal::WrongOwner | PlaceRefusal::Writable),
        "{refused}"
    );
}

#[test]
fn for_test_with_the_user_trusts_what_the_user_owns() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = dir.path().join("a.json");
    fs::write(&file, "{}").unwrap();
    let trust = Trust::for_test(my_owners().unwrap());
    assert_eq!(trust.system_owned(&file, Level::Anchor), Ok(()));
    // somebody else's owner: the same file is refused
    let other = Trust::for_test(vec![fs_place::Owner::from_text("S-1-5-18").unwrap()]);
    assert_eq!(
        other.system_owned(&file, Level::Anchor),
        Err(PlaceRefusal::WrongOwner)
    );
    assert_eq!(
        trust.system_owned(&dir.path().join("none"), Level::Anchor),
        Err(PlaceRefusal::Unreadable)
    );
}

#[test]
fn a_junction_is_refused_even_with_the_right_owner() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let target = dir.path().join("target");
    fs::create_dir(&target).unwrap();
    let junction = dir.path().join("junction");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&target)
        .output()
        .unwrap();
    assert!(status.status.success(), "mklink failed");
    let trust = Trust::for_test(my_owners().unwrap());
    assert_eq!(
        trust.system_owned(&junction, Level::Anchor),
        Err(PlaceRefusal::Symlink)
    );
}

#[test]
fn paths_that_are_not_a_plain_local_disk_path_are_refused() {
    for bad in [
        r"\\server\share\x",
        r"\\?\GLOBALROOT\Device\x",
        r"\\.\pipe\x",
        r"C:\Windows\x:stream",
        r"C:\Windows\..\Windows",
        r"Windows",
        r"C:Windows",
    ] {
        assert_eq!(
            system_owned(p(bad), Level::Anchor),
            Err(PlaceRefusal::NotAbsolute),
            "{bad}"
        );
    }
}
