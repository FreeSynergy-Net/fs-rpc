//! A module held in the program itself: no socket, no second process; writing, undo and watch run as on a held connection.
//!
//! Pattern: Adapter (a local capability behind the same handle [`Hold`] as a socket connection) + Command (jobs, as
//! [`hold_calling`](super::hold_calling)). Deviation: nothing to reconnect, so no `HoldEvent`s and no `OnAttach`; whoever needs
//! `introduce` or `watch` runs it once as a [`Hold::call`] job.

use std::io;
use std::sync::mpsc;
use std::time::Duration;

use capnp::capability::Client;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::oneshot;

use super::Hold;
use super::hold_call::Call;

/// Let the closing of the capability reach the module (bounded: the owner of the handle waits for the thread).
const CLOSE_GRACE: Duration = Duration::from_millis(100);

/// Holds a module in this program: `make` runs on a NEW thread (inside its `LocalSet`) and returns the capability, usually
/// `Guarded::new(core, Ports::new(.., FixedPerson(person))).bootstrap(&Caller::new("in-process", ..)).hook` wrapped as a
/// `Client`; the `Guarded` is built there because it is `Rc`, so `!Send`. The jobs of [`Hold::call`] get that capability.
/// Dropping the handle ends the thread. Blocks until `make` has run.
///
/// # Errors
/// The thread did not start, or it ended before `make` returned.
pub fn hold_in_process(
    name: &str,
    make: impl FnOnce() -> Client + Send + 'static,
) -> io::Result<Hold> {
    let (stop, stopped) = oneshot::channel::<()>();
    let (sender, mut calls): (UnboundedSender<Call>, _) = unbounded_channel();
    let (ready, answer) = mpsc::channel::<io::Result<()>>();
    let thread = std::thread::Builder::new()
        .name(format!("fs-hold-{name}"))
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(e) => return drop(ready.send(Err(e))),
            };
            let local = tokio::task::LocalSet::new();
            local.block_on(&runtime, async {
                let client = make();
                let _ = ready.send(Ok(()));
                let mut stopped = stopped;
                loop {
                    tokio::select! {
                        _ = &mut stopped => break,
                        call = calls.recv() => match call {
                            Some(call) => { tokio::task::spawn_local(call(Some(client.clone()))); }
                            None => break,
                        },
                    }
                }
            });
            runtime.block_on(async {
                let _ = tokio::time::timeout(CLOSE_GRACE, local).await;
            });
        })?;
    let hold = Hold {
        stop: Some(stop),
        thread: Some(thread),
        calls: Some(sender),
    };
    answer
        .recv()
        .map_err(|_| io::Error::other("the hold thread ended"))??;
    Ok(hold)
}
