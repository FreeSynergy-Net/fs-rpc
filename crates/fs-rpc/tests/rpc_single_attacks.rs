//! Attacks on `ensure_single` (found by the review of 36e81faa), each a regression test with its mutant: a leftover `.beat` file, the
//! heartbeat of another process, a copy named `<exe> (deleted)`, long work that does not tick.
//!
//! Pattern: as `rpc_single.rs` (real child processes, one at a time, a watchdog on every test). `FS_TEST_SCALE` scales the waits.
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::time::Duration;

use fs_rpc::{Heartbeat, RpcError, Saved, SingleError};

mod handshake_support;
mod single_support;
use handshake_support::*;
use single_support::*;

#[test]
fn helper_hanging_server() {
    hanging_server_main();
}

/// A: a leftover `<socket>.beat` file (nobody listens) is no channel: a program without a heartbeat gets no signal at all.
/// Mutant: the channel is taken from `exists()` of the file (then it is saved, `SIGTERM`ed and killed).
#[test]
fn a_leftover_heartbeat_file_is_no_channel() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("ignore-term", false);
    drop(std::os::unix::net::UnixListener::bind(socket.with_extension("beat")).unwrap()); // the file stays, nobody listens
    assert!(socket.with_extension("beat").exists());
    let error = run(&socket, &times(), None).expect_err("not connected");
    let coded = RpcError::from(&error);
    assert_eq!(
        coded.param("why").and_then(fs_rpc::Param::as_text),
        Some("no-control")
    );
    assert!(
        alive(&mut child),
        "no SIGTERM, no SIGKILL without a proven channel"
    );
}

/// A: the heartbeat of ANOTHER process (here: this test process) next to a silent server is no channel of that server: the process
/// that answers must be the one that listens on the socket. Mutant: the pid of the answering process is not compared.
#[test]
fn the_heartbeat_of_another_process_is_no_channel() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (dir, socket, mut child, marker) = setup("silent", false);
    let neighbour = Heartbeat::start(&socket).unwrap();
    neighbour.on_save({
        let marker = marker.clone();
        move || {
            std::fs::write(&marker, "saved").unwrap();
            Saved::Saved
        }
    });
    let error = run(&socket, &times(), None).expect_err("not connected");
    let coded = RpcError::from(&error);
    assert_eq!(
        coded.param("why").and_then(fs_rpc::Param::as_text),
        Some("no-control")
    );
    assert!(alive(&mut child));
    assert!(!marker.exists(), "the neighbour was never asked to save");
    drop((neighbour, dir));
}

/// A: a COPY whose file name is `<our file> (deleted)` is another program (the kernel's word only counts for a file that is gone,
/// link count 0). Mutant: ` (deleted)` is always cut off (then the copy is "the same real path": saved and ended).
#[test]
fn a_copy_named_deleted_is_another_program() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let exe = std::env::current_exe().unwrap();
    let copy = PathBuf::from(format!("{} (deleted)", exe.display()));
    std::fs::copy(&exe, &copy).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("s.sock");
    let marker = dir.path().join("saved.marker");
    let mut child = spawn_child(&copy, &socket, "silent-beat", Some(&marker));
    let error = run(&socket, &times(), None).expect_err("not connected");
    let coded = RpcError::from(&error);
    assert_eq!(
        coded.param("why").and_then(fs_rpc::Param::as_text),
        Some("other-program")
    );
    assert!(alive(&mut child), "another program is never ended");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "nothing was saved in it");
    drop(child);
    std::fs::remove_file(&copy).unwrap();
}

/// Long work in the work thread that neither ticks nor says busy is not ended before the tick has stood still for the whole watch.
/// Scaled: `no_tick` is 3 s x `FS_TEST_SCALE` here (the plan says 10 s); the program has already run longer than that (so its tick age is
/// large at the first ping), the call is allowed 2 s x scale, i.e. less than `no_tick`: nothing may be ended, the answer is the time
/// limit. Mutant: hanging without the watch window, or a limit of 2 s until the end (the child is saved and killed, the call connects).
#[test]
fn long_work_that_does_not_tick_is_not_ended_within_the_tick_limit() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(90);
    let (_dir, socket, mut child, marker) = setup("silent-beat", true);
    let mut t = times();
    t.hang.no_tick = Duration::from_secs(3) * scale();
    t.hang.no_beat = Duration::from_secs(600);
    t.hang.no_progress = Duration::from_secs(600);
    t.overall = Duration::from_secs(2) * scale();
    std::thread::sleep(t.hang.no_tick + Duration::from_millis(300) * scale());
    let error = run(&socket, &t, None).expect_err("not connected");
    assert!(matches!(error, SingleError::TooLong(_)), "{error}");
    assert!(alive(&mut child), "long work is not ended");
    assert!(!marker.exists(), "and nothing was asked of it");
}
