//! A made-up system for the tests of the ending: records what was done, lets a test choose what the process looks like.
//!
//! Pattern: Fake (test double of `Env`).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use super::ending::{Env, Held, Target, Waits, end_process};
use super::error::SingleError;
use super::rule::{Image, Own, Peer};
use crate::{Beat, Pulse, Saved};

pub(super) const ME: u32 = 1000;

pub(super) fn image(ino: u64, path: &str) -> Image {
    Image {
        dev: 7,
        ino,
        path: path.into(),
    }
}

pub(super) fn own() -> Own {
    Own {
        pid: 500,
        uid: ME,
        exe: image(10, "/usr/bin/fs-x"),
    }
}

pub(super) fn peer(start: u64) -> Peer {
    Peer {
        pid: 900,
        uid: ME,
        start: Some(start),
        exe: Some(image(10, "/usr/bin/fs-x")),
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) events: RefCell<Vec<String>>,
    pub(super) gone: Cell<bool>,
}

pub(super) struct Fake {
    pub(super) state: Rc<State>,
    pub(super) peers: RefCell<VecDeque<Peer>>,
    pub(super) beat: Option<Beat>,
    pub(super) channel: bool,
    pub(super) saved: Saved,
    pub(super) present: bool,
    pub(super) dies_on: &'static str,
}

pub(super) struct FakeHeld(Rc<State>, &'static str);

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
    #[allow(clippy::unused_async_trait_impl)]
    async fn save(&self) -> Saved {
        self.state.events.borrow_mut().push("save".into());
        self.saved
    }
    #[allow(clippy::unused_async_trait_impl)]
    async fn beat(&self) -> Option<Pulse> {
        self.state.events.borrow_mut().push("beat".into());
        self.beat.clone().map(|beat| Pulse {
            beat,
            tick_age: None,
        })
    }
    fn log(&self, _: &str) {}
}

pub(super) fn fake(peers: Vec<Peer>, dies_on: &'static str, beat: Option<Beat>) -> Fake {
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

pub(super) fn run(env: &Fake) -> Result<super::error::TakenOver, SingleError> {
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

pub(super) fn events(env: &Fake) -> Vec<String> {
    env.state.events.borrow().clone()
}

pub(super) fn why(e: &SingleError) -> &'static str {
    match e {
        SingleError::Unresponsive(s) => s.why,
        other => panic!("{other}"),
    }
}
