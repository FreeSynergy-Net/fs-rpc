//! Who a process is: the user of a token, and the check "is the other end of the pipe this user".
//! Pattern: Value object ([`Sid`]) + RAII ([`Handle`]). The Windows side of what `SO_PEERCRED` does on Unix.

use std::io;
use std::os::windows::io::RawHandle;
use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    EqualSid, GetLengthSid, GetTokenInformation, LookupAccountSidW, PSID, SID_NAME_USE,
    TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER, TokenOwner, TokenUser,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// A handle that is closed when dropped.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // Nothing to do if closing fails.
        unsafe { CloseHandle(self.0) };
    }
}

/// A security identifier, copied out of the system's buffer.
pub(super) struct Sid(Vec<u32>);

impl Sid {
    fn copy(sid: PSID) -> Self {
        let len = unsafe { GetLengthSid(sid) } as usize;
        let mut words = vec![0_u32; len.div_ceil(4)];
        unsafe {
            std::ptr::copy_nonoverlapping(sid.cast::<u8>(), words.as_mut_ptr().cast::<u8>(), len);
        };
        Self(words)
    }

    pub(super) fn ptr(&self) -> PSID {
        self.0.as_ptr().cast_mut().cast()
    }

    pub(super) fn equals(&self, other: PSID) -> bool {
        unsafe { EqualSid(self.ptr(), other) != 0 }
    }

    /// `S-1-5-21-...`
    pub(super) fn text(&self) -> io::Result<String> {
        let mut out: *mut u16 = null_mut();
        if unsafe { ConvertSidToStringSidW(self.ptr(), &raw mut out) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while unsafe { *out.add(len) } != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(out, len) });
        unsafe { LocalFree(out.cast()) };
        Ok(text)
    }
}

/// The user and the default owner of a token. Files made by an elevated process are owned by a group, the owner of the token.
pub(super) struct Who {
    pub(super) user: Sid,
    pub(super) owner: Sid,
}

fn token_buffer(token: HANDLE, class: i32) -> io::Result<Vec<u64>> {
    let mut need = 0_u32;
    unsafe { GetTokenInformation(token, class, null_mut(), 0, &raw mut need) };
    if need == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0_u64; (need as usize).div_ceil(8)];
    let ok = unsafe {
        GetTokenInformation(
            token,
            class,
            buffer.as_mut_ptr().cast(),
            need,
            &raw mut need,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn who_of(token: &Handle) -> io::Result<Who> {
    let user = token_buffer(token.0, TokenUser)?;
    let owner = token_buffer(token.0, TokenOwner)?;
    // The buffers hold a TOKEN_USER and a TOKEN_OWNER; the SIDs they point to lie inside the same buffers.
    let user = Sid::copy(unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid });
    let owner = Sid::copy(unsafe { (*owner.as_ptr().cast::<TOKEN_OWNER>()).Owner });
    Ok(Who { user, owner })
}

fn token_of(process: HANDLE) -> io::Result<Handle> {
    let mut token: HANDLE = null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(token))
}

/// The user this process runs as; read once.
pub(super) fn me() -> io::Result<&'static Who> {
    static ME: OnceLock<Option<Who>> = OnceLock::new();
    ME.get_or_init(|| {
        token_of(unsafe { GetCurrentProcess() })
            .and_then(|t| who_of(&t))
            .ok()
    })
    .as_ref()
    .ok_or_else(|| io::Error::other("the user of this process is not known"))
}

/// `S-1-5-21-...` of this process.
pub(super) fn my_sid_text() -> io::Result<String> {
    me()?.user.text()
}

/// Does the process `pid` run as this user? A process that cannot be looked at (gone, or of somebody else) is not.
pub(super) fn process_is_me(pid: u32) -> bool {
    let Ok(me) = me() else { return false };
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = Handle(process);
    token_of(process.0)
        .and_then(|t| who_of(&t))
        .is_ok_and(|w| me.user.equals(w.user.ptr()))
}

/// Does the client of this pipe instance run as this user?
pub(super) fn client_is_me(pipe: RawHandle) -> bool {
    let mut pid = 0_u32;
    unsafe { GetNamedPipeClientProcessId(pipe, &raw mut pid) != 0 && process_is_me(pid) }
}

/// Does the server of this pipe run as this user?
pub(super) fn server_is_me(pipe: RawHandle) -> bool {
    let mut pid = 0_u32;
    unsafe { GetNamedPipeServerProcessId(pipe, &raw mut pid) != 0 && process_is_me(pid) }
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

/// Who runs the client process of this pipe instance, from the process token (the SID, and the account name if the system has
/// one); asked again for every connection. Compiled on Windows only; not run by the authors (marked unchecked in the CHANGELOG).
pub(super) fn client_caller(pipe: RawHandle) -> io::Result<(String, Option<String>)> {
    let mut pid = 0_u32;
    if unsafe { GetNamedPipeClientProcessId(pipe, &raw mut pid) } == 0 {
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
