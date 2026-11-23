//! The heartbeat socket of the Unix side: the checks of both ends (SECURITY MODULE: sockets).
//!
//! Pattern: Test. Another user cannot be made here, so the check is a function the unit gets (`Admit`): the test gives one that
//! says no, and what the check protects is looked at. Mutants (each turns its test red): M1 the server does not check the
//! client (`admit` is not asked in `serve_one`): `a_client_that_is_not_this_user_gets_nothing_and_runs_no_hook`; M2 the
//! client does not check the process behind the socket (`expect_pid` is not compared): `a_heartbeat_of_another_process_is_refused`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::super::server::Shared;
use super::super::{BeatError, ask_beat, ask_beat_from, ask_save};
use super::open_with;
use crate::Saved;

const LIMIT: Duration = Duration::from_secs(2);

fn nobody(_stream: &std::os::unix::net::UnixStream) -> bool {
    false
}

fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

#[test]
fn a_client_that_is_not_this_user_gets_nothing_and_runs_no_hook() {
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("p.sock");
    let shared = Arc::new(Shared::new());
    *shared.hook_for_test() = Some(Arc::new(|| {
        RUNS.fetch_add(1, Ordering::SeqCst);
        Saved::Saved
    }));
    let (_path, _guard) = open_with(&socket, &shared, nobody).unwrap();
    let e = block_on(ask_beat(&socket, LIMIT)).unwrap_err();
    assert!(matches!(e, BeatError::NoAnswer), "{e:?}");
    let e = block_on(ask_save(&socket, LIMIT)).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof, "{e:?}");
    assert_eq!(
        RUNS.load(Ordering::SeqCst),
        0,
        "the hook of a foreign client ran"
    );
}

#[test]
fn a_heartbeat_of_another_process_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("p.sock");
    let _hb = crate::Heartbeat::start(&socket).unwrap();
    let mine = i32::try_from(std::process::id()).unwrap();
    assert!(block_on(ask_beat_from(&socket, LIMIT, Some(mine))).is_ok());
    let e = block_on(ask_beat_from(&socket, LIMIT, Some(mine + 1))).unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied, "{e:?}");
    assert!(matches!(BeatError::from(e), BeatError::Refused));
}
