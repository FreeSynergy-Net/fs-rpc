//! Who is on the other end: a process, a pipe peer, the account name of a caller (split from `sid`; re-exported there).
//! Pattern: Specification (is this peer this user; no state). The Windows side of what `SO_PEERCRED` does on Unix.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr::null_mut;

use windows_sys::Win32::Security::{LookupAccountSidW, SID_NAME_USE};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

use super::sid::{Handle, Sid, me, token_of, who_of};

/// Does the process `pid` run as this user? A process that cannot be looked at (gone, or of somebody else) is not.
#[must_use]
pub fn process_is_me(pid: u32) -> bool {
    let Ok(me) = me() else { return false };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = Handle(process);
    token_of(process.0)
        .and_then(|t| who_of(&t))
        .is_ok_and(|w| me.user.equals(&w.user))
}

/// Does the client of this pipe instance run as this user?
#[must_use]
pub fn client_is_me(pipe: &impl AsRawHandle) -> bool {
    let mut pid = 0_u32;
    unsafe {
        GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid) != 0 && process_is_me(pid)
    }
}

/// Does the server of this pipe run as this user?
#[must_use]
pub fn server_is_me(pipe: &impl AsRawHandle) -> bool {
    let mut pid = 0_u32;
    unsafe {
        GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut pid) != 0 && process_is_me(pid)
    }
}

/// The account name of a SID (`name` of `DOMAIN\\name`); `None` if the system has none.
fn account_name(sid: &Sid) -> Option<String> {
    let (mut name_len, mut domain_len) = (0_u32, 0_u32);
    let mut kind: SID_NAME_USE = 0;
    // The first call only asks how long the buffers must be.
    unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid.ptr(),
            null_mut(),
            &raw mut name_len,
            null_mut(),
            &raw mut domain_len,
            &raw mut kind,
        )
    };
    if name_len == 0 {
        return None;
    }
    let mut name = vec![0_u16; name_len as usize];
    let mut domain = vec![0_u16; domain_len.max(1) as usize];
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            sid.ptr(),
            name.as_mut_ptr(),
            &raw mut name_len,
            domain.as_mut_ptr(),
            &raw mut domain_len,
            &raw mut kind,
        )
    };
    (ok != 0).then(|| String::from_utf16_lossy(&name[..name_len as usize]))
}

/// The account name of the user of this process (`None` if the system has none); to compare with a caller.
#[must_use]
pub fn my_account_name() -> Option<String> {
    me().ok().and_then(|m| account_name(&m.user))
}

/// Who runs the client process of this pipe instance, from the process token (the SID, and the account name if the system has
/// one); asked again for every connection.
///
/// # Errors
/// The pipe or the process cannot be looked at.
pub fn client_caller(pipe: &impl AsRawHandle) -> io::Result<(String, Option<String>)> {
    let mut pid = 0_u32;
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = Handle(process);
    let who = who_of(&token_of(process.0)?)?;
    Ok((who.user.text()?, account_name(&who.user)))
}
