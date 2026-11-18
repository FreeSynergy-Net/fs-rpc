//! A stop that comes while `open` runs, where `open` then ends at once, still ends the run: the stop is not lost between the two
//! stages (open, then serving). Alone in its file: the signal goes to this whole process.
//!
//! Pattern: Watchdog (own thread, red after a limit instead of hanging). The signal is sent from inside `open`, which ends in the
//! same poll: the runtime has not heard of the signal yet, so `open` is ready first and the stop has to be kept for the serving.
//! Mutant: the serving stage listens to a new stop future instead of the one that heard the signal (the signal is lost: it hangs).
#![cfg(unix)] // signals sent with `kill`

use std::time::Duration;

use capnp_rpc::new_client;
use fs_interfaces::common_capnp::blob;
use fs_rpc::run_until_signal;
use rustix::process::{Signal, getpid, kill_process};

struct NoBlob;
impl blob::Server for NoBlob {}

/// TERM and INT in turn inside one test: the signal goes to the whole process.
#[test]
fn a_signal_during_open_where_open_then_ends_stops_the_serving_all_the_same() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(30));
        eprintln!("watchdog: the run did not end after the signal");
        std::process::exit(101);
    });
    // When `open` ends and the signal is heard in the same moment, the run picks one of the two at random: so many rounds, that the
    // way "open first, then the serving has to hear the stop" is taken in some of them (the mutant hangs there, red by the watchdog).
    for round in 0..40 {
        for (name, signal) in [("x.sock", Signal::TERM), ("y.sock", Signal::INT)] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join(name);
            run_until_signal(&path, || async move {
                kill_process(getpid(), signal).unwrap();
                let root: blob::Client = new_client(NoBlob);
                Ok(root.client)
            })
            .unwrap();
            assert!(
                !path.exists(),
                "round {round}: the socket stayed behind after {signal:?}"
            );
        }
    }
}
