//! The rights of a pipe and of a data folder: this user alone (moved from fs-rpc; the pipe itself stays there). Pattern: Factory
//! (a descriptor for this user, a folder made private) + RAII ([`Descriptor`]).

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW, SDDL_REVISION_1,
    SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, OWNER_SECURITY_INFORMATION,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
};

use super::sid::{me, my_sid_text};

/// What the one entry of [`Descriptor::only_me`] is inherited by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inherit {
    /// Nothing below it: a pipe.
    None,
    /// Every folder and file below it: a data folder.
    FoldersAndFiles,
}

/// A security descriptor the system allocated.
pub struct Descriptor(PSECURITY_DESCRIPTOR);

impl Descriptor {
    /// A protected descriptor with one entry: full access for this user, with the inheritance `inherit` (a closed list: no text
    /// of the caller ever reaches the descriptor).
    ///
    /// # Errors
    /// The user is not known, or the system refuses.
    pub fn only_me(inherit: Inherit) -> io::Result<Self> {
        only_me(inherit)
    }

    /// Take over a descriptor the system allocated (it is freed on drop).
    pub(crate) fn from_raw(raw: PSECURITY_DESCRIPTOR) -> Self {
        Self(raw)
    }

    /// The system's view of it; valid as long as `self` lives.
    #[must_use]
    pub fn as_ptr(&self) -> PSECURITY_DESCRIPTOR {
        self.0
    }
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

pub(crate) fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

fn only_me(inherit: Inherit) -> io::Result<Descriptor> {
    let flags = match inherit {
        Inherit::None => "",
        Inherit::FoldersAndFiles => "OICI",
    };
    let sddl = format!("D:P(A;{flags};FA;;;{})", my_sid_text()?);
    let mut out: PSECURITY_DESCRIPTOR = null_mut();
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide(sddl.as_ref()).as_ptr(),
            SDDL_REVISION_1,
            &raw mut out,
            null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Descriptor(out))
}

fn code(error: u32) -> io::Result<()> {
    if error == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            i32::try_from(error).unwrap_or(i32::MAX),
        ))
    }
}

/// Is the owner of `dir` this user (or the owner a token of this user gives to what it makes)?
fn owned_by_me(dir: &Path) -> io::Result<bool> {
    let me = me()?;
    let (mut owner, mut descriptor): (PSID, PSECURITY_DESCRIPTOR) = (null_mut(), null_mut());
    code(unsafe {
        GetNamedSecurityInfoW(
            wide(dir.as_os_str()).as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &raw mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &raw mut descriptor,
        )
    })?;
    let descriptor = Descriptor(descriptor);
    let mine = me.user.equals_raw(owner) || me.owner.equals_raw(owner);
    drop(descriptor);
    Ok(mine)
}

/// Create `dir` (and what is above), refuse one that another user owns, and leave rights for this user alone: nothing inherited.
///
/// # Errors
/// Another user owns it (`PermissionDenied`), or the system refuses.
pub fn make_private(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    if !owned_by_me(dir)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} belongs to another user", dir.display()),
        ));
    }
    let descriptor = only_me(Inherit::FoldersAndFiles)?;
    let (mut present, mut defaulted, mut acl): (i32, i32, *mut ACL) = (0, 0, null_mut());
    if unsafe {
        GetSecurityDescriptorDacl(
            descriptor.0,
            &raw mut present,
            &raw mut acl,
            &raw mut defaulted,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    code(unsafe {
        SetNamedSecurityInfoW(
            wide(dir.as_os_str()).as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            acl,
            null(),
        )
    })
}
