//! Who is on the other end: the actor and the OS user of the log, derived from the `Caller` the system named for the connection
//! (fs-rpc: `SO_PEERCRED` on Unix, the pipe client's token on Windows), and the lines the service logs.
//!
//! Pattern: Adapter (system answer -> actor reference). The caller is NEVER read from a field of the client, an argument or the environment.

use fs_rpc::Caller;
use fs_types::{BasicValue, Reference};

/// The OS user of the log: the NAME the system has for the caller (display), the number (`Caller::id`) when it has none. One form.
#[must_use]
pub fn os_user_of(caller: &Caller) -> Option<String> {
    Some(caller.name().unwrap_or_else(|| caller.id()).to_owned())
}

pub use fs_rpc::id_usable;

/// The default actor of a caller: `urn:fs:os-user:<id>` from `Caller::id` ONLY (the name is display; it can be chosen by whoever set up the
/// account). An id that is unusable ([`id_usable`]) is the actor `urn:fs:os-user:unknown`, NEVER the name; the service does not even get
/// that far: it refuses such a connection (`caller-unknown`, see [`crate::SettingsCallers`]).
///
/// # Panics
/// Never: the fallback is a fixed valid IRI.
#[must_use]
pub fn actor_of(caller: &Caller) -> Reference {
    let unknown = || Reference::parse("urn:fs:os-user:unknown").expect("valid IRI");
    if !id_usable(caller.id()) {
        return unknown();
    }
    Reference::parse(&format!("urn:fs:os-user:{}", caller.id())).unwrap_or_else(|_| unknown())
}

/// The line a claimed `by` becomes in the log: the claim as written (quoted, so an empty or odd one is visible) and that it was ignored.
#[must_use]
pub fn claim_line(by: &str) -> String {
    format!(
        "fs-settings-rpc: admin claimed-by={by:?} ignored (the actor is the identity of the connection)"
    )
}

/// The default `claimed_by` of a service: the claim goes to standard error (a host logs it its own way by setting `claimed_by`).
pub fn log_claim(by: &str) {
    eprintln!("{}", claim_line(by));
}

/// The line a connection the system could not name becomes in the log.
#[must_use]
pub fn refused_line(code: &str) -> String {
    format!("fs-settings-rpc: connection refused ({code}): the system did not say who it is")
}

/// The default `refused_note`: standard error.
pub fn log_refused(code: &str) {
    eprintln!("{}", refused_line(code));
}
