//! The thread that owns the connection (capnp-rpc is `!Send`): a current-thread runtime and a `LocalSet`, a queue of commands in.
//!
//! Pattern: Command (a boxed closure that gets the live [`Link`] and answers through its own channel) + Proxy (a lost
//! connection is mended on the next call, not before: no spinning). One call at a time; each has a deadline.
//! Why not `fs_rpc::client::Worker`: it needs a connection at start and the provider-rule `Capabilities`; the store must start
//! WITHOUT a service and say `store-unreachable` (PLAN 5.4 (2)).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use fs_rpc::{Connection, Problem, connect};
use fs_settings::{Coded, SettingCode, keys};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::problem::from_problem;
use crate::settings_capnp::{admin, settings_hub, store};
use crate::wire::broken;

/// Limits, from outside (rule "einstellbar statt fest").
#[derive(Clone, Copy, Debug)]
pub struct ClientTimes {
    /// How long one call may take; after it: `store-unreachable`, defaults and the last cache.
    pub call: Duration,
    /// How long after a failed connection nothing is tried again (so a program without a service is not slowed on every call).
    pub retry: Duration,
}

impl Default for ClientTimes {
    fn default() -> Self {
        Self {
            call: Duration::from_secs(2),
            retry: Duration::from_millis(500),
        }
    }
}

/// Which capability of the hub this connection holds.
#[derive(Clone, Debug)]
pub(crate) enum Role {
    /// `SettingsHub.open(program)`.
    Program(String),
    /// `SettingsHub.manage()`.
    Manage,
    /// `SettingsHub.admin(by)` (and `manage()` for reading).
    Admin(String),
}

/// What one call can fail with, before it is a code.
pub(crate) enum CallError {
    Rpc(capnp::Error),
    Coded(Coded<SettingCode>),
}

impl From<capnp::Error> for CallError {
    fn from(e: capnp::Error) -> Self {
        Self::Rpc(e)
    }
}

impl From<std::str::Utf8Error> for CallError {
    fn from(e: std::str::Utf8Error) -> Self {
        Self::Rpc(e.into())
    }
}

impl From<Coded<SettingCode>> for CallError {
    fn from(c: Coded<SettingCode>) -> Self {
        Self::Coded(c)
    }
}

/// The live capabilities of one connection.
#[derive(Clone)]
pub(crate) struct Link {
    pub hub: settings_hub::Client,
    pub store: store::Client,
    pub admin: Option<admin::Client>,
    _connection: Rc<Connection<settings_hub::Client>>,
}

pub(crate) type Pinned<T> = Pin<Box<dyn std::future::Future<Output = T>>>;
type Job = Box<dyn FnOnce(Rc<Ctx>) -> Pinned<()> + Send>;

/// Thread-local state of the worker.
pub(crate) struct Ctx {
    socket: PathBuf,
    role: Role,
    times: ClientTimes,
    link: RefCell<Option<Link>>,
    failed_at: Cell<Option<Instant>>,
    pub(crate) held: RefCell<HashMap<u64, Box<dyn Any>>>,
}

impl Ctx {
    fn unreachable(&self) -> Coded<SettingCode> {
        unreachable_at(&self.socket)
    }

    async fn connect(&self) -> Result<Link, Coded<SettingCode>> {
        let attempt = async {
            let connection = connect::<settings_hub::Client>(&self.socket).await.map_err(|_| self.unreachable())?;
            let hub = connection.client.clone();
            let (store, admin) = match &self.role {
                Role::Program(p) => {
                    let mut r = hub.open_request();
                    r.get().set_program(p.as_str());
                    (r.send().promise.await.and_then(|x| x.get()?.get_store()), None)
                }
                Role::Manage => (hub.manage_request().send().promise.await.and_then(|x| x.get()?.get_store()), None),
                Role::Admin(by) => {
                    let mut r = hub.admin_request();
                    r.get().set_by(by.as_str());
                    let a = r.send().promise.await.and_then(|x| x.get()?.get_admin());
                    (hub.manage_request().send().promise.await.and_then(|x| x.get()?.get_store()), Some(a))
                }
            };
            let admin = match admin {
                Some(Ok(a)) => Some(a),
                Some(Err(e)) => return Err(self.rpc_error(&e)),
                None => None,
            };
            let store = store.map_err(|e| self.rpc_error(&e))?;
            Ok(Link { hub, store, admin, _connection: Rc::new(connection) })
        };
        tokio::time::timeout(self.times.call, attempt).await.unwrap_or_else(|_| Err(self.unreachable()))
    }

