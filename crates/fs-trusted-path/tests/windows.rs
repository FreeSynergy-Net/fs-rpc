//! The Windows check against real folders of the runner (their owners and ACLs are the ones WH0 recorded). The negative cases
//! need no `for_test`. Pattern: Test.
#![cfg(windows)]

use std::fs;
use std::path::Path;
use std::process::Command;

use fs_trusted_path::windows::sid::{my_owners, my_sid_text};
use fs_trusted_path::{Level, Owner, PlaceRefusal, Trust, system_owned};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};

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

// ---- M1: only a local fixed disk -----------------------------------------------------------------------------------------

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

// ---- M2: the owner, the rights, the null DACL, the entries the check cannot judge -----------------------------------------

/// Set the DACL of `file` from SDDL, protected (nothing inherited). The error is the code of the system.
fn try_set_dacl(file: &Path, sddl: &str) -> Result<(), u32> {
    fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        text.encode_wide().chain(std::iter::once(0)).collect()
    }
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(sddl.as_ref()).as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(converted, 0, "SDDL {sddl}");
    let (mut present, mut defaulted, mut acl): (i32, i32, *mut ACL) = (0, 0, std::ptr::null_mut());
    let got = unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &raw mut present,
            &raw mut acl,
            &raw mut defaulted,
        )
    };
    assert_ne!(got, 0);
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide(file.as_os_str()).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl,
            std::ptr::null(),
        )
    };
    unsafe { LocalFree(descriptor) };
    if status == 0 { Ok(()) } else { Err(status) }
}

/// A file with a DACL made from SDDL: SYSTEM and Administrators in full, then `more` (entries of SDDL).
fn file_with_sddl(dir: &Path, more: &str) -> std::path::PathBuf {
    let file = dir.join("a.json");
    fs::write(&file, "{}").unwrap();
    let sddl = format!("D:P(A;;FA;;;SY)(A;;FA;;;BA){more}");
    try_set_dacl(&file, &sddl).unwrap_or_else(|e| panic!("SetNamedSecurityInfoW {sddl}: {e}"));
    file
}

#[test]
fn a_file_of_the_user_with_a_closed_acl_is_a_wrong_owner_with_the_real_system_trust() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    // SYSTEM and Administrators only: the ACL is closed, so only the owner decides
    let file = file_with(dir.path(), &[("S-1-5-18", "F"), ("S-1-5-32-544", "F")]);
    assert_eq!(system_owned(&file, Level::Anchor), Ok(()));
    // the owner becomes this user (needs only the right to write the owner; the user is no member of the circle of trust)
    icacls(
        &file,
        &["/setowner", &format!("*{}", my_sid_text().unwrap())],
    );
    for level in [Level::Anchor, Level::AboveAnchor] {
        assert_eq!(system_owned(&file, level), Err(PlaceRefusal::WrongOwner));
    }
}

/// One test per right: a file that gives `Everyone` exactly this right is refused as an anchor; above the anchor it is refused
/// only if the right can swap the child (`swaps`). The masks are written out here, not taken from the code under test.
macro_rules! right {
    ($name:ident, $mask:literal, swaps = $swaps:literal) => {
        #[test]
        fn $name() {
            let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
            let file = file_with_sddl(dir.path(), concat!("(A;;", $mask, ";;;WD)"));
            assert_eq!(
                system_owned(&file, Level::Anchor),
                Err(PlaceRefusal::Writable),
                "as an anchor"
            );
            let above = system_owned(&file, Level::AboveAnchor);
            let swaps: bool = $swaps;
            if swaps {
                assert_eq!(above, Err(PlaceRefusal::Writable), "above the anchor");
            } else {
                assert_eq!(above, Ok(()), "above the anchor");
            }
        }
    };
}

right!(
    everyone_may_write_data_so_the_file_is_writable,
    "0x2",
    swaps = false
); // FILE_WRITE_DATA
right!(
    everyone_may_append_so_the_file_is_writable,
    "0x4",
    swaps = false
); // FILE_APPEND_DATA
right!(
    everyone_may_write_ea_so_the_file_is_writable,
    "0x10",
    swaps = false
); // FILE_WRITE_EA
right!(
    everyone_may_delete_a_child_so_the_file_is_writable,
    "0x40",
    swaps = true
); // FILE_DELETE_CHILD
right!(
    everyone_may_write_attributes_so_the_file_is_writable,
    "0x100",
    swaps = false
); // FILE_WRITE_ATTRIBUTES
right!(
    everyone_may_delete_so_the_file_is_writable,
    "0x10000",
    swaps = true
); // DELETE
right!(
    everyone_may_write_the_dacl_so_the_file_is_writable,
    "0x40000",
    swaps = true
); // WRITE_DAC
right!(
    everyone_may_write_the_owner_so_the_file_is_writable,
    "0x80000",
    swaps = true
); // WRITE_OWNER
right!(
    everyone_has_generic_write_so_the_file_is_writable,
    "0x40000000",
    swaps = true
); // GENERIC_WRITE
right!(
    everyone_has_generic_all_so_the_file_is_writable,
    "0x10000000",
    swaps = true
); // GENERIC_ALL
right!(
    everyone_has_maximum_allowed_so_the_file_is_writable,
    "0x2000000",
    swaps = true
); // MAXIMUM_ALLOWED

#[test]
fn rights_that_only_read_do_not_make_a_file_writable() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    // FILE_READ_DATA | FILE_READ_EA | FILE_EXECUTE | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE
    let file = file_with_sddl(dir.path(), "(A;;0x1200A9;;;WD)");
    for level in [Level::Anchor, Level::AboveAnchor] {
        assert_eq!(system_owned(&file, level), Ok(()));
    }
}

#[test]
fn a_file_without_a_dacl_is_writable_for_everybody() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = file_with_sddl(dir.path(), "");
    assert_eq!(system_owned(&file, Level::Anchor), Ok(()));
    try_set_dacl(&file, "D:NO_ACCESS_CONTROL").unwrap();
    for level in [Level::Anchor, Level::AboveAnchor] {
        assert_eq!(system_owned(&file, level), Err(PlaceRefusal::Writable));
    }
}

#[test]
fn an_allow_entry_with_a_condition_is_refused_even_with_a_harmless_mask() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    // a conditional allow of reading: the check cannot judge the condition, so it refuses
    let file = file_with_sddl(dir.path(), r#"(XA;;FR;;;WD;(@User.Title=="PM"))"#);
    for level in [Level::Anchor, Level::AboveAnchor] {
        assert_eq!(system_owned(&file, level), Err(PlaceRefusal::Writable));
    }
}

#[test]
fn an_object_allow_entry_is_refused_even_with_a_harmless_mask() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = file_with_sddl(
        dir.path(),
        "(OA;;CC;bf967aba-0de6-11d0-a285-00aa00305240;;WD)",
    );
    for level in [Level::Anchor, Level::AboveAnchor] {
        assert_eq!(system_owned(&file, level), Err(PlaceRefusal::Writable));
    }
}
