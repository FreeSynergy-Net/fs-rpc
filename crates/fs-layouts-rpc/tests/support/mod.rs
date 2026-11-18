//! A real layouts service on a real socket, on a thread of its own; the client side runs on the test's own `LocalSet`.
#![allow(dead_code, clippy::too_many_lines)]

use std::cell::Cell;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::thread::JoinHandle;
use std::time::Duration;

use fs_forms_layout::*;
use fs_layouts_rpc::{ClientTimes, LayoutService, RpcLayouts, Watchers};
use fs_rpc::{bind_socket, serve};
use tempfile::TempDir;
use tokio::sync::oneshot;

mod build;
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

/// The memory store with a counting clock, behind the id mapping, as a service for `PERSON`.
pub fn service(instance: bool) -> LayoutService<dyn LayoutStore> {
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
    let store = Rc::new(UuidStore(mem));
    let mut svc =
        LayoutService::<dyn LayoutStore>::new(store.clone(), Reference(PERSON.to_owned()));
    if instance {
        svc.admin = Some(store);
    }
    svc
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
            let (boot, watchers) = make().bootstrap();
            ready.send(watchers).unwrap();
            serve(bound, boot, async move {
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
