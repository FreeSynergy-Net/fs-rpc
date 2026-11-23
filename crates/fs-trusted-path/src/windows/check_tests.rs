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
