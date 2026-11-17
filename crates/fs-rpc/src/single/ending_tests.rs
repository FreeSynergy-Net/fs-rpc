//! The ending of a hanging server, on a made-up system: the order of the steps and every guard.
//!
//! Pattern: Fake `Env` that writes down what was done. Mutants (each turns its test red): M1 save after the signal (or none);
//! M2 no second look after the `pidfd` (a changed process is ended); M3 SIGKILL without waiting for SIGTERM to run out; M4 SIGKILL
//! although a heartbeat still answers (also `busy`); M5 no third look before SIGKILL; M6 another program is saved or signalled
//! (it is only reported); M10 a program without a heartbeat channel is killed; M7 the socket file is removed although it is not the one that was seen; M8 a process that is gone
//! before it is held gets a signal; M9 a refusal is sent as `Ok`.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use super::ending::{Env, Held, Target, Waits, end_process, remove_if_same};
use super::error::SingleError;
use super::rule::{Image, Own, Peer};
use crate::{Beat, Pulse, Saved};

const ME: u32 = 1000;

fn image(ino: u64, path: &str) -> Image {
    Image {
        dev: 7,
        ino,
        path: path.into(),
    }
}

fn own() -> Own {
    Own {
        pid: 500,
        uid: ME,
        exe: image(10, "/usr/bin/fs-x"),
    }
}

fn peer(start: u64) -> Peer {
    Peer {
        pid: 900,
        uid: ME,
        start: Some(start),
        exe: Some(image(10, "/usr/bin/fs-x")),
    }
}

#[derive(Default)]
struct State {
    events: RefCell<Vec<String>>,
    gone: Cell<bool>,
}

struct Fake {
    state: Rc<State>,
    peers: RefCell<VecDeque<Peer>>,
    beat: Option<Beat>,
    channel: bool,
    saved: Saved,
    present: bool,
    dies_on: &'static str,
}

struct FakeHeld(Rc<State>, &'static str);

impl Held for FakeHeld {
    fn term(&self) {
        self.0.events.borrow_mut().push("term".into());
        if self.1 == "term" {
            self.0.gone.set(true);
        }
    }
    fn kill(&self) {
        self.0.events.borrow_mut().push("kill".into());
        if self.1 != "never" {
            self.0.gone.set(true);
        }
    }
}

impl Env for Fake {
    fn peer(&self, _: i32, _: u32) -> Peer {
        let mut q = self.peers.borrow_mut();
        if q.len() > 1 {
            q.pop_front().unwrap()
        } else {
            q[0].clone()
        }
    }
    fn hold(&self, _: i32) -> std::io::Result<Option<Box<dyn Held>>> {
        self.state.events.borrow_mut().push("hold".into());
        Ok(self
            .present
            .then(|| Box::new(FakeHeld(self.state.clone(), self.dies_on)) as Box<dyn Held>))
    }
    fn gone(&self, _: &Target) -> bool {
        self.state.gone.get()
    }
    async fn save(&self) -> Saved {
        self.state.events.borrow_mut().push("save".into());
        self.saved
    }
    async fn beat(&self) -> Option<Pulse> {
        self.state.events.borrow_mut().push("beat".into());
        self.beat.clone().map(|beat| Pulse {
            beat,
            tick_age: None,
        })
    }
    fn log(&self, _: &str) {}
}

fn fake(peers: Vec<Peer>, dies_on: &'static str, beat: Option<Beat>) -> Fake {
    Fake {
        state: Rc::default(),
        peers: RefCell::new(peers.into()),
        beat,
        channel: true,
        saved: Saved::Saved,
        present: true,
        dies_on,
    }
}

fn run(env: &Fake) -> Result<super::error::TakenOver, SingleError> {
    let waits = Waits {
        term: Duration::from_millis(80),
        kill: Duration::from_millis(80),
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(end_process(
            env,
            &Target {
                pid: 900,
                start: 4242,
            },
            ME,
            &own(),
            waits,
            Duration::from_secs(1),
            env.channel,
        ))
}

fn events(env: &Fake) -> Vec<String> {
    env.state.events.borrow().clone()
}

fn why(e: &SingleError) -> &'static str {
    match e {
        SingleError::Unresponsive(s) => s.why,
        other => panic!("{other}"),
    }
}

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
        assert!(events(&env).is_empty());
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
fn a_program_without_a_heartbeat_channel_is_never_killed_only_reported() {
    let mut env = fake(vec![peer(4242)], "kill", None);
    env.channel = false;
    assert_eq!(why(&run(&env).unwrap_err()), "no-control");
    let events = events(&env);
    assert!(
        events.contains(&"term".to_owned()),
        "a clean end may be asked: {events:?}"
    );
    assert!(!events.contains(&"kill".to_owned()), "{events:?}");
    // If it ends after SIGTERM, all is well.
    let mut env = fake(vec![peer(4242)], "term", None);
    env.channel = false;
    assert_eq!(run(&env).unwrap().how, "sigterm");
}
