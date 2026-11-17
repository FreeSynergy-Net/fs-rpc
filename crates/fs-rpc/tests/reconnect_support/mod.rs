//! Shared by the reconnect tests: a recorder that is `Session` and `Resync` at once, short times, a waiting helper.
#![allow(dead_code)]

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

use fs_interfaces::common_capnp::blob;
use fs_rpc::{ReconnectTimes, Reconnected, RestartTimes, Resync, RpcError, Session};
use futures::future::LocalBoxFuture;

#[derive(Default)]
pub struct Recorder {
    pub attached: Cell<u32>,
    pub dropped: Rc<Cell<u32>>,
    pub events: RefCell<Vec<Reconnected>>,
    pub gave_up: RefCell<Vec<RpcError>>,
    pub ended: Rc<tokio::sync::Notify>,
}

pub struct Held(Rc<Cell<u32>>);
impl Drop for Held {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

impl Session<blob::Client> for Recorder {
    fn attach(&self, _client: blob::Client) -> LocalBoxFuture<'_, io::Result<Box<dyn Any>>> {
        self.attached.set(self.attached.get() + 1);
        let held: Box<dyn Any> = Box::new(Held(self.dropped.clone()));
        Box::pin(async move { Ok(held) })
    }

    fn ended(&self) -> LocalBoxFuture<'_, ()> {
        let ended = self.ended.clone();
        Box::pin(async move { ended.notified().await })
    }
}

impl Resync for Recorder {
    fn reconnected(&self, event: Reconnected) {
        self.events.borrow_mut().push(event);
    }
    fn gave_up(&self, error: &RpcError) {
        self.gave_up.borrow_mut().push(error.clone());
    }
}

pub fn quick(give_up_ms: u64) -> ReconnectTimes {
    let ms = Duration::from_millis;
    ReconnectTimes {
        attempt: ms(2000),
        backoff: vec![ms(30), ms(60), ms(100)],
        give_up: ms(give_up_ms),
        ping: ms(40),
        ping_limit: ms(500),
        handshake: RestartTimes {
            call: ms(500),
            stop: ms(2000),
            start: ms(2000),
        },
    }
}

pub async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let began = Instant::now();
    while !done() {
        assert!(began.elapsed() < Duration::from_secs(15), "{what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
