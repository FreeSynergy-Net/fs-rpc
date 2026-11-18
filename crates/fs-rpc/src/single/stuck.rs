//! A server that holds the socket and does not answer: the one shared description of it (fs-rpc and the programs).
//!
//! Pattern: Value object, typed (a code and parameters, ADR 0057). `server-unresponsive` is made by `From<&Stuck> for RpcError`
//! and read back by [`Stuck::from_error`]; the program words it (no English sentence here). The fields are the union of what
//! `ensure_single` and the window of fs-tasks knew: `can_ask` and `start` for the "end it?" question, `since` and `exe` for the
//! words, `why` for the reason it was not ended, `limit` for the wait that ran out.
//!
//! The process number is `u32`: a process number is never negative, and a signed one invites a signal to a process group
//! (`kill(-pid)`). A number a system gives as `i32` that is not positive is not a process number (`u32::try_from` refuses it).

use std::time::Duration;

use crate::code::param_keys as k;
use crate::{Param, RpcError, RpcErrorCode};

/// The reasons a silent server was not ended (the values of `why`): the rule says `itself`, `foreign-user`, `unreadable`,
/// `pid-reused`, `other-program`; the ending says `beating` (it still answers its heartbeat), `not-ending` (it did not end),
/// `no-control` (no heartbeat channel: nothing could be saved first); `unsupported` is where nothing can be looked at.
pub const WHY_TOKENS: [&str; 9] = [
    "itself",
    "foreign-user",
    "unreadable",
    "pid-reused",
    "other-program",
    "beating",
    "not-ending",
    "no-control",
    "unsupported",
];

/// A server that holds the socket, does not answer and was not ended (`server-unresponsive`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Stuck {
    /// Its process, if known.
    pub pid: Option<u32>,
    /// The start time of its process in clock ticks since the boot (what a confirmation to end it is checked against).
    pub start: Option<u64>,
    /// When it started, as UTC text (RFC 3339), if known.
    pub since: Option<String>,
    /// The real path of its program file, if it could be read.
    pub exe: Option<String>,
    /// Whether the person may say yes to ending it (their own process: the same user, nothing unclear).
    pub can_ask: bool,
    /// Why it was not ended: one of [`WHY_TOKENS`]. Another program is only reported.
    pub why: &'static str,
    /// How long the wait was.
    pub limit: Duration,
}

impl Stuck {
    /// Nothing known but the reason and the wait; set the rest with the fields.
    #[must_use]
    pub fn new(why: &'static str, limit: Duration) -> Self {
        Self {
            pid: None,
            start: None,
            since: None,
            exe: None,
            can_ask: false,
            why,
            limit,
        }
    }

    /// Read the description back from `server-unresponsive`. `None` for another code, a reason that is none of [`WHY_TOKENS`],
    /// a parameter of the wrong kind, or a process number or a wait that does not fit.
    #[must_use]
    pub fn from_error(e: &RpcError) -> Option<Self> {
        if e.code() != RpcErrorCode::ServerUnresponsive {
            return None;
        }
        let number = |key: &str| match e.param(key) {
            None => Some(None),
            Some(Param::Number(n)) => Some(Some(*n)),
            Some(Param::Text(_)) => None,
        };
        let text = |key: &str| match e.param(key) {
            None => Some(None),
            Some(Param::Text(t)) => Some(Some(t.clone())),
            Some(Param::Number(_)) => None,
        };
        let pid = match number(k::PID)? {
            Some(n) => Some(u32::try_from(n).ok()?),
            None => None,
        };
        let given = e.param(k::WHY)?.as_text()?;
        let why = *WHY_TOKENS.iter().find(|w| **w == given)?;
        Some(Self {
            pid,
            start: number(k::START)?,
            since: text(k::SINCE)?,
            exe: text(k::EXE)?,
            can_ask: text(k::CAN_ASK)?.as_deref() == Some("yes"),
            why,
            limit: Duration::from_millis(number(k::LIMIT_MS)??),
        })
    }
}

impl From<&Stuck> for RpcError {
    /// `server-unresponsive` with `pid`, `start`, `since`, `exe` (each left out if unknown), `can-ask` (`yes`, left out if not),
    /// `why` and `limit-ms`.
    fn from(s: &Stuck) -> Self {
        let mut e = Self::new(RpcErrorCode::ServerUnresponsive);
        if let Some(pid) = s.pid {
            e = e.with_number(k::PID, u64::from(pid));
        }
        if let Some(start) = s.start {
            e = e.with_number(k::START, start);
        }
        if let Some(since) = &s.since {
            e = e.with_text(k::SINCE, since.as_str());
        }
        if let Some(exe) = &s.exe {
            e = e.with_text(k::EXE, exe.as_str());
        }
        if s.can_ask {
            e = e.with_text(k::CAN_ASK, "yes");
        }
        let millis = u64::try_from(s.limit.as_millis()).unwrap_or(u64::MAX);
        e.with_text(k::WHY, s.why).with_number(k::LIMIT_MS, millis)
    }
}
