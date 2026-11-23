//! Where a program listens and how it is reached: one implementation per system behind [`Endpoint`].
//! Pattern: Facade (declarations).

mod endpoint;
mod peer;
#[cfg(unix)]
mod peer_unix;
mod socket;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
mod unix_dir;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::beat;

pub(super) use endpoint::{Accepted, Endpoint};
pub use peer::{Caller, CallerError, CallerSource, PerConnection};
pub(super) use peer::{Shared, bootstrap_for};
pub use socket::{BoundSocket, bind_socket, companion_path, default_socket};
#[cfg(unix)]
pub(super) use unix::Unix as System;
#[cfg(windows)]
pub(super) use windows::Windows as System;
