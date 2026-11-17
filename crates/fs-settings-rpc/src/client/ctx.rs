//! The thread-local state of the connection thread: the live link, the retry pause, what is held for subscriptions.
//!
//! Pattern: Proxy (a lost connection is mended on the next call, not before).

use crate::Coded;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

use fs_rpc::{Problem, connect};
use fs_settings::{SettingCode, keys};

use super::worker::{CallError, ClientTimes, Link, Pinned, Role};
use crate::problem::meaning;
use crate::settings_capnp::settings_hub;

/// Thread-local state of the worker.
pub(crate) struct Ctx {
    pub(crate) socket: PathBuf,
    pub(crate) role: Role,
    pub(crate) times: ClientTimes,
    pub(crate) link: RefCell<Option<Link>>,
    pub(crate) failed_at: Cell<Option<Instant>>,
    pub(crate) held: RefCell<HashMap<u64, Box<dyn Any>>>,
}

impl Ctx {
    pub(crate) fn new(socket: PathBuf, role: Role, times: ClientTimes) -> Self {
        Self {
            socket,
            role,
            times,
            link: RefCell::new(None),
            failed_at: Cell::new(None),
            held: RefCell::new(HashMap::new()),
        }
    }

    fn unreachable(&self) -> Coded<SettingCode> {
        unreachable_at(&self.socket)
    }

    async fn connect(&self) -> Result<Link, Coded<SettingCode>> {
        let attempt = async {
            let connection = connect::<settings_hub::Client>(&self.socket)
                .await
                .map_err(|_| self.unreachable())?;
            let hub = connection.client.clone();
            let (store, admin) = match &self.role {
                Role::Program(p) => {
                    let mut r = hub.open_request();
                    r.get().set_program(p.as_str());
                    (
                        r.send().promise.await.and_then(|x| x.get()?.get_store()),
                        None,
                    )
                }
                Role::Manage => (
                    hub.manage_request()
                        .send()
                        .promise
                        .await
                        .and_then(|x| x.get()?.get_store()),
                    None,
                ),
                Role::Admin(by) => {
                    let mut r = hub.admin_request();
                    r.get().set_by(by.as_str());
                    let a = r.send().promise.await.and_then(|x| x.get()?.get_admin());
                    (
                        hub.manage_request()
                            .send()
                            .promise
                            .await
                            .and_then(|x| x.get()?.get_store()),
                        Some(a),
                    )
                }
            };
            let admin = match admin {
                Some(Ok(a)) => Some(a),
                Some(Err(e)) => return Err(self.rpc_error(&e)),
                None => None,
            };
            let store = store.map_err(|e| self.rpc_error(&e))?;
            Ok(Link {
                hub,
                store,
                admin,
                _connection: Rc::new(connection),
            })
        };
        tokio::time::timeout(self.times.call, attempt)
            .await
            .unwrap_or_else(|_| Err(self.unreachable()))
    }

    fn rpc_error(&self, e: &capnp::Error) -> Coded<SettingCode> {
        match Problem::from_error(e) {
            Some(p) => meaning(&p),
            None => self.unreachable(),
        }
    }

    async fn link(&self) -> Result<Link, Coded<SettingCode>> {
        if let Some(l) = self.link.borrow().clone() {
            return Ok(l);
        }
        if self
            .failed_at
            .get()
            .is_some_and(|t| t.elapsed() < self.times.retry)
        {
            return Err(self.unreachable());
        }
        match self.connect().await {
            Ok(l) => {
                self.failed_at.set(None);
                *self.link.borrow_mut() = Some(l.clone());
                Ok(l)
            }
            Err(c) => {
                self.failed_at.set(Some(Instant::now()));
                Err(c)
            }
        }
    }

    /// Runs one call on the live link; a lost link is dropped (next call mends it), a problem of the service is its code.
    pub(crate) async fn run<T>(
        &self,
        f: impl FnOnce(Link) -> Pinned<Result<T, CallError>>,
    ) -> Result<T, Coded<SettingCode>> {
        let link = self.link().await?;
        let lost = |this: &Self| {
            this.link.borrow_mut().take();
            this.failed_at.set(Some(Instant::now()));
            this.unreachable()
        };
        match tokio::time::timeout(self.times.call, f(link)).await {
            Ok(Ok(t)) => Ok(t),
            Ok(Err(CallError::Coded(c))) => Err(c),
            Ok(Err(CallError::Rpc(e))) => match Problem::from_error(&e) {
                Some(p) => Err(meaning(&p)),
                None => Err(lost(self)),
            },
            Err(_) => Err(lost(self)),
        }
    }
}

/// `store-unreachable{socket}`.
#[must_use]
pub fn unreachable_at(socket: &Path) -> Coded<SettingCode> {
    Coded::new(SettingCode::StoreUnreachable).with_text(keys::SOCKET, socket.display().to_string())
}
