//! The Windows check against real folders of the runner (their owners and ACLs are the ones WH0 recorded). The negative cases
//! need no `for_test`. Pattern: Test.
#![cfg(windows)]

use std::fs;
use std::path::Path;
use std::process::Command;

use fs_place::windows::sid::{my_owners, my_sid_text};
use fs_place::{Level, Owner, PlaceRefusal, Trust, system_owned};

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

/// A file with exactly the rights `grants` (a SID text and a right each), no inheritance: a real ACL, made by icacls.
fn file_with(dir: &Path, grants: &[(&str, &str)]) -> std::path::PathBuf {
    let file = dir.join("a.json");
    fs::write(&file, "{}").unwrap();
    icacls(&file, &["/inheritance:r"]);
    for (sid, right) in grants {
        icacls(&file, &["/grant:r", &format!("*{sid}:{right}")]);
    }
    file
}

fn icacls(file: &Path, args: &[&str]) {
    let out = Command::new("icacls")
        .arg(file)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "icacls {args:?}: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn a_real_acl_of_the_system_counts_and_a_grant_to_anybody_else_does_not_without_for_test() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    // SYSTEM and Administrators only; the file is owned by the elevated runner, that is Administrators
    let file = file_with(dir.path(), &[("S-1-5-18", "F"), ("S-1-5-32-544", "F")]);
    assert_eq!(system_owned(&file, Level::Anchor), Ok(()));
    // Everyone may write: the same file is refused
    icacls(&file, &["/grant", "*S-1-1-0:W"]);
    assert_eq!(
        system_owned(&file, Level::Anchor),
        Err(PlaceRefusal::Writable)
    );
    // Everyone may only read (a right of the standard rule 2.4 list is missing): a closed file again
    icacls(&file, &["/grant:r", "*S-1-1-0:R"]);
    assert_eq!(system_owned(&file, Level::Anchor), Ok(()));
}

#[test]
fn the_profile_of_the_user_is_not_the_systems_without_for_test() {
    // the user has full control of the own profile: an allow entry of somebody who is not the system
    let profile = std::env::var("USERPROFILE").unwrap();
    let refused = system_owned(Path::new(&profile), Level::Anchor).unwrap_err();
    assert!(
        matches!(refused, PlaceRefusal::WrongOwner | PlaceRefusal::Writable),
        "{refused}"
    );
}

#[test]
fn for_test_trusts_only_the_owners_it_is_given() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let me = my_sid_text().unwrap();
    let file = file_with(dir.path(), &[(me.as_str(), "F"), ("S-1-5-18", "F")]);
    // the user's own entry is foreign to the real system ...
    let refused = system_owned(&file, Level::Anchor).unwrap_err();
    assert!(
        matches!(refused, PlaceRefusal::Writable | PlaceRefusal::WrongOwner),
        "{refused}"
    );
    // ... and trusted when the test says so (the file is owned by the user or by Administrators: both are "me")
    let mut owners = my_owners().unwrap();
    owners.push(Owner::from_text("S-1-5-18").unwrap());
    let trust = Trust::for_test(owners);
    assert_eq!(trust.system_owned(&file, Level::Anchor), Ok(()));
    // another owner list: wrong owner
    let other = Trust::for_test(vec![Owner::from_text("S-1-1-0").unwrap()]);
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
