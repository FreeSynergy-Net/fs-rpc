//! The loop: connect, subscribe, watch, and again when the link is lost.
//!
//! Pattern: Template method (the loop) + Strategy (`Session`: how the program subscribes; `Resync`: what it does when it must
//! load anew) + Observer (`Resync`). Why a loop in the library: every client of every program had one `connect` and one
//! `subscribe`, so a server that was replaced (update, crash, take-over) left a feed that was dead for good. The hint
//! `resync_needed` says that changes may have been missed; the library cannot know which, the program compares the
//! `generation()` of its contract. Must run on a `LocalSet`, like `connect`.

use std::any::Any;
use std::future::Future;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use capnp::capability::FromClientHook;
use fs_interfaces::common_capnp::control;
use futures::future::LocalBoxFuture;

use super::backoff::{Backoff, ReconnectTimes};
use crate::client::Fault;
use crate::{Connection, Expected, RpcError, ServerStarter, connect_matching_with};

/// What [`Resync::reconnected`] is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reconnected {
    /// Which try of this connection succeeded (1 = the first, no pause).
    pub attempt: u32,
    /// Changes may have been missed: load everything anew (or compare the contract's `generation()` first). `false` only for the
    /// very first connection of the loop.
    pub resync_needed: bool,
}

/// The program's reaction to the state of the link.
pub trait Resync {
    /// The link stands (connected and subscribed). Called once for each new connection.
    fn reconnected(&self, event: Reconnected);

    /// The link could not be restored within `give_up`; `error` is what the last try said. The loop ends after this.
    fn gave_up(&self, error: &RpcError);
}

/// How the program subscribes on a connection.
pub trait Session<C> {
    /// Subscribe on the new connection (`client` is its root capability). The answer is what keeps the subscription alive (the
    /// capability and its response); the loop holds it until the link is lost, and dropping it ends the subscription.
    fn attach(&self, client: C) -> LocalBoxFuture<'_, io::Result<Box<dyn Any>>>;

    /// Resolves when the subscription ended although the connection may still stand (`ChangeListener.ended`). The default
    /// never does: only a connection that no longer answers makes the loop connect again.
    fn ended(&self) -> LocalBoxFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
}

/// Why [`keep_connected`] returned.
#[derive(Debug, PartialEq, Eq)]
pub enum ReconnectEnd {
    /// The caller's `stop` came.
    Stopped,
    /// No success within `give_up`; the last error (the program was told through [`Resync::gave_up`]).
    GaveUp(RpcError),
}

/// Keep a connection to the server at `socket`, matching `expected`, with the subscription of `session`: connect (and start
/// the server through `starter` if it is not there), subscribe, tell `resync`, then watch; when the server stops answering,
/// the subscription ended or the connection is gone, connect again after the pauses of `times` and tell `resync` that a reload
/// is needed. Every step has a limit; after `times.give_up` without success `resync.gave_up` is told and the loop ends.
/// Returns when `stop` resolves.
pub async fn keep_connected<C>(
    socket: &Path,
    expected: &Expected,
    starter: &dyn ServerStarter,
    times: &ReconnectTimes,
    session: &dyn Session<C>,
    resync: &dyn Resync,
    stop: impl Future<Output = ()>,
) -> ReconnectEnd
where
    C: FromClientHook + Clone,
{
    tokio::pin!(stop);
    let mut first = true;
    loop {
        let began = Instant::now();
        let mut backoff = Backoff::new(&times.backoff);
        let mut attempt = 0u32;
        let (connection, held) = loop {
            attempt += 1;
            let tried = tokio::select! {
                () = &mut stop => return ReconnectEnd::Stopped,
                tried = try_once(socket, expected, starter, times, session) => tried,
            };
            let error = match tried {
                Ok(link) => break link,
                Err(error) => error,
            };
            let left = times.give_up.saturating_sub(began.elapsed());
            if left.is_zero() {
                resync.gave_up(&error);
                return ReconnectEnd::GaveUp(error);
            }
            tokio::select! {
                () = &mut stop => return ReconnectEnd::Stopped,
                () = tokio::time::sleep(backoff.next_pause().min(left)) => {}
            }
        };
        resync.reconnected(Reconnected {
            attempt,
            resync_needed: !first,
        });
        first = false;
        tokio::select! {
            () = &mut stop => return ReconnectEnd::Stopped,
            () = watch(&connection.client, session, times) => {}
        }
        drop((connection, held));
    }
}

/// One try, within `times.attempt`: connect with the handshake, then subscribe.
async fn try_once<C>(
    socket: &Path,
    expected: &Expected,
    starter: &dyn ServerStarter,
    times: &ReconnectTimes,
    session: &dyn Session<C>,
) -> Result<(Connection<C>, Box<dyn Any>), RpcError>
where
    C: FromClientHook + Clone,
{
    let work = async {
        let connection = connect_matching_with::<C>(socket, expected, starter, times.handshake)
            .await
            .map_err(|e| RpcError::from(&e))?;
        let held = session
            .attach(connection.client.clone())
            .await
            .map_err(|e| RpcError::from(&e))?;
        Ok((connection, held))
    };
    tokio::time::timeout(times.attempt, work)
        .await
        .unwrap_or_else(|_| {
            Err(RpcError::from(&Fault::TimedOut {
                server: "reconnect",
                limit: times.attempt,
            }))
        })
}

/// Returns when the link is lost: the subscription ended, or the server did not answer a ping in `ping_limit`.
async fn watch<C: FromClientHook>(client: &C, session: &dyn Session<C>, times: &ReconnectTimes) {
    let ended = session.ended();
    tokio::pin!(ended);
    loop {
        tokio::select! {
            () = &mut ended => return,
            () = tokio::time::sleep(times.ping) => {
                if !answers(client, times.ping_limit).await {
                    return;
                }
            }
        }
    }
}

/// Whether the server answers `Control.version` in time. A server without `Control` answers "not implemented": it is there.
async fn answers<C: FromClientHook>(client: &C, limit: Duration) -> bool {
    let control = control::Client::new(client.as_client_hook().add_ref());
    match tokio::time::timeout(limit, control.version_request().send().promise).await {
        Err(_) => false,
        Ok(Ok(_)) => true,
        Ok(Err(e)) => {
            e.kind == capnp::ErrorKind::Unimplemented || e.extra.contains("not implemented")
        }
    }
}
