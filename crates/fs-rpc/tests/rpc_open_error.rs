//! An `open` that fails ends the run with that error (and no socket is left): it is not swallowed into a quiet `Ok`.
//! Alone in its file: nothing here stops the run from outside.
//!
//! Pattern: Error-path test. Mutant: the error of `open` is dropped (`per.ok()`), the run then serves nothing and waits for ever.

use std::io;
use std::time::Duration;

use fs_rpc::run_until_signal;

#[test]
fn a_failing_open_is_the_error_of_the_run_and_leaves_no_socket() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(30));
        eprintln!("watchdog: the run did not end after the error of open");
        std::process::exit(101);
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.sock");
    let error = run_until_signal(&path, || async {
        Err::<capnp::capability::Client, _>(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "no store",
        ))
    })
    .expect_err("open failed");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().contains("no store"), "{error}");
    assert!(!path.exists(), "the socket stayed behind");
}
