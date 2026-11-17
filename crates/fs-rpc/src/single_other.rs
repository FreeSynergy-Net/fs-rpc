//! `ensure_single` where there is no `/proc` and no heartbeat socket (Windows): the handshake only; a server that holds the socket and
//! does not answer is reported (`server-unresponsive`, `why` = `unsupported`), nothing is ended.
//!
//! Pattern: Null object — the same names as the Unix `single` module, so a program compiles everywhere. The types of the error are the
//! Unix ones (`error.rs`, shared by path); only the limits are fewer.

#[path = "single/error.rs"]
mod error;

use std::path::Path;
use std::time::Duration;

use capnp::capability::FromClientHook;

pub use error::{SingleError, Stuck, TakenOver};

use crate::{Connection, Expected, RestartTimes, ServerStarter, connect_matching_with};

/// The limits of `ensure_single` (here: the handshake and the whole call).
#[derive(Clone, Copy, Debug)]
pub struct SingleTimes {
    /// The handshake.
    pub handshake: RestartTimes,
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
            overall: Duration::from_secs(90),
        }
    }
}

/// The connection (nothing is ended here, so `taken_over` is always `None`).
pub struct Single<C> {
    /// The connection to the server that fits.
    pub connection: Connection<C>,
    /// Always `None` on this system.
    pub taken_over: Option<TakenOver>,
}

/// The connection to the server of this program that fits `expected`; starts one through `starter` if none runs. A silent server is
/// reported as `unsupported`. `exe` is not used here.
///
/// # Errors
/// [`SingleError`].
pub async fn ensure_single<C: FromClientHook>(
    socket: &Path,
    expected: &Expected,
    starter: &dyn ServerStarter,
    times: &SingleTimes,
    _exe: Option<&Path>,
) -> Result<Single<C>, SingleError> {
    let work = connect_matching_with::<C>(socket, expected, starter, times.handshake);
    match tokio::time::timeout(times.overall, work).await {
        Ok(Ok(connection)) => Ok(Single {
            connection,
            taken_over: None,
        }),
        Ok(Err(e)) if error::is_silent(&e) => Err(SingleError::Unresponsive(Stuck {
            pid: None,
            why: "unsupported",
            exe: None,
            limit: times.overall,
        })),
        Ok(Err(e)) => Err(SingleError::Handshake(e)),
        Err(_) => Err(SingleError::TooLong(times.overall)),
    }
}
