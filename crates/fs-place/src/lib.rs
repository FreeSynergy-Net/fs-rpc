//! Is this part of a path the system's? The check behind "this came from the package manager / the installer": nobody but the
//! system owner owns it, nobody else can write it, and there is no link on the way. Unix: owner and mode. Windows: owner and
//! the ACL (decision of the plan WIN-HOST, rules 2.1 to 2.6; the plan's threat model: an ordinary local user, not an administrator).
//! Also holds the Windows identity and rights basics (`windows`), shared with fs-rpc.
//!
//! Security module. No relaxation: a doubt is a refusal.
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
