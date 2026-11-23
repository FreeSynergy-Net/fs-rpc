//! Tests of the entry kinds of the check (the allowlist of rule 2.4). Pattern: Test.

use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_CALLBACK_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE,
    ACCESS_ALLOWED_COMPOUND_ACE_TYPE, ACCESS_ALLOWED_OBJECT_ACE_TYPE, SYSTEM_AUDIT_ACE_TYPE,
};

use super::check::{AceKind, kind_of};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE, ACCESS_DENIED_CALLBACK_ACE_TYPE,
    ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_DENIED_OBJECT_ACE_TYPE,
};

#[test]
fn only_the_plain_allow_entry_is_looked_at_and_only_the_four_deny_kinds_are_skipped() {
    assert_eq!(kind_of(ACCESS_ALLOWED_ACE_TYPE), AceKind::Check);
    for deny in [
        ACCESS_DENIED_ACE_TYPE,
        ACCESS_DENIED_OBJECT_ACE_TYPE,
        ACCESS_DENIED_CALLBACK_ACE_TYPE,
        ACCESS_DENIED_CALLBACK_OBJECT_ACE_TYPE,
    ] {
        assert_eq!(kind_of(deny), AceKind::GivesNothing, "{deny}");
    }
}

#[test]
fn every_other_kind_is_refused_known_or_not() {
    for other in [
        ACCESS_ALLOWED_COMPOUND_ACE_TYPE,
        ACCESS_ALLOWED_OBJECT_ACE_TYPE,
        ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
        ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE,
        SYSTEM_AUDIT_ACE_TYPE,
        0x42,
        0xFF,
    ] {
        assert_eq!(kind_of(other), AceKind::Gives, "{other}");
    }
}

/// The system stores no generic right or `MAXIMUM_ALLOWED` in the DACL of a file (it maps them), so a real file cannot show them:
/// the lists are checked as lists, one right each.
#[test]
fn every_right_that_writes_is_in_the_list_and_only_those_that_swap_are_in_the_short_one() {
    use super::check::{swaps, writes};
    use windows_sys::Win32::Foundation::{GENERIC_ALL, GENERIC_WRITE};
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_APPEND_DATA, FILE_DELETE_CHILD, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA,
        FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
    };
    use windows_sys::Win32::System::SystemServices::MAXIMUM_ALLOWED;

    for (name, right, swaps) in [
        ("FILE_WRITE_DATA", FILE_WRITE_DATA, false),
        ("FILE_APPEND_DATA", FILE_APPEND_DATA, false),
        ("FILE_WRITE_EA", FILE_WRITE_EA, false),
        ("FILE_WRITE_ATTRIBUTES", FILE_WRITE_ATTRIBUTES, false),
        ("FILE_DELETE_CHILD", FILE_DELETE_CHILD, true),
        ("DELETE", DELETE, true),
        ("WRITE_DAC", WRITE_DAC, true),
        ("WRITE_OWNER", WRITE_OWNER, true),
        ("GENERIC_WRITE", GENERIC_WRITE, true),
        ("GENERIC_ALL", GENERIC_ALL, true),
        ("MAXIMUM_ALLOWED", MAXIMUM_ALLOWED, true),
    ] {
        assert_eq!(writes() & right, right, "{name} in WRITES");
        assert_eq!(swaps() & right == right, swaps, "{name} in SWAPS");
    }
}
