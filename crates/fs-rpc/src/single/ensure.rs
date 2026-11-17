//! `ensure_single` and its limits: the template of the call (see the module text of `single`).
//!
//! Pattern: Template method.

use std::cell::RefCell;
use std::path::Path;
use std::time::Duration;

use capnp::capability::FromClientHook;

#[cfg(not(target_os = "linux"))]
use super::error::Stuck;
use super::error::{SingleError, TakenOver};

use crate::heartbeat::HangTimes;
use crate::{Connection, Expected, RestartTimes, ServerStarter, connect_matching_with};

/// The limits of `ensure_single`; the defaults are the plan, tests make them shorter.
#[derive(Clone, Copy, Debug)]
pub struct SingleTimes {
    /// The handshake (`call` 2 s, a new server gets `start` 5 s).
    pub handshake: RestartTimes,
    /// The heartbeat limits (ping every 2 s, hanging after 10 s without answer or 30 s without progress).
    pub hang: HangTimes,
    /// How long the program gets to save before the end.
    pub save: Duration,
    /// After SIGTERM: how long until SIGKILL may be considered.
    pub term: Duration,
    /// After SIGKILL: how long until it is called not ending.
    pub kill: Duration,
    /// The whole call, at most.
    pub overall: Duration,
}

impl Default for SingleTimes {
    fn default() -> Self {
        Self {
            handshake: RestartTimes {
                call: Duration::from_secs(2),
                stop: Duration::from_secs(5),
                start: Duration::from_secs(5),
            },
            hang: HangTimes::default(),
            save: Duration::from_secs(10),
            term: Duration::from_secs(10),
            kill: Duration::from_secs(2),
            overall: Duration::from_secs(90),
        }
    }
}

/// The connection, and what had to be done to get it.
pub struct Single<C> {
    /// The connection to the server that fits.
    pub connection: Connection<C>,
    /// A hanging server of this program was ended for it: who, what was saved, how.
    pub taken_over: Option<TakenOver>,
}

#[cfg(not(target_os = "linux"))]
mod other {
    pub(super) enum Freed {
        Ready,
        Free(Option<super::TakenOver>),
    }
    pub(super) type BusySeen = std::cell::RefCell<Option<(String, u64, Option<u32>)>>;
    pub(super) async fn free_socket(
        _: &std::path::Path,
        times: &super::SingleTimes,
        _: Option<&std::path::Path>,
        _: &BusySeen,
    ) -> Result<Freed, super::SingleError> {
        Err(super::SingleError::Unresponsive(super::Stuck {
            pid: None,
            why: "unsupported",
            exe: None,
            limit: times.overall,
        }))
    }
}
#[cfg(target_os = "linux")]
use super::watching::{BusySeen, Freed, free_socket};
#[cfg(not(target_os = "linux"))]
use other::{BusySeen, Freed, free_socket};

/// The connection to the one server of this program that fits `expected`; starts one through `starter` if none runs; ends one that
/// hangs (see the module text) and replaces it. `exe` is the program file that counts as "this program" (`None`: the file of the
/// running process). Must run on a `LocalSet`.
///
/// # Errors
/// [`SingleError`]; `RpcError::from(&error)` gives the code (`server-unresponsive`, `server-busy`, or the handshake's).
pub async fn ensure_single<C: FromClientHook>(
    socket: &Path,
    expected: &Expected,
    starter: &dyn ServerStarter,
    times: &SingleTimes,
    exe: Option<&Path>,
) -> Result<Single<C>, SingleError> {
    let busy: BusySeen = RefCell::new(None);
    let run = run::<C>(socket, expected, starter, times, exe, &busy);
    match tokio::time::timeout(times.overall, run).await {
        Ok(done) => done,
        Err(_) => Err(match busy.take() {
            Some((what, progress, pid)) => SingleError::Busy {
                pid,
                what,
                progress,
            },
            None => SingleError::TooLong(times.overall),
        }),
    }
}

async fn run<C: FromClientHook>(
    socket: &Path,
    expected: &Expected,
    starter: &dyn ServerStarter,
    times: &SingleTimes,
    exe: Option<&Path>,
    busy: &BusySeen,
) -> Result<Single<C>, SingleError> {
    let mut taken_over = None;
    let mut looked = false;
    loop {
        match connect_matching_with::<C>(socket, expected, starter, times.handshake).await {
            Ok(connection) => {
                return Ok(Single {
                    connection,
                    taken_over,
                });
            }
            Err(e) if super::error::is_silent(&e) && !looked => {
                looked = true;
                if let Freed::Free(t) = free_socket(socket, times, exe, busy).await? {
                    taken_over = t;
                }
            }
            Err(e) => return Err(SingleError::Handshake(e)),
        }
    }
}
