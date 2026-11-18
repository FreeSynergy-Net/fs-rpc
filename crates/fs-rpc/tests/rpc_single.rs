//! `ensure_single` against real hanging servers: a child process (this test binary again, or a copy of it) holds the socket and does
//! not answer.
//!
//! Pattern: Table of scenarios, one child per test, serial (`ONE_AT_A_TIME`). Rules and their tests (the mutant that removes the rule
//! turns the test red; the pure rules have unit tests in `src/single/*_tests.rs`):
//! R1 own program, hangs, heartbeat with a hook -> saved, then ended, replaced (`a_hanging_server_with_a_heartbeat_is_saved_then_ended`);
//! R2 hanging without a heartbeat channel -> NO signal at all, only `no-control` (`a_hanging_server_without_a_heartbeat_gets_no_signal_only_a_report`);
//! R3 busy and moving -> never ended, `server-busy` (`a_busy_server_that_moves_is_never_ended`);
//! R4 busy, number stands still -> hanging -> ended (`a_busy_server_that_stands_still_is_ended`);
//! R5 another program under the same name (a copy of the file) -> reported at once, never signalled, nothing saved
//! (`another_program_is_only_reported`); R6 the heartbeat answers but the work thread's tick stands still -> hanging -> ended
//! (`a_blocked_work_thread_behind_a_live_heartbeat_is_ended`); R8 every wait has its limit
//! (`the_whole_call_has_a_limit`). Mutants outside the table: M1 no limit on the whole call; M2 a signal for R2; M3 `Busy` counts as
//! hanging; M4 the copy of the program is taken for this program; M5 the heartbeat answer counts as health although the tick stands still.
#![cfg(target_os = "linux")]

use std::time::{Duration, Instant};

use fs_rpc::{RpcError, RpcErrorCode, Saved, SingleError};

mod handshake_support;
mod single_support;
use handshake_support::*;
use single_support::*;

#[test]
fn helper_hanging_server() {
    hanging_server_main();
}

#[test]
fn a_hanging_server_with_a_heartbeat_is_saved_then_ended() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, marker) = setup("silent-beat", true);
    let pid = child.0.id();
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600); // the child never calls tick(): only the standing tick can say hanging
    let taken = run(&socket, &t, None)
        .expect("taken over and replaced")
        .expect("a process was ended");
    assert_eq!(
        (taken.pid, taken.saved, taken.how),
        (pid, Saved::Saved, "sigterm")
    );
    assert!(marker.exists(), "the program saved before it was ended");
    assert!(!alive(&mut child));
    let log = std::fs::read_to_string(socket.with_extension("takeover.log")).unwrap();
    assert!(
        log.contains(&format!("pid={pid}")) && log.contains("saved=saved"),
        "{log}"
    );
}

#[test]
fn a_hanging_server_without_a_heartbeat_gets_no_signal_only_a_report() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("silent", false);
    let error = run(&socket, &times(), None).expect_err("not connected");
    let coded = RpcError::from(&error);
    assert_eq!(coded.code(), RpcErrorCode::ServerUnresponsive);
    assert_eq!(
        coded.param("why").and_then(fs_rpc::Param::as_text),
        Some("no-control")
    );
    assert_eq!(
        coded.param("pid").and_then(fs_rpc::Param::as_number),
        Some(u64::from(child.0.id()))
    );
    assert_started_just_now(&coded);
    std::thread::sleep(Duration::from_millis(300));
    assert!(alive(&mut child), "no signal: it could not save first");
}

#[test]
fn a_busy_server_that_moves_is_never_ended() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("busy-moving", false);
    let mut t = times();
    t.overall = Duration::from_millis(4000);
    t.hang.no_progress = Duration::from_millis(700);
    let began = Instant::now();
    let error = run(&socket, &t, None).expect_err("not connected");
    assert!(
        began.elapsed() < Duration::from_secs(8),
        "the call has its limit"
    );
    match &error {
        SingleError::Busy {
            pid,
            what,
            progress,
        } => {
            assert_eq!(*pid, Some(child.0.id()));
            assert_eq!(what, "importing");
            assert!(*progress > 1);
        }
        other => panic!("{other}"),
    }
    let coded = RpcError::from(&error);
    assert_eq!(coded.code(), RpcErrorCode::ServerBusy);
    assert_eq!(coded.message_key(), "rpc-error-server-busy");
    assert!(alive(&mut child), "a busy program is never ended");
}

#[test]
fn a_busy_server_that_stands_still_is_ended() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("busy-stalled", true);
    let taken = run(&socket, &times(), None).expect("ended after the standstill");
    assert!(taken.is_some());
    assert!(!alive(&mut child));
}

#[test]
fn a_blocked_work_thread_behind_a_live_heartbeat_is_ended() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600); // only the tick can say hanging here
    let (_dir, socket, mut child, _) = setup("stuck-worker", true);
    let began = Instant::now();
    let taken = run(&socket, &t, None).expect("ended because the tick stood still");
    assert!(began.elapsed() < Duration::from_secs(15));
    assert!(taken.is_some());
    assert!(!alive(&mut child));
}

#[test]
fn another_program_is_only_reported() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("the-same-name");
    std::fs::copy(std::env::current_exe().unwrap(), &copy).unwrap();
    let socket = dir.path().join("s.sock");
    let marker = dir.path().join("saved.marker");
    let mut child = spawn_child(&copy, &socket, "silent-beat", Some(&marker));
    let began = Instant::now();
    let error = run(&socket, &times(), None).expect_err("not connected");
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "reported at once: {:?}",
        began.elapsed()
    );
    let coded = RpcError::from(&error);
    assert_eq!(coded.code(), RpcErrorCode::ServerUnresponsive);
    assert_eq!(
        coded.param("why").and_then(fs_rpc::Param::as_text),
        Some("other-program")
    );
    assert_eq!(
        coded.param("pid").and_then(fs_rpc::Param::as_number),
        Some(u64::from(child.0.id()))
    );
    assert!(coded.param("exe").is_some());
    assert_started_just_now(&coded);
    assert!(alive(&mut child), "another program is never ended");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!marker.exists(), "nothing was saved in it either");
    // And when the copy is the program that counts as ours, it is ours: the program file is a parameter.
}

#[test]
fn the_whole_call_has_a_limit() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(60);
    let (_dir, socket, mut child, _) = setup("silent-beat", false);
    let mut t = times();
    t.hang.no_progress = Duration::from_secs(600);
    t.hang.no_tick = Duration::from_secs(600);
    t.hang.no_beat = Duration::from_secs(600);
    t.overall = Duration::from_millis(1500);
    let began = Instant::now();
    let error = run(&socket, &t, None).expect_err("not connected");
    assert!(
        began.elapsed() < Duration::from_secs(5),
        "{:?}",
        began.elapsed()
    );
    assert!(matches!(error, SingleError::TooLong(_)), "{error}");
    assert!(alive(&mut child));
}

/// The failure says when the silent process started (`start` in ticks, `since` as UTC text): a process of this test, a moment ago.
fn assert_started_just_now(coded: &RpcError) {
    assert!(
        coded
            .param("start")
            .and_then(fs_rpc::Param::as_number)
            .is_some()
    );
    let since = coded
        .param("since")
        .and_then(fs_rpc::Param::as_text)
        .expect("since");
    let at = chrono::DateTime::parse_from_rfc3339(since)
        .expect("RFC 3339")
        .timestamp();
    let now = chrono::Utc::now().timestamp();
    assert!(
        (now - 600..=now + 2).contains(&at),
        "{since} is not a start within the last minutes"
    );
}
