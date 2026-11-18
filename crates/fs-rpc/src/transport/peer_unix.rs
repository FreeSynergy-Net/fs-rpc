//! The caller of a Unix connection. Pattern: Adapter (`SO_PEERCRED`, read again for every connection, to [`CallerSource`]).
//!
//! The kernel fills `SO_PEERCRED` with the effective user id the peer had when it connected (`geteuid` semantics, the same one
//! [`is_me`](super::unix) compares against), not the real one. The name comes from the system's passwd file by that id.

use tokio::net::UnixStream;

use super::peer::{Caller, CallerError, CallerSource, passwd_name};

impl CallerSource for UnixStream {
    fn caller(&self) -> Result<Caller, CallerError> {
        let uid = self.peer_cred().map_err(CallerError::Credentials)?.uid();
        let name = std::fs::read_to_string("/etc/passwd")
            .ok()
            .and_then(|text| passwd_name(&text, uid));
        Ok(Caller::new(uid.to_string(), name))
    }
}
