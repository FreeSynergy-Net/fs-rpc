//! SIGTERM and SIGINT while `open` hangs end the run in bounded time. Alone in its file: the signal goes to this whole
//! process. The sibling `rpc_signal_start.rs` has `open` that ends by itself; here it never does.
//! Pattern: Watchdog (own thread, red after a limit instead of hanging).
#![cfg(unix)] // signals sent with `kill`

use std::sync::mpsc;
use std::time::Duration;

use fs_rpc::run_until_signal;
use rustix::process::{Signal, getpid, kill_process};

const WATCHDOG: Duration = Duration::from_secs(20);

/// TERM and INT in turn inside one test: the signal goes to the whole process, so two tests would hear each other's.
#[test]
fn a_stop_signal_while_open_hangs_ends_the_run_and_removes_the_socket() {
    for (name, signal) in [("x.sock", Signal::TERM), ("y.sock", Signal::INT)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        let socket = path.clone();
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let result = run_until_signal(&socket, || async {
                std::future::pending::<()>().await;
                unreachable!("open was never to finish")
            });
            let _ = done.send(result.map_err(|e| e.to_string()));
        });
        // The handlers are registered before `open`; wait until the socket is bound, which comes before it.
        let began = std::time::Instant::now();
        while !path.exists() {
            assert!(began.elapsed() < WATCHDOG, "the socket was never bound");
            std::thread::sleep(Duration::from_millis(10));
        }
        kill_process(getpid(), signal).unwrap();
        let result = finished
            .recv_timeout(WATCHDOG)
            .unwrap_or_else(|_| panic!("{signal:?} did not end the run while open hangs"));
        assert_eq!(result, Ok(()), "{signal:?} during open is no error");
        assert!(!path.exists(), "the socket stayed behind after {signal:?}");
    }
}
