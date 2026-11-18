//! Who is on the other end: the OS user of the connection, as ONE form (a name; the number when the system knows no name), and the
//! actor derived from it.
//!
//! Pattern: Adapter (system answer -> actor reference). The service's transport (fs-rpc) admits only a peer whose effective user is
//! its own (`SO_PEERCRED`, `geteuid` semantics), so the peer's user is `geteuid()`; it is never read from the environment.

use fs_types::{BasicValue, Reference};

/// The user name of `uid` in the text of `/etc/passwd`, if the text names it.
#[must_use]
pub fn name_in_passwd(passwd: &str, uid: u32) -> Option<String> {
    passwd.lines().find_map(|line| {
        let mut f = line.split(':');
        let (name, _, id) = (f.next()?, f.next()?, f.next()?);
        (id.parse::<u32>() == Ok(uid) && !name.is_empty()).then(|| name.to_owned())
    })
}

/// The OS user of the connection: the name, else the number (one documented form); `unknown` where the system gives none.
#[must_use]
pub fn os_user_name() -> String {
    #[cfg(unix)]
    {
        let uid = rustix::process::geteuid().as_raw();
        std::fs::read_to_string("/etc/passwd")
            .ok()
            .and_then(|text| name_in_passwd(&text, uid))
            .unwrap_or_else(|| uid.to_string())
    }
    #[cfg(not(unix))]
    {
        "unknown".to_owned()
    }
}

/// The default actor of a connection: `urn:fs:os-user:<name>` (the number when the name is no IRI part).
///
/// # Panics
/// Never: the fallback is a fixed valid IRI.
#[must_use]
pub fn actor_of_os_user() -> Reference {
    let name = os_user_name();
    let usable = name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    let text = if usable {
        format!("urn:fs:os-user:{name}")
    } else {
        "urn:fs:os-user:unknown".to_owned()
    };
    Reference::parse(&text)
        .unwrap_or_else(|_| Reference::parse("urn:fs:os-user:unknown").expect("valid IRI"))
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
