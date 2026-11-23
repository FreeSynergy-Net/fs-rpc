//! The heartbeat pipe of the Windows side against real pipes: the checks of both ends (SECURITY MODULE: sockets).
//!
//! Pattern: Test. Another user cannot be made here, so the check is a function the unit gets (`Admit`): the test gives one that
//! says no, and what the check protects is looked at: a client that is not admitted gets no answer and the save hook does not
//! run; a server that is not this user's is not believed. Mutants (each turns its test red): M1 the server does not check the
//! client (`client_admitted` is not asked in `serve`): `a_client_that_is_not_this_user_gets_nothing_and_runs_no_hook`;
//! M2 the client does not check the server (`server_ok` is not asked in `beat::connect`):
//! `a_server_that_is_not_this_user_is_not_believed`.

use std::os::windows::io::RawHandle;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::time::Duration;

use super::super::client_windows::exchange_with;
use super::super::server::Shared;
use super::super::{Beat, BeatError, ask_beat, ask_save};
use super::{Guard, open_with};
use crate::transport::beat::SERVER_IS_ME;
use crate::{Heartbeat, Saved};

const LIMIT: Duration = Duration::from_secs(2);

fn unique(tag: &str) -> PathBuf {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    PathBuf::from(format!(
        r"\\.\pipe\freesynergy-test-beat-{tag}-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::SeqCst)
    ))
}

fn nobody(_pipe: RawHandle) -> bool {
    false
}

fn open_nobody(socket: &std::path::Path, shared: &Arc<Shared>) -> Guard {
    open_with(socket, shared, nobody).unwrap().1
}

#[tokio::test]
async fn this_user_is_answered_on_the_pipe_of_the_beat() {
    let socket = unique("me");
    let hb = Heartbeat::start(&socket).unwrap();
    assert!(hb.path().to_string_lossy().ends_with(".beat"));
    let pulse = ask_beat(&socket, LIMIT).await.unwrap();
    assert_eq!(pulse.beat, Beat::Alive);
    hb.busy("importing", 3);
    assert!(matches!(
        ask_beat(&socket, LIMIT).await.unwrap().beat,
        Beat::Busy { progress: 3, .. }
    ));
}

#[tokio::test]
async fn a_client_that_is_not_this_user_gets_nothing_and_runs_no_hook() {
    let socket = unique("foreign-client");
    let shared = Arc::new(Shared::new());
    static RUNS: AtomicUsize = AtomicUsize::new(0);
    // The hook counts its runs; a foreign client must never run it.
    *shared.hook_for_test() = Some(Arc::new(|| {
        RUNS.fetch_add(1, Ordering::SeqCst);
        Saved::Saved
    }));
    let _guard = open_nobody(&socket, &shared);
    let e = ask_beat(&socket, LIMIT).await.unwrap_err();
    assert!(matches!(e, BeatError::NoAnswer), "{e:?}");
    let e = ask_save(&socket, LIMIT).await.unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof, "{e:?}");
    assert_eq!(
        RUNS.load(Ordering::SeqCst),
        0,
        "the hook of a foreign client ran"
    );
}

#[tokio::test]
async fn a_server_that_is_not_this_user_is_not_believed() {
    let socket = unique("foreign-server");
    let hb = Heartbeat::start(&socket).unwrap();
    let e = exchange_with(&socket, "ping", LIMIT, nobody)
        .await
        .unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied, "{e:?}");
    assert!(matches!(BeatError::from(e), BeatError::Refused));
    // The same call with the real check is believed.
    assert!(
        exchange_with(&socket, "ping", LIMIT, SERVER_IS_ME)
            .await
            .is_ok()
    );
    drop(hb);
}

#[tokio::test]
async fn the_pipe_has_one_owner_and_goes_with_the_heartbeat() {
    let socket = unique("owner");
    let hb = Heartbeat::start(&socket).unwrap();
    let again = Heartbeat::start(&socket)
        .err()
        .expect("one heartbeat per pipe");
    assert_eq!(again.kind(), std::io::ErrorKind::AddrInUse, "{again:?}");
    drop(hb);
    assert!(matches!(
        ask_beat(&socket, LIMIT).await,
        Err(BeatError::NoAnswer)
    ));
    drop(Heartbeat::start(&socket).unwrap()); // a new one can start
}

#[tokio::test]
async fn a_slow_save_does_not_block_a_ping() {
    let socket = unique("slow-save");
    let hb = Heartbeat::start(&socket).unwrap();
    hb.on_save(|| {
        std::thread::sleep(Duration::from_millis(800));
        Saved::Saved
    });
    let saving = tokio::spawn({
        let socket = socket.clone();
        async move { ask_save(&socket, Duration::from_secs(5)).await }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let began = std::time::Instant::now();
    ask_beat(&socket, LIMIT).await.unwrap();
    assert!(
        began.elapsed() < Duration::from_millis(500),
        "the ping waited for the save"
    );
    assert_eq!(saving.await.unwrap().unwrap(), Saved::Saved);
}
