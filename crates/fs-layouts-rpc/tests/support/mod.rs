//! A real layouts service on a real socket, on a thread of its own; the client side runs on the test's own `LocalSet`.
#![allow(dead_code, clippy::too_many_lines)]

use std::cell::Cell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread::JoinHandle;
use std::time::Duration;

use fs_forms_layout::*;
use fs_layouts_rpc::{ClientTimes, Hub, LayoutService, RpcLayouts, Watchers};
use fs_rpc::{bind_socket, serve_callers};
use tempfile::TempDir;

use lax::Lax;
use tokio::sync::oneshot;

mod build;
mod lax;
mod uuid_ids;
mod uuid_store;
#[allow(unused_imports)]
pub use build::*;
pub use uuid_store::UuidStore;

/// A UUID text of the layout kind with number `n` (the id of a layout that was never made).
pub fn uuid_text(n: u64) -> String {
    uuid_ids::fwd(&format!("layout-{n:06}"))
}

pub const PERSON: &str = "https://example.org/people/test";

pub fn who() -> Who {
    let r = Reference(PERSON.to_owned());
    Who {
        actor: r.clone(),
        author: r,
    }
}

pub fn times() -> ClientTimes {
    ClientTimes {
        call: Duration::from_secs(5),
        retry: Duration::from_millis(50),
    }
}

pub fn dir() -> TempDir {
    tempfile::Builder::new()
        .prefix("l")
        .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
        .unwrap()
}

/// The memory store with a counting clock (the same one for the service and for the local copy the tests compare with).
pub fn mem(instance: bool, providers: Option<Rc<dyn ProviderCatalogs>>) -> MemoryLayoutStore {
    let t = Cell::new(1_000_i64);
    let clock = move || {
        t.set(t.get() + 10);
        Utc(t.get())
    };
    let mut mem = MemoryLayoutStore::new(Box::new(OpenCatalog), Limits::declared(), who().actor)
        .with_clock(Box::new(clock));
    if instance {
        mem = mem.as_instance();
    }
    if let Some(p) = providers {
        mem = mem.with_providers(p);
    }
    mem
}

/// The memory store behind the id mapping, as a service for `PERSON`.
pub fn service(instance: bool) -> LayoutService<dyn LayoutStore> {
    service_with(instance, None)
}

pub fn service_with(
    instance: bool,
    providers: Option<Rc<dyn ProviderCatalogs>>,
) -> LayoutService<dyn LayoutStore> {
    service_full(instance, providers, Arc::default(), None)
}

/// Everything switchable: the providers, a store that cannot say its stamp (`stamp_fail`), another actor of the connection.
pub fn service_full(
    instance: bool,
    providers: Option<Rc<dyn ProviderCatalogs>>,
    stamp_fail: Arc<AtomicBool>,
    actor: Option<&str>,
) -> LayoutService<dyn LayoutStore> {
    let store = Rc::new(UuidStore(mem(instance, providers), stamp_fail));
    let mut svc =
        LayoutService::<dyn LayoutStore>::new(store.clone(), Reference(PERSON.to_owned()));
    if let Some(actor) = actor {
        let actor = Reference(actor.to_owned());
        svc.actors = Rc::new(move |_| Some(actor.clone()));
    }
    if instance {
        svc.admin = Some(store);
    }
    svc
}

/// The contract without the port: raw requests, for what the port has no word for.
pub async fn raw(
    socket: &Path,
) -> fs_rpc::Connection<fs_layouts_rpc::layouts_capnp::layouts::Client> {
    fs_rpc::connect(socket).await.unwrap()
}

