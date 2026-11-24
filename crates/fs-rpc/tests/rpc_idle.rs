//! The optional end of `serve_options` after a time without a connection (SB-3). Platform neutral: a socket on Unix, a pipe on Windows.
//! Pattern: Test (real socket, short real times; the margins are wide: the end must not come while a connection is up).

use std::time::{Duration, Instant};

use capnp_rpc::new_client;
use fs_interfaces::common_capnp::blob;
use fs_rpc::{ServeOptions, bind_socket, connect, serve_options};

struct NoBlob;
impl blob::Server for NoBlob {}

#[cfg(unix)]
fn place(_test: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("i.sock");
    (dir, path)
}

#[cfg(windows)]
fn place(test: &str) -> ((), std::path::PathBuf) {
    (
        (),
        format!(
            r"\\.\pipe\freesynergy-test-idle-{}-{test}",
            std::process::id()
        )
        .into(),
    )
}

fn run<F: Future<Output = ()>>(test: F) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, test);
}

const IDLE: Duration = Duration::from_millis(400);

#[test]
fn the_default_options_have_no_end() {
    assert_eq!(ServeOptions::default().idle_timeout, None);
    assert_eq!(
        ServeOptions::default().with_idle_timeout(IDLE).idle_timeout,
        Some(IDLE)
    );
}

#[test]
fn the_service_ends_after_the_time_without_a_connection() {
    let (_dir, path) = place("ends");
    run(async {
        let bound = bind_socket(&path).await.unwrap();
        let root: blob::Client = new_client(NoBlob);
        let began = Instant::now();
        let options = ServeOptions::default().with_idle_timeout(IDLE);
        let ended = tokio::time::timeout(
            Duration::from_secs(10),
            serve_options(bound, root.client, std::future::pending(), options),
        )
        .await
        .expect("no end after the idle time");
        ended.unwrap();
        assert!(began.elapsed() >= IDLE, "ended before the time");
    });
}

#[test]
fn a_connection_keeps_it_up_and_the_time_runs_again_after_its_end() {
    let (_dir, path) = place("kept");
    run(async {
        let bound = bind_socket(&path).await.unwrap();
        let root: blob::Client = new_client(NoBlob);
        let options = ServeOptions::default().with_idle_timeout(IDLE);
        let serving = tokio::task::spawn_local(serve_options(
            bound,
            root.client,
            std::future::pending(),
            options,
        ));
        let conn = connect::<blob::Client>(&path).await.unwrap();
        // well beyond the idle time: the connection is still there
        tokio::time::sleep(IDLE * 3).await;
        assert!(!serving.is_finished(), "ended with a connection up");
        let closed = Instant::now();
        drop(conn);
        let ended = tokio::time::timeout(Duration::from_secs(10), serving)
            .await
            .expect("no end after the connection closed");
        ended.unwrap().unwrap();
        assert!(
            closed.elapsed() >= IDLE,
            "the time did not run again from the end of the connection"
        );
    });
}

#[test]
fn without_the_option_nothing_ends_it_but_shutdown() {
    let (_dir, path) = place("plain");
    run(async {
        let bound = bind_socket(&path).await.unwrap();
        let root: blob::Client = new_client(NoBlob);
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::task::spawn_local(serve_options(
            bound,
            root.client,
            async {
                let _ = stopped.await;
            },
            ServeOptions::default(),
        ));
        tokio::time::sleep(IDLE * 2).await;
        assert!(!serving.is_finished());
        stop.send(()).unwrap();
        serving.await.unwrap().unwrap();
    });
}

/// R1: the idle end also reaches the signal way (`run_until_signal_serving`); without it only the stop ends the run.
#[test]
fn run_until_signal_serving_ends_after_the_idle_time_and_not_without_it() {
    use fs_rpc::{ServerIdentity, Serving, Shared, run_until_signal_serving, stop_self};
    let (_dir, path) = place("serving");
    let serve = |serving: Serving, path: std::path::PathBuf| {
        std::thread::spawn(move || {
            run_until_signal_serving(&path, serving, || async {
                let root: blob::Client = new_client(NoBlob);
                Ok(Shared(root.client))
            })
        })
    };
    let identity = ServerIdentity::new("test", "0.0.1");
    let began = Instant::now();
    let with = serve(
        Serving::new(identity.clone()).with_idle_timeout(IDLE),
        path.clone(),
    );
    with.join().unwrap().unwrap();
    assert!(began.elapsed() >= IDLE, "ended before the idle time");
    assert!(!path.exists() || cfg!(windows), "the socket is removed");
    // Without the idle time nothing ends it but the stop.
    let without = serve(Serving::new(identity), path);
    std::thread::sleep(IDLE * 3);
    assert!(!without.is_finished(), "ended without an idle time");
    stop_self();
    without.join().unwrap().unwrap();
}
