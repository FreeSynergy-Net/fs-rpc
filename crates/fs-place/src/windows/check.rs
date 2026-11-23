//! The Windows version of the check (plan WIN-HOST, rules 2.1 to 2.6). One part of a way: a local disk path without tricks, no
//! reparse point, owned by SYSTEM, Administrators or `TrustedInstaller`, and no allow entry of anybody else that writes.
//! Pattern: Specification (a pure check over owner and ACL; no state).

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, Prefix};
use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{GENERIC_ALL, GENERIC_WRITE};
use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
    DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, INHERIT_ONLY_ACE, IsValidSid,
    OWNER_SECURITY_INFORMATION, PSID,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_APPEND_DATA, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DELETE_CHILD,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
};
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
    ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_ALLOWED_COMPOUND_ACE_TYPE,
    ACCESS_ALLOWED_OBJECT_ACE_TYPE, MAXIMUM_ALLOWED,
};

use super::acl::{Descriptor, wide};
use super::sid::Sid;
use crate::{Level, PlaceRefusal};

/// SYSTEM, Administrators, `TrustedInstaller`. Whoever is one of them can do everything anyway; the check does not protect against that.
const SYSTEM_SIDS: [&str; 3] = [
    "S-1-5-18",
    "S-1-5-32-544",
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
];

/// Every right that lets somebody change what is in the part or the part itself.
const WRITES: u32 = FILE_WRITE_DATA
    | FILE_APPEND_DATA
    | FILE_WRITE_EA
    | FILE_WRITE_ATTRIBUTES
    | FILE_DELETE_CHILD
    | DELETE
    | WRITE_DAC
    | WRITE_OWNER
    | GENERIC_WRITE
    | GENERIC_ALL
    | MAXIMUM_ALLOWED;
/// Above the anchor only what can swap the child: "create a folder" in `C:\` does not replace a present `Program Files` (rule 2.5).
const SWAPS: u32 = FILE_DELETE_CHILD
    | DELETE
    | WRITE_DAC
    | WRITE_OWNER
    | GENERIC_WRITE
    | GENERIC_ALL
    | MAXIMUM_ALLOWED;

pub(crate) fn system_owners() -> Vec<Sid> {
    static OWNERS: OnceLock<Vec<Sid>> = OnceLock::new();
    // A SID that cannot be made is left out: fewer owners are trusted, never more.
    OWNERS
        .get_or_init(|| {
            SYSTEM_SIDS
                .iter()
                .filter_map(|t| Sid::from_text(t).ok())
                .collect()
        })
        .clone()
}

/// Rule 2.1: a path of a local disk, nothing else (no UNC, network or device path, no `:` after the drive, no `.` or `..`).
fn shape(part: &Path) -> Result<(), PlaceRefusal> {
    if !part.is_absolute() {
        return Err(PlaceRefusal::NotAbsolute);
    }
    let mut parts = part.components();
    match parts.next() {
        Some(Component::Prefix(p))
            if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)) => {}
        _ => return Err(PlaceRefusal::NotAbsolute),
    }
    for c in parts {
        match c {
            Component::RootDir => {}
            Component::Normal(n) if !n.encode_wide().any(|u| u == u16::from(b':')) => {}
            _ => return Err(PlaceRefusal::NotAbsolute),
        }
    }
    Ok(())
}

pub(crate) fn check(owners: &[Sid], part: &Path, level: Level) -> Result<(), PlaceRefusal> {
    shape(part)?;
    // Rule 2.2: the attribute, not `is_symlink()` (which knows less kinds of link).
    let meta = std::fs::symlink_metadata(part).map_err(|_| PlaceRefusal::Unreadable)?;
    if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(PlaceRefusal::Symlink);
    }
    let (mut owner, mut dacl, mut raw): (PSID, *mut ACL, _) = (null_mut(), null_mut(), null_mut());
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide(part.as_os_str()).as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &raw mut owner,
            null_mut(),
            &raw mut dacl,
            null_mut(),
            &raw mut raw,
        )
    };
    if status != 0 {
        return Err(PlaceRefusal::Unreadable);
    }
    // `owner` and `dacl` point into this descriptor.
    let _descriptor = Descriptor::from_raw(raw);
    // Rule 2.3.
    if owner.is_null() || !owners.iter().any(|o| o.equals_raw(owner)) {
        return Err(PlaceRefusal::WrongOwner);
    }
    // Rule 2.4 and 2.5.
    let limit = match level {
        Level::Anchor => WRITES,
        Level::AboveAnchor => SWAPS,
    };
    dacl_is_closed(owners, dacl, limit)
}

/// Rule 2.4: a missing DACL, or an allow entry of somebody else (not only inherited) that gives a right of `limit`, is refused.
/// Deny entries are not counted: that can judge too strict, never too lax (the counterpart of `mode & 0o022`).
fn dacl_is_closed(owners: &[Sid], dacl: *mut ACL, limit: u32) -> Result<(), PlaceRefusal> {
    // A null DACL gives everybody everything.
    if dacl.is_null() {
        return Err(PlaceRefusal::Writable);
    }
    let mut size = ACL_SIZE_INFORMATION::default();
    let ok = unsafe {
        GetAclInformation(
            dacl,
            (&raw mut size).cast(),
            u32::try_from(size_of::<ACL_SIZE_INFORMATION>()).unwrap_or(0),
            AclSizeInformation,
        )
    };
    if ok == 0 {
        return Err(PlaceRefusal::Unreadable);
    }
    for index in 0..size.AceCount {
        let mut ace: *mut c_void = null_mut();
        if unsafe { GetAce(dacl, index, &raw mut ace) } == 0 || ace.is_null() {
            return Err(PlaceRefusal::Unreadable);
        }
        let header = unsafe { *ace.cast::<ACE_HEADER>() };
        match u32::from(header.AceType) {
            ACCESS_ALLOWED_ACE_TYPE => {}
            // Kinds that allow with a condition or an object: not understood, so refused.
            ACCESS_ALLOWED_COMPOUND_ACE_TYPE
            | ACCESS_ALLOWED_OBJECT_ACE_TYPE
            | ACCESS_ALLOWED_CALLBACK_ACE_TYPE
            | ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE => return Err(PlaceRefusal::Writable),
            // Deny, audit, label ...: they give nothing.
            _ => continue,
        }
        if u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0 {
            continue;
        }
        let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
        let (mask, sid): (u32, PSID) = unsafe { ((*ace).Mask, (&raw mut (*ace).SidStart).cast()) };
        if unsafe { IsValidSid(sid) } == 0 {
            return Err(PlaceRefusal::Unreadable);
        }
        if mask & limit != 0 && !owners.iter().any(|o| o.equals_raw(sid)) {
            return Err(PlaceRefusal::Writable);
        }
    }
    Ok(())
}