/// A server on its own thread; stopped (and the socket removed) on drop.
pub struct Server {
    pub socket: PathBuf,
    pub watchers: Watchers,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

pub fn serve_on(
    socket: &Path,
    make: impl FnOnce() -> LayoutService<dyn LayoutStore> + Send + 'static,
) -> Server {
    let (stop, stopped) = oneshot::channel::<()>();
    let (ready, up) = std::sync::mpsc::channel();
    let path = socket.to_path_buf();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            let bound = bind_socket(&path).await.unwrap();
            let hub = Hub::new();
            ready.send(hub.watchers()).unwrap();
            serve_callers(bound, make().per_connection(&hub), async move {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
    });
    let watchers = up.recv().unwrap();
    Server {
        socket: socket.to_path_buf(),
        watchers,
        stop: Some(stop),
        thread: Some(thread),
    }
}

pub fn person(socket: &Path) -> Server {
    serve_on(socket, || service(false))
}

pub fn instance(socket: &Path) -> Server {
    serve_on(socket, || service(true))
}

/// Runs a client test body on a `LocalSet` (capnp-rpc needs one thread).
pub fn run<F: Future<Output = ()>>(f: F) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, f);
}

pub fn client(socket: &Path) -> RpcLayouts {
    RpcLayouts::new(socket, times())
}

pub fn code(e: &Coded<LayoutCode>) -> &'static str {
    e.code.as_str()
}

pub fn param<'a>(e: &'a Coded<LayoutCode>, k: &str) -> Option<&'a str> {
    e.param(k)
}

/// The refusal of a write: the store may say it as an error or as `problems` of the report; either way it is one code.
pub fn refusal(r: Result<Report, Coded<LayoutCode>>) -> Coded<LayoutCode> {
    match r {
        Err(e) => e,
        Ok(r) => r
            .problems
            .into_iter()
            .next()
            .expect("the write was refused"),
    }
}

/// Writes a UUID text (`8-4-4-4-12`) into a wire id.
pub fn put_uuid(mut out: fs_interfaces::common_capnp::uuid::Builder<'_>, text: &str) {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    out.set_hi(u64::from_str_radix(&hex[..16], 16).unwrap());
    out.set_lo(u64::from_str_radix(&hex[16..], 16).unwrap());
}

/// ONE store and ONE hub, several sockets, each socket one actor of the connection (the actor is bound to the connection the caller got, never
/// to a field it sends): `sockets` = (path, actor; `None` = the person).
pub fn serve_actors(sockets: Vec<(PathBuf, Option<String>)>) -> Server {
    serve_actors_with(sockets, None)
}

/// Like [`serve_actors`], with `watchers_max` of every service (the limit counts the watchers of the whole hub).
pub fn serve_actors_with(
    sockets: Vec<(PathBuf, Option<String>)>,
    watchers_max: Option<u32>,
) -> Server {
    use futures::FutureExt;
    let (stop, stopped) = oneshot::channel::<()>();
    let (ready, up) = std::sync::mpsc::channel();
    let first = sockets[0].0.clone();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async move {
            let store = Rc::new(UuidStore(mem(true, None), Arc::default()));
            let shut = stopped.shared();
            let mut serving = Vec::new();
            let hub = Hub::new();
            for (path, actor) in sockets {
                let mut svc = LayoutService::<dyn LayoutStore>::new(
                    store.clone(),
                    Reference(PERSON.to_owned()),
                );
                svc.admin = Some(Rc::new(Lax));
                if let Some(max) = watchers_max {
                    svc.limits.watchers_max = max;
                }
                if let Some(actor) = actor {
                    let actor = Reference(actor);
                    svc.actors = Rc::new(move |_| Some(actor.clone()));
                }
                let bound = bind_socket(&path).await.unwrap();
                let per = svc.per_connection(&hub);
                let shut = shut.clone();
                serving.push(serve_callers(bound, per, async move {
                    let _ = shut.await;
                }));
            }
            ready.send(hub.watchers()).unwrap();
            for r in futures::future::join_all(serving).await {
                r.unwrap();
            }
        });
    });
    let watchers = up.recv().unwrap();
    Server {
        socket: first,
        watchers,
        stop: Some(stop),
        thread: Some(thread),
    }
}
