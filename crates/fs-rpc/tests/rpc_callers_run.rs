//! `run_until_signal_callers` on a real socket with a counting `PerConnection`: every connection gets a capability of its own
//! (built when it comes, with its caller), and a stop through `Control` still ends the run. A capability built once and handed
//! to all would pass the single-connection tests and still be wrong: here each connection must show its own number.
//! Alone in its file: the run registers the process signal handlers.
#![cfg(unix)]

use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use capnp::capability::Client;
use capnp_rpc::new_client;
use fs_interfaces::tasks_capnp::{items, tasks};
use fs_rpc::{
    Caller, PerConnection, ServerIdentity, connect, run_until_signal_callers, stop_server,
};

/// Says which connection it was built for (its number, counted from 1) in the error of `assign`.
struct Items(usize, String);

impl items::Server for Items {
    #[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
    async fn assign(
        self: Rc<Self>,
        _: items::AssignParams,
        _: items::AssignResults,
    ) -> Result<(), capnp::Error> {
        Err(capnp::Error::failed(format!(
            "n={} caller={}",
            self.0, self.1
        )))
    }
}

struct Root(usize, String);

impl tasks::Server for Root {
    #[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
    async fn items(
        self: Rc<Self>,
        _: tasks::ItemsParams,
        mut results: tasks::ItemsResults,
    ) -> Result<(), capnp::Error> {
        let i: items::Client = new_client(Items(self.0, self.1.clone()));
        results.get().set_items(i);
        Ok(())
    }
}

struct Counting(Arc<AtomicUsize>);

impl PerConnection for Counting {
    fn bootstrap(&mut self, caller: &Caller) -> Client {
        let n = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        let root: tasks::Client = new_client(Root(n, caller.id().to_owned()));
        root.client
    }
}

async fn which(path: &Path) -> String {
    let conn = connect::<tasks::Client>(path).await.unwrap();
    let reply = conn.client.items_request().send().promise.await.unwrap();
    let items = reply.get().unwrap().get_items().unwrap();
    let e = items.assign_request().send().promise.await.err().unwrap();
    e.to_string()
}

async fn wait_for_socket(path: &Path) {
    let end = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(Instant::now() < end, "the server did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn each_connection_gets_a_capability_built_for_it_and_control_still_stops_the_run() {
    const N: usize = 4;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("callers.sock");
    let built = Arc::new(AtomicUsize::new(0));
    let (p, counting) = (path.clone(), Counting(built.clone()));
    let server = std::thread::spawn(move || {
        run_until_signal_callers(&p, ServerIdentity::new("test", "0.0.0"), || async {
            Ok(counting)
        })
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let uid = rustix::process::geteuid().as_raw();
    let seen = tokio::task::LocalSet::new().block_on(&rt, async {
        wait_for_socket(&path).await;
        let mut seen = Vec::new();
        for _ in 0..N {
            seen.push(which(&path).await);
        }
        stop_server(&path).await.unwrap();
        seen
    });
    server.join().unwrap().unwrap();

    for (i, text) in seen.iter().enumerate() {
        let want = format!("n={} caller={uid}", i + 1);
        assert!(
            text.contains(&want),
            "connection {}: {text:?} lacks {want:?}",
            i + 1
        );
    }
    // N connections of the client, one more for the stop: each one was built for, none shared.
    assert_eq!(built.load(Ordering::SeqCst), N + 1);
}