    fn rpc_error(&self, e: &capnp::Error) -> Coded<SettingCode> {
        match Problem::from_error(e) {
            Some(p) => from_problem(&p).unwrap_or_else(|| broken("problem-type")),
            None => self.unreachable(),
        }
    }

    async fn link(&self) -> Result<Link, Coded<SettingCode>> {
        if let Some(l) = self.link.borrow().clone() {
            return Ok(l);
        }
        if self.failed_at.get().is_some_and(|t| t.elapsed() < self.times.retry) {
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
                Some(p) => Err(from_problem(&p).unwrap_or_else(|| broken("problem-type"))),
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

/// The thread and its queue.
pub(crate) struct Worker {
    socket: PathBuf,
    jobs: Option<UnboundedSender<Job>>,
    thread: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the thread; connects on the first call, never here.
    pub(crate) fn start(socket: &Path, role: Role, times: ClientTimes) -> Self {
        let (jobs, mut queue) = unbounded_channel::<Job>();
        let path = socket.to_path_buf();
        let thread = std::thread::Builder::new()
            .name("fs-settings-client".to_owned())
            .spawn({
                let path = path.clone();
                move || {
                    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                        return;
                    };
                    let ctx = Rc::new(Ctx {
                        socket: path,
                        role,
                        times,
                        link: RefCell::new(None),
                        failed_at: Cell::new(None),
                        held: RefCell::new(HashMap::new()),
                    });
                    let local = tokio::task::LocalSet::new();
                    local.block_on(&runtime, async {
                        while let Some(job) = queue.recv().await {
                            tokio::task::spawn_local(job(ctx.clone()));
                        }
                        ctx.link.borrow_mut().take();
                    });
                    runtime.block_on(local);
                }
            })
            .ok();
        Self {
            socket: path,
            jobs: Some(jobs),
            thread,
        }
    }

    /// Runs `f` on the worker thread and waits.
    ///
    /// # Panics
    /// In a debug build when called from a runtime thread: the wait would block that runtime ("never from a runtime thread").
    pub(crate) fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(Link) -> Pinned<Result<T, CallError>> + Send + 'static,
    ) -> Result<T, Coded<SettingCode>> {
        self.exec(move |ctx| Box::pin(async move { ctx.run(f).await }))
    }

    /// Runs `f` with the thread-local state and waits.
    pub(crate) fn exec<T: Send + 'static>(
        &self,
        f: impl FnOnce(Rc<Ctx>) -> Pinned<Result<T, Coded<SettingCode>>> + Send + 'static,
    ) -> Result<T, Coded<SettingCode>> {
        debug_assert!(
            tokio::runtime::Handle::try_current().is_err(),
            "never from a runtime thread: the port is sync and blocks"
        );
        let gone = || unreachable_at(&self.socket);
        let (reply, answer) = mpsc::channel();
        let job: Job = Box::new(move |ctx| {
            Box::pin(async move {
                let r = f(ctx).await;
                let _ = reply.send(r);
            })
        });
        self.jobs.as_ref().ok_or_else(gone)?.send(job).map_err(|_| gone())?;
        answer.recv().map_err(|_| gone())?
    }
}

/// A way to tell the worker something later, without owning it (the end of a subscription).
pub(crate) struct Handle(UnboundedSender<Job>);

impl Handle {
    /// Lets go of what the worker holds under `id`.
    pub(crate) fn forget(&self, id: u64) {
        let _ = self.0.send(Box::new(move |ctx| {
            Box::pin(async move {
                ctx.held.borrow_mut().remove(&id);
            })
        }));
    }
}

impl Worker {
    /// A handle that outlives no work: sends only.
    pub(crate) fn handle(&self) -> Option<Handle> {
        self.jobs.clone().map(Handle)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(t) = self.thread.take() {
            if t.thread().id() != std::thread::current().id() {
                let _ = t.join();
            }
        }
    }
}
