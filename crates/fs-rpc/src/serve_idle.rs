//! The accept loop, with an optional end after a time without a user (ADR 0097 N2 no. 4: a module service ends when nobody uses it).
//!
//! Pattern: Template method (one loop for `serve*` and `run_until_signal*`) + Options (a value that adds the end, the default changes nothing).
//! The loop counts the connections it serves. An accepted caller that is dropped (another user, a refused caller) is no connection and
//! does not keep the service alive.

use std::cell::Cell;
use std::future::Future;
use std::io;
use std::rc::Rc;
use std::time::Duration;

use capnp::capability::Client;
use tokio::sync::Notify;
use tokio::time::Instant;

use super::serve::{ACCEPT_PAUSE, ReadLimits, system};
use super::transport::{
    Accepted, BoundSocket, Endpoint, PerConnection, Shared, System, bootstrap_for,
};
use capnp_rpc::rpc_twoparty_capnp::Side;

/// What `serve_options` and `serve_callers_options` add to [`serve`](super::serve): the read limits and the optional end.
///
/// The default is the behaviour of `serve`: default limits, no end but `shutdown`.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ServeOptions {
    /// What this end reads at most in one message, see [`ReadLimits`].
    pub limits: ReadLimits,
    /// End the serving (`Ok(())`, the socket removed) when this long no connection was there: counted from the start and from the end
    /// of the last connection. `None` (the default) = never.
    pub idle_timeout: Option<Duration>,
}

impl ServeOptions {
    /// The options with an end after `idle` without a connection.
    #[must_use]
    pub fn with_idle_timeout(mut self, idle: Duration) -> Self {
        self.idle_timeout = Some(idle);
        self
    }

    /// The options with other read limits.
    #[must_use]
    pub fn with_limits(mut self, limits: ReadLimits) -> Self {
        self.limits = limits;
        self
    }
}

/// [`serve_with`](super::serve_with) with [`ServeOptions`]: the same loop, ends also after `options.idle_timeout` without a connection.
///
/// # Errors
/// As for [`serve`](super::serve).
pub async fn serve_options(
    socket: BoundSocket,
    bootstrap: Client,
    shutdown: impl Future<Output = ()>,
    options: ServeOptions,
) -> io::Result<()> {
    let mut per = Shared(bootstrap);
    accept_idle(&socket, |s| bootstrap_for(s, &mut per), shutdown, options).await
}

/// [`serve_callers`](super::serve_callers) with [`ServeOptions`].
///
/// # Errors
/// As for [`serve`](super::serve).
pub async fn serve_callers_options<P: PerConnection>(
    socket: BoundSocket,
    mut per: P,
    shutdown: impl Future<Output = ()>,
    options: ServeOptions,
) -> io::Result<()> {
    accept_idle(&socket, |s| bootstrap_for(s, &mut per), shutdown, options).await
}

/// Counts the connection that is served; on drop it is over, and the loop is told.
struct Live {
    count: Rc<Cell<usize>>,
    ended: Rc<Notify>,
}

impl Live {
    fn begin(count: &Rc<Cell<usize>>, ended: &Rc<Notify>) -> Self {
        count.set(count.get() + 1);
        Self {
            count: count.clone(),
            ended: ended.clone(),
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.count.set(self.count.get() - 1);
        self.ended.notify_one();
    }
}

pub(super) async fn accept_idle(
    socket: &BoundSocket,
    mut bootstrap: impl FnMut(&<System as Endpoint>::ServerStream) -> Option<Client>,
    shutdown: impl Future<Output = ()>,
    options: ServeOptions,
) -> io::Result<()> {
    tokio::pin!(shutdown);
    let live = Rc::new(Cell::new(0_usize));
    let ended = Rc::new(Notify::new());
    // Since when nobody is connected (`None` while a connection is served).
    let mut quiet_since = Some(Instant::now());
    // One `accept` is kept until it answers: another arm waking the loop must not drop a half done accept.
    let mut accept = Box::pin(System::accept(socket));
    loop {
        let idle_over = async {
            match (options.idle_timeout, quiet_since) {
                (Some(idle), Some(since)) => tokio::time::sleep_until(since + idle).await,
                _ => std::future::pending().await,
            }
        };
        tokio::select! {
            () = &mut shutdown => return Ok(()),
            () = idle_over => return Ok(()),
            () = ended.notified() => {
                if live.get() == 0 {
                    quiet_since = Some(Instant::now());
                }
            }
            accepted = &mut accept => {
                accept = Box::pin(System::accept(socket));
                match accepted? {
                    Accepted::Peer(stream) => {
                        if let Some(client) = bootstrap(&stream) {
                            quiet_since = None;
                            let guard = Live::begin(&live, &ended);
                            let rpc = system(stream, Side::Server, Some(client), options.limits);
                            tokio::task::spawn_local(async move {
                                drop(rpc.await);
                                drop(guard);
                            });
                        }
                    }
                    Accepted::Skip => {}
                    Accepted::Pause => {
                        tokio::select! {
                            () = &mut shutdown => return Ok(()),
                            () = tokio::time::sleep(ACCEPT_PAUSE) => {}
                        }
                    }
                }
            }
        }
    }
}
