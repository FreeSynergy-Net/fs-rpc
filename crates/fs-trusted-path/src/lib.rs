//! Is this part of a path the system's? The check behind "this came from the package manager / the installer": nobody but the
//! system owner owns it, nobody else can write it, and there is no link on the way. Unix: owner and mode. Windows: owner and
//! the ACL (decision of the plan WIN-HOST, rules 2.1 to 2.6; the plan's threat model: an ordinary local user, not an administrator).
//! Also holds the Windows identity and rights basics (`windows`), shared with fs-rpc.
//!
//! Security module. No relaxation: a doubt is a refusal.
//!
//! Limits, said openly (Windows):
//! - "Elevated = Administrators = trusted", like root or sudo on Unix: whatever an elevated process of any administrator makes is owned
//!   by Administrators and counts as the system's. UAC is not a security boundary (Microsoft says so). Holders of `SeRestorePrivilege`
//!   or `SeBackupPrivilege` (Backup Operators) pass any ACL, like `CAP_DAC_OVERRIDE`.
//! - The check looks at the path, not at a handle. It holds only while every part above is closed too: the caller walks the way
//!   from the top and checks each part. A race remains only against accounts that are trusted anyway.
//! - Only a local disk counts: only what the system calls a fixed disk (`DRIVE_FIXED`) and no `subst` alias count; network drives and `subst` aliases are refused (`NotLocalDisk`). A USB hard disk often says "fixed" too, so the anchor must come from the system (`FOLDERID_ProgramFiles`), never from a path of the user.
//! - A process of the same user at the same integrity level is no border (e.g. the drive letters of a session).
//!
//! Pattern: Specification ([`Trust`]) + Value ([`PlaceRefusal`] names the border hit).

mod refusal;
mod trust;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
pub mod windows;

pub use refusal::{Level, PlaceRefusal};
pub use trust::{Trust, system_owned};

#[cfg(not(any(unix, windows)))]
pub use trust::Owner;
#[cfg(unix)]
pub use unix::Owner;
#[cfg(windows)]
pub use windows::sid::Sid as Owner;
