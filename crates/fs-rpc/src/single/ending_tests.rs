//! The ending of a hanging server, on a made-up system: the order of the steps and every guard.
//!
//! Pattern: Fake `Env` (`ending_fake.rs`) that writes down what was done. Mutants (each turns its test red): M1 save after the signal (or
//! none); M2 no second look after the `pidfd` (a changed process is ended); M3 SIGKILL without waiting for SIGTERM to run out; M4 SIGKILL
//! although a heartbeat still answers (also `busy`); M5 no third look before SIGKILL; M6 another program is saved or signalled (it is only
//! reported); M7 the socket file is removed although it is not the one that was seen; M8 a process that is gone before it is held gets a
//! signal; M9 a refusal is sent as `Ok`; M10 a program without a heartbeat channel gets a signal (SIGTERM or SIGKILL).

use super::ending::remove_if_same;
use super::ending_fake::{events, fake, image, peer, run, why};
use super::rule::Peer;
use crate::{Beat, Saved};

#[test]
fn it_saves_first_then_holds_then_ends_with_sigterm() {
    let env = fake(vec![peer(4242)], "term", None);
    let taken = run(&env).unwrap();
    assert_eq!(
        (taken.pid, taken.saved, taken.how),
        (900, Saved::Saved, "sigterm")
    );
    assert_eq!(
        events(&env),
        ["save", "hold", "term"],
        "save before any signal, no kill"
    );
}

#[test]
fn sigkill_comes_only_after_sigterm_ran_out_and_nothing_answers() {
    let env = fake(vec![peer(4242)], "kill", None);
    let taken = run(&env).unwrap();
    assert_eq!(taken.how, "sigkill");
    assert_eq!(events(&env), ["save", "hold", "term", "beat", "kill"]);
}

#[test]
fn a_heartbeat_that_still_answers_is_never_killed_busy_or_alive() {
    for beat in [
        Beat::Alive,
        Beat::Busy {
            what: "importing".into(),
            progress: 3,
        },
    ] {
        let env = fake(vec![peer(4242)], "never", Some(beat));
        let e = run(&env).unwrap_err();
        assert_eq!(why(&e), "beating");
        assert!(
            !events(&env).contains(&"kill".to_owned()),
            "{:?}",
            events(&env)
        );
    }
}

#[test]
fn a_process_that_does_not_end_even_after_sigkill_is_reported() {
    let env = fake(vec![peer(4242)], "never", None);
    assert_eq!(why(&run(&env).unwrap_err()), "not-ending");
}

#[test]
fn another_program_is_only_reported_nothing_is_saved_or_signalled() {
    let other = Peer {
        exe: Some(image(11, "/home/x/bin/fs-x")),
        ..peer(4242)
    };
    let env = fake(vec![other], "term", None);
    assert_eq!(why(&run(&env).unwrap_err()), "other-program");
    assert!(events(&env).is_empty(), "{:?}", events(&env));
}

#[test]
fn another_user_a_reused_number_and_the_system_are_refused_before_anything_is_done() {
    for (p, expected) in [
        (
            Peer {
                uid: 0,
                ..peer(4242)
            },
            "foreign-user",
        ),
        (peer(1), "pid-reused"),
        (
            Peer {
                pid: 1,
                ..peer(4242)
            },
            "itself",
        ),
        (
            Peer {
                start: None,
                ..peer(4242)
            },
            "unreadable",
        ),
    ] {
        let env = fake(vec![p], "term", None);
        assert_eq!(why(&run(&env).unwrap_err()), expected);
        assert_eq!(events(&env), Vec::<String>::new());
    }
}

#[test]
fn a_process_that_changed_after_the_pidfd_is_taken_is_not_signalled() {
    // First look: the process that was seen. Second look, with the descriptor in hand: another start time.
    let env = fake(vec![peer(4242), peer(9999)], "term", None);
    assert_eq!(why(&run(&env).unwrap_err()), "pid-reused");
    assert_eq!(events(&env), ["save", "hold"], "no signal");
}

#[test]
fn a_process_that_changed_before_sigkill_is_not_killed() {
    // Looks 1 and 2 are fine; the third (after SIGTERM ran out) shows another process.
    let env = fake(vec![peer(4242), peer(4242), peer(7777)], "kill", None);
    assert_eq!(why(&run(&env).unwrap_err()), "pid-reused");
    assert!(!events(&env).contains(&"kill".to_owned()));
}

#[test]
fn a_process_that_is_gone_before_it_is_held_gets_no_signal() {
    let mut env = fake(vec![peer(4242)], "term", None);
    env.present = false;
    let taken = run(&env).unwrap();
    assert_eq!(taken.how, "gone");
    assert_eq!(events(&env), ["save", "hold"]);
}

#[test]
fn only_the_socket_file_that_belonged_to_the_ended_process_is_removed() {
    use std::os::unix::fs::MetadataExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.sock");
    let old = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let ino = std::fs::symlink_metadata(&path).unwrap().ino();
    std::fs::remove_file(&path).unwrap();
    let new = std::os::unix::net::UnixListener::bind(&path).unwrap();
    assert!(
        !remove_if_same(&path, Some(ino)),
        "a successor's socket was removed"
    );
    assert!(path.exists());
    assert!(!remove_if_same(&path, None));
    let seen = std::fs::symlink_metadata(&path).unwrap().ino();
    assert!(remove_if_same(&path, Some(seen)));
    assert!(!path.exists());
    drop((old, new));
    std::fs::write(&path, "plain").unwrap();
    let plain = std::fs::symlink_metadata(&path).unwrap().ino();
    assert!(
        !remove_if_same(&path, Some(plain)),
        "a plain file is never removed"
    );
}

#[test]
fn a_program_without_a_heartbeat_channel_gets_no_signal_only_a_report() {
    for dies_on in ["term", "kill", "never"] {
        let mut env = fake(vec![peer(4242)], dies_on, None);
        env.channel = false;
        assert_eq!(why(&run(&env).unwrap_err()), "no-control");
        assert!(
            events(&env).is_empty(),
            "nothing saved, held or signalled: {:?}",
            events(&env)
        );
    }
}

#[test]
fn a_failure_tells_what_the_system_knows_of_the_process_including_when_it_started() {
    let mut env = fake(vec![peer(4242)], "term", None);
    env.channel = false;
    let Err(super::error::SingleError::Unresponsive(s)) = run(&env) else {
        panic!("no report");
    };
    assert_eq!((s.pid, s.start), (Some(900), Some(4242)));
    assert_eq!(s.exe.as_deref(), Some("/usr/bin/fs-x"));
    assert!(!s.can_ask, "ensure_single does not ask: the program does");
    let since = s.since.expect("the start is known, so is the time");
    assert_eq!(Some(since), super::procfs::since_of(4242));
}
