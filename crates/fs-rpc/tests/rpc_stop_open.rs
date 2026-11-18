//! A stop that comes while `open` hangs ends the run in bounded time (Package P-RPC-SIGNAL). Alone in its file:
//! a request to stop is remembered by the whole process. Platform neutral: `stop_self` is the stop that Windows has too.
//! Pattern: Watchdog (the run is on its own thread, the test is red after a limit instead of hanging).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use fs_rpc::{run_until_signal, stop_self};

/// Only against a stuck run; not what the test measures.
const WATCHDOG: Duration = Duration::from_secs(20);
/// The stop comes this long after the start, while `open` hangs.
const STOP_AFTER: Duration = Duration::from_millis(300);

/// What dropping the unfinished `open` takes.
const CLOSING: Duration = Duration::from_millis(30);
/// The run must end this soon after the stop (generous: the real time is a few milliseconds).
const BOUND: Duration = Duration::from_secs(5);

static OPEN_DROPPED: AtomicBool = AtomicBool::new(false);

struct SetOnDrop;
impl Drop for SetOnDrop {
    fn drop(&mut self) {
        // Closing a half-open store takes a moment: the run must wait for it (and the test must be able to tell).
        std::thread::sleep(CLOSING);
        OPEN_DROPPED.store(true, Ordering::SeqCst);
    }
}

#[test]
fn a_stop_while_open_hangs_ends_the_run_cleanly_and_leaves_no_socket() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.sock");
    let socket = path.clone();
    let (done, finished) = mpsc::channel();
    let run = std::thread::spawn(move || {
        let result = run_until_signal(&socket, || async {
            let _guard = SetOnDrop;
            // The store never opens.
            std::future::pending::<()>().await;
            unreachable!("open was never to finish")
        });
        let _ = done.send(result.map_err(|e| e.to_string()));
    });
    std::thread::sleep(STOP_AFTER);
    let began = Instant::now();
    stop_self();
    let result = finished
        .recv_timeout(WATCHDOG)
        .expect("the run did not end after the stop: `open` is not raced against it");
    assert!(
        began.elapsed() >= CLOSING,
        "the run ended before the unfinished open was dropped"
    );
    assert!(
        began.elapsed() < BOUND,
        "the run took too long to end after the stop"
    );
    assert_eq!(result, Ok(()), "a stop during open is no error");
    assert!(OPEN_DROPPED.load(Ordering::SeqCst), "open was not dropped");
    assert!(!path.exists(), "the socket stayed behind");
    run.join().unwrap();
}
