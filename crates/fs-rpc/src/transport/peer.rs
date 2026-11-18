//! Who is on the other end of a connection: the caller, read from the system for EVERY connection.
//!
//! Pattern: Strategy ([`CallerSource`], one per system: `SO_PEERCRED` on Unix, the client process token of the named pipe on
//! Windows; a fake in the tests) + Factory per connection ([`PerConnection`]: the program builds its capability for each caller).
//! The caller is read from the system and from nothing else: never from a field the client sends, an argument or the
//! environment, not even for an administrator. A program that needs "who asks" takes it from the [`Caller`] its capability was
//! built with, so every call on that connection has the same, unforgeable answer. A bot is told apart by its key, later (edge).

use std::fmt;
use std::io;

use capnp::capability::Client;

/// Who is on the other end: what the system says about the process that opened the connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caller {
    id: String,
    name: Option<String>,
}

impl Caller {
    /// A caller by the system's id and, if the system knows it, name.
    #[must_use]
    pub fn new(id: impl Into<String>, name: Option<String>) -> Self {
        Self {
            id: id.into(),
            name,
        }
    }

    /// The system's id of the user: the decimal user id (`1000`) on Unix, the SID (`S-1-5-21-...`) on Windows.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The system's name of the user (`/etc/passwd` on Unix, the account name on Windows); `None` if the system has none.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

/// The system could not say who is on the other end. The connection is refused; the code is `caller-credentials`.
#[derive(Debug)]
#[non_exhaustive]
pub enum CallerError {
    /// The system refused to tell (`SO_PEERCRED`, the pipe client's process or token).
    Credentials(io::Error),
}

impl CallerError {
    /// The stable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Credentials(_) => "caller-credentials",
        }
    }
}

impl fmt::Display for CallerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credentials(e) => write!(f, "{} ({:?})", self.code(), e.kind()),
        }
    }
}

impl std::error::Error for CallerError {}

/// Where the caller of one connection is read from. Implemented for the streams of the system; a test brings a fake.
pub trait CallerSource {
    /// Asks the system, now, for the caller of THIS connection.
    ///
    /// # Errors
    /// The system does not tell.
    fn caller(&self) -> Result<Caller, CallerError>;
}

/// The program's side: builds the capability handed to one connection, knowing who is on it.
pub trait PerConnection {
    /// The capability for this caller; it keeps the [`Caller`] and answers "who asks" from it on every call.
    fn bootstrap(&mut self, caller: &Caller) -> Client;

    /// A connection was dropped because the system did not tell who it is (nothing is served to it).
    fn refused(&mut self, _why: &CallerError) {}
}

/// One capability for everybody (the way [`serve`](crate::serve) works): the caller is not looked at.
pub(crate) struct Shared(pub(crate) Client);

impl PerConnection for Shared {
    fn bootstrap(&mut self, _: &Caller) -> Client {
        self.0.clone()
    }
}

/// The capability for one new connection: the caller is asked for each connection anew, never kept from an earlier one.
pub(crate) fn bootstrap_for<S: CallerSource, P: PerConnection>(
    stream: &S,
    per: &mut P,
) -> Option<Client> {
    match stream.caller() {
        Ok(caller) => Some(per.bootstrap(&caller)),
        Err(why) => {
            per.refused(&why);
            None
        }
    }
}

/// The name of user `uid` in the text of a passwd file (`name:x:uid:gid:...`); the first entry wins, as in the system.
#[cfg(unix)]
#[must_use]
pub(crate) fn passwd_name(passwd: &str, uid: u32) -> Option<String> {
    let want = uid.to_string();
    passwd.lines().find_map(|line| {
        let mut f = line.split(':');
        let name = f.next()?;
        (f.nth(1)? == want && !name.is_empty() && !name.starts_with('#')).then(|| name.to_owned())
    })
}

#[cfg(test)]
#[path = "peer_tests.rs"]
mod tests;
