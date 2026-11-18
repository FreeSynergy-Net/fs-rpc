//! The connection of the client: the live link, the retry pause, the call timeout. A lost link is mended on the next call, not
//! before; while the pause runs, calls answer `unreachable{socket}` at once.
//!
//! Pattern: Proxy (virtual connection) with a circuit pause. Times are settings of the caller (`ClientTimes`), not constants.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use fs_forms_layout::{Coded, LayoutCode};
use fs_rpc::{Connection, connect};

use crate::ensure::unreachable_at;
use crate::fail::H;
use crate::layouts_capnp::layouts;

/// How long a call may take and how long to wait after a lost link before trying again. Settings of the program that hosts the
/// client; `declared()` is the default (not measured).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientTimes {
    /// Longest wait for one call (a connect included).
    pub call: Duration,
    /// Pause after a failure before the next connect.
    pub retry: Duration,
}

impl ClientTimes {
    /// The declared defaults: 10 s per call, 1 s pause.
    #[must_use]
    pub fn declared() -> Self {
        Self {
            call: Duration::from_secs(10),
            retry: Duration::from_secs(1),
        }
    }
}

#[derive(Clone)]
struct Link {
    client: layouts::Client,
    _connection: Rc<Connection<layouts::Client>>,
}

pub(crate) struct Ctx {
    pub(crate) socket: PathBuf,
    times: ClientTimes,
    link: RefCell<Option<Link>>,
    failed_at: Cell<Option<Instant>>,
}

impl Ctx {
    pub(crate) fn new(socket: PathBuf, times: ClientTimes) -> Self {
        Self {
            socket,
            times,
            link: RefCell::new(None),
            failed_at: Cell::new(None),
        }
    }

    fn unreachable(&self) -> Coded<LayoutCode> {
        unreachable_at(&self.socket)
    }

    fn lost(&self) -> Coded<LayoutCode> {
        self.link.borrow_mut().take();
        self.failed_at.set(Some(Instant::now()));
        self.unreachable()
    }

    pub(crate) async fn link(&self) -> Result<layouts::Client, Coded<LayoutCode>> {
        if let Some(l) = self.link.borrow().clone() {
            return Ok(l.client);
        }
        if self
            .failed_at
            .get()
            .is_some_and(|t| t.elapsed() < self.times.retry)
        {
            return Err(self.unreachable());
        }
        let attempt =
            tokio::time::timeout(self.times.call, connect::<layouts::Client>(&self.socket)).await;
        match attempt {
            Ok(Ok(c)) => {
                self.failed_at.set(None);
                let l = Link {
                    client: c.client.clone(),
                    _connection: Rc::new(c),
                };
                *self.link.borrow_mut() = Some(l.clone());
                Ok(l.client)
            }
            _ => Err(self.lost()),
        }
    }

    /// Runs one call on the live link; a lost link is dropped, a problem of the service is its code.
    pub(crate) async fn call<T>(
        &self,
        f: impl AsyncFnOnce(layouts::Client) -> H<T>,
    ) -> Result<T, Coded<LayoutCode>> {
        let client = self.link().await?;
        match tokio::time::timeout(self.times.call, f(client)).await {
            Ok(Ok(t)) => Ok(t),
            Ok(Err(fail)) => Err(match fail.into_coded() {
                Ok(code) => code,
                // only a lost connection is `unreachable`; any other error without a problem (a message too large, a method the
                // server does not know) is an answer of the service and keeps the link
                Err(e) if fs_rpc::client::connection_lost(&e) => self.lost(),
                Err(_) => crate::wire::text::bad("rpc-error"),
            }),
            Err(_) => Err(self.lost()),
        }
    }
}
