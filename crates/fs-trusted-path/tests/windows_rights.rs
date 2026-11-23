//! The Windows check against files with ACLs the tests make themselves (SDDL, icacls): the owner, every right, the null DACL, the
//! entries the check cannot judge. The drive is in `windows_drive.rs`. Pattern: Test.
#![cfg(windows)]

mod common;

use std::fs;
use std::path::Path;

use common::{file_with, icacls};
use fs_trusted_path::windows::sid::my_sid_text;
use fs_trusted_path::{Level, PlaceRefusal, system_owned};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR,
};

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
    everyone_has_generic_all_so_the_file_is_writable,
    "0x10000000",
    swaps = true
); // GENERIC_ALL

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
