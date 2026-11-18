//! A server that builds a capability per connection from its `Caller`, and a client that claims to be somebody else.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use capnp::capability::Client;
use capnp_rpc::new_client;
use fs_interfaces::tasks_capnp::{items, tasks};
use fs_rpc::{Caller, PerConnection, bind_socket, connect, serve_callers};
use tempfile::TempDir;

/// What a call saw: who the connection says, and what the client claimed in its own field.
pub type Seen = Arc<Mutex<Vec<(Caller, String)>>>;

struct Items {
    caller: Caller,
    seen: Seen,
}

impl items::Server for Items {
    #[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
    async fn assign(
        self: Rc<Self>,
        params: items::AssignParams,
        _: items::AssignResults,
    ) -> Result<(), capnp::Error> {
        let claimed = params.get()?.get_name()?.to_string()?;
        self.seen
            .lock()
            .unwrap()
            .push((self.caller.clone(), claimed));
        Err(capnp::Error::failed("recorded".into()))
    }
}

struct Root(Caller, Seen);

impl tasks::Server for Root {
    #[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
    async fn items(
        self: Rc<Self>,
        _: tasks::ItemsParams,
        mut results: tasks::ItemsResults,
    ) -> Result<(), capnp::Error> {
        let i: items::Client = new_client(Items {
            caller: self.0.clone(),
            seen: self.1.clone(),
        });
        results.get().set_items(i);
        Ok(())
    }
}

struct Factory {
    seen: Seen,
    built: Arc<AtomicUsize>,
}

impl PerConnection for Factory {
    fn bootstrap(&mut self, caller: &Caller) -> Client {
        self.built.fetch_add(1, Ordering::SeqCst);
        let root: tasks::Client = new_client(Root(caller.clone(), self.seen.clone()));
        root.client
    }
}

pub struct Server {
    pub path: PathBuf,
    pub seen: Seen,
    pub built: Arc<AtomicUsize>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
    _dir: TempDir,
}

impl Server {
    pub fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t").join("tasks.sock");
        let (seen, built) = (Seen::default(), Arc::new(AtomicUsize::new(0)));
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let (ready, listening) = std::sync::mpsc::channel();
        let (p, factory) = (
            path.clone(),
            Factory {
                seen: seen.clone(),
                built: built.clone(),
            },
        );
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            tokio::task::LocalSet::new().block_on(&rt, async move {
                let bound = bind_socket(&p).await.unwrap();
                ready.send(()).unwrap();
                let _ = serve_callers(bound, factory, async {
                    let _ = stopped.await;
                })
                .await;
            });
        });
        listening.recv().unwrap();
        Self {
            path,
            seen,
            built,
            stop: Some(stop),
            thread: Some(thread),
            _dir: dir,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// One connection; the client says it is `claim` in the field the call has.
pub fn call_as(path: &Path, claim: &str) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let conn = connect::<tasks::Client>(path).await.unwrap();
        let reply = conn.client.items_request().send().promise.await.unwrap();
        let items = reply.get().unwrap().get_items().unwrap();
        let mut req = items.assign_request();
        req.get().set_name(claim);
        assert!(req.send().promise.await.is_err());
    });
}

pub fn my_name() -> String {
    // The system's own answer, asked of a child that is told a different name in its environment.
    let out = std::process::Command::new("id")
        .arg("-un")
        .env("USER", "mallory")
        .env("LOGNAME", "mallory")
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
