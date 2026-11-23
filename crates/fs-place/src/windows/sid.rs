//! Who a process is: the user of a token, as a value (`Sid`, `Who`, `me`). The peer checks live in `peer` and are re-exported here.
//! Pattern: Value object ([`Sid`]) + RAII ([`Handle`]).

use std::io;
use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows_sys::Win32::Security::{
    EqualSid, GetLengthSid, GetTokenInformation, PSID, TOKEN_OWNER, TOKEN_QUERY, TOKEN_USER,
    TokenOwner, TokenUser,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

pub use super::peer::{client_caller, client_is_me, my_account_name, process_is_me, server_is_me};

/// A handle that is closed when dropped.
pub(super) struct Handle(pub(super) HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // Nothing to do if closing fails.
        unsafe { CloseHandle(self.0) };
    }
}

/// A security identifier, copied out of the system's buffer.
#[derive(Clone, Debug)]
pub struct Sid(Vec<u32>);

impl Sid {
    fn copy(sid: PSID) -> Self {
        let len = unsafe { GetLengthSid(sid) } as usize;
        let mut words = vec![0_u32; len.div_ceil(4)];
        unsafe {
            std::ptr::copy_nonoverlapping(sid.cast::<u8>(), words.as_mut_ptr().cast::<u8>(), len);
        };
        Self(words)
    }

    /// The system's view of the SID; valid as long as `self` lives.
    #[must_use]
    pub fn ptr(&self) -> PSID {
        self.0.as_ptr().cast_mut().cast()
    }

    /// Are the two the same SID?
    #[must_use]
    pub fn equals(&self, other: &Sid) -> bool {
        self.equals_raw(other.ptr())
    }

    /// Is `other` (a SID inside a buffer of the system, valid) the same SID?
    pub(crate) fn equals_raw(&self, other: PSID) -> bool {
        unsafe { EqualSid(self.ptr(), other) != 0 }
    }

    /// The SID of a text such as `S-1-5-18`.
    ///
    /// # Errors
    /// The text is not a SID.
    pub fn from_text(text: &str) -> io::Result<Self> {
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let mut out: PSID = null_mut();
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &raw mut out) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let sid = Self::copy(out);
        unsafe { LocalFree(out.cast()) };
        Ok(sid)
    }

    /// `S-1-5-21-...`
    ///
    /// # Errors
    /// The system cannot make the text.
    pub fn text(&self) -> io::Result<String> {
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
pub struct Who {
    pub user: Sid,
    pub owner: Sid,
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

pub(super) fn who_of(token: &Handle) -> io::Result<Who> {
    let user = token_buffer(token.0, TokenUser)?;
    let owner = token_buffer(token.0, TokenOwner)?;
    // The buffers hold a TOKEN_USER and a TOKEN_OWNER; the SIDs they point to lie inside the same buffers.
    let user = Sid::copy(unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid });
    let owner = Sid::copy(unsafe { (*owner.as_ptr().cast::<TOKEN_OWNER>()).Owner });
    Ok(Who { user, owner })
}

pub(super) fn token_of(process: HANDLE) -> io::Result<Handle> {
    let mut token: HANDLE = null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(Handle(token))
}

/// The user this process runs as; read once.
///
/// # Errors
/// The user of this process is not known.
pub fn me() -> io::Result<&'static Who> {
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
///
/// # Errors
/// The user of this process is not known.
pub fn my_sid_text() -> io::Result<String> {
    me()?.user.text()
}

/// The user of this process and the default owner of what its token makes: both are "me" as an owner (an elevated process owns
/// its files as a group).
///
/// # Errors
/// The user of this process is not known.
pub fn my_owners() -> io::Result<Vec<Sid>> {
    let me = me()?;
    Ok(vec![me.user.clone(), me.owner.clone()])
}
