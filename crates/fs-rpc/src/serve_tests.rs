//! The accept loop when the system does not tell who is on a connection: made-up caller source, a real socket.
//! Pattern: Test double (a `CallerSource` that always fails) + Mutant-proof (the connection is dropped and `refused` is called).
use std::io;
use std::time::Duration;

use capnp::capability::Client;
use capnp_rpc::new_client;
use fs_interfaces::tasks_capnp::tasks;
use tokio::sync::oneshot;

use super::{ReadLimits, accept_until, bind_socket, connect};
use crate::transport::{Caller, CallerError, CallerSource, PerConnection, bootstrap_for};

struct Unknown;

impl CallerSource for Unknown {
    fn caller(&self) -> Result<Caller, CallerError> {
        Err(CallerError::Credentials(io::Error::from(
            io::ErrorKind::PermissionDenied,
        )))
    }
}

#[derive(Default)]
struct Log {
    built: usize,
    refused: Vec<&'static str>,
}

struct Nothing;
impl tasks::Server for Nothing {}

impl PerConnection for Log {
    fn bootstrap(&mut self, _: &Caller) -> Client {
        self.built += 1;
        let root: tasks::Client = new_client(Nothing);
        root.client
    }

    fn refused(&mut self, why: &CallerError) {
        self.refused.push(why.code());
    }
}

#[test]
fn a_connection_whose_caller_is_not_told_is_dropped_and_refused_is_called_for_each() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("r.sock");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let log = tokio::task::LocalSet::new().block_on(&rt, async {
        let bound = bind_socket(&path).await.unwrap();
        let (stop, stopped) = oneshot::channel::<()>();
        let task = tokio::task::spawn_local(async move {
            let mut per = Log::default();
            let served = accept_until(
                &bound,
                |_| bootstrap_for(&Unknown, &mut per),
                async {
                    let _ = stopped.await;
                },
                ReadLimits::default(),
            )
            .await;
            (served, per)
        });
        for _ in 0..3 {
            let conn = connect::<tasks::Client>(&path).await.unwrap();
            // Nothing is served to it: the connection ends instead of answering.
            let answer = conn.client.items_request().send().promise.await;
            assert!(answer.is_err(), "a dropped connection answered");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        stop.send(()).unwrap();
        let (served, per) = task.await.unwrap();
        served.unwrap();
        per
    });
    assert_eq!(log.built, 0, "a capability was built for an unknown caller");
    assert_eq!(log.refused, ["caller-credentials"; 3]);
}
