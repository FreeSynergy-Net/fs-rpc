//! The heartbeat's channel on Windows: the pipe `<pipe>.beat`, answered by a thread of its own with a `current_thread` runtime of
//! its own (so a blocked work thread, or the work thread's runtime, never silences it). Instances are made by the same
//! `create_pipe` as the work pipe (open to this user alone, the first-instance rule); a client that is not this user is dropped
//! without a word. A `save` runs on the blocking pool of the heartbeat's runtime, so a slow hook does not block a ping.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::NamedPipeServer;
use tokio::sync::Notify;

use super::server::Shared;
use crate::transport::beat::{self, Admit};

/// Holds the thread: dropping it stops the thread, and the pipe goes with its last instance.
pub(super) struct Guard {
    stop: Arc<Notify>,
    thread: Option<JoinHandle<()>>,
}

pub(super) fn open(socket: &std::path::Path, shared: &Arc<Shared>) -> io::Result<(PathBuf, Guard)> {
    open_with(socket, shared, beat::CLIENT_IS_ME)
}

/// [`open`] with the check of the client as an argument (the test of the refusal gives a check that says no).
pub(super) fn open_with(
    socket: &std::path::Path,
    shared: &Arc<Shared>,
    admit: Admit,
) -> io::Result<(PathBuf, Guard)> {
    let name = beat::pipe_name(socket)?;
    let stop = Arc::new(Notify::new());
    let (started, told) = mpsc::channel::<io::Result<()>>();
    let thread = {
        let (name, shared, stop) = (name.clone(), shared.clone(), stop.clone());
        std::thread::Builder::new()
            .name("fs-heartbeat".into())
            .spawn(move || run(&name, &shared, &stop, admit, &started))?
    };
    match told.recv() {
        Ok(Ok(())) => Ok((
            PathBuf::from(name),
            Guard {
                stop,
                thread: Some(thread),
            },
        )),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            Err(io::Error::other("the heartbeat thread ended at its start"))
        }
    }
}

fn run(
    name: &str,
    shared: &Arc<Shared>,
    stop: &Arc<Notify>,
    admit: Admit,
    started: &mpsc::Sender<io::Result<()>>,
) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = started.send(Err(e));
            return;
        }
    };
    runtime.block_on(async {
        // Made inside the runtime: a pipe instance registers with the reactor of the thread that makes it.
        let first = match beat::first_instance(name) {
            Ok(first) => first,
            Err(e) => {
                let _ = started.send(Err(e));
                return;
            }
        };
        let _ = started.send(Ok(()));
        serve(name, first, shared, stop, admit).await;
    });
}

/// The next instance, tried again until it exists; `None` if the heartbeat is stopped meanwhile.
async fn next_instance(name: &str, stop: &Notify) -> Option<NamedPipeServer> {
    loop {
        if let Ok(instance) = beat::next_instance(name) {
            return Some(instance);
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(50)) => {}
            () = stop.notified() => return None,
        }
    }
}

async fn serve(
    name: &str,
    first: NamedPipeServer,
    shared: &Arc<Shared>,
    stop: &Notify,
    admit: Admit,
) {
    let mut instance = first;
    loop {
        let connected = tokio::select! {
            r = instance.connect() => r,
            () = stop.notified() => return,
        };
        // The next instance exists before this one is handed over, so the name is never without one for long.
        let Some(next) = next_instance(name, stop).await else {
            return;
        };
        let served = std::mem::replace(&mut instance, next);
        if connected.is_err() {
            continue; // the caller hung up before it was taken: this instance is spent
        }
        if beat::client_admitted(&served, admit) {
            tokio::spawn(answer(served, shared.clone()));
        }
        // A client that is not this user: the instance is dropped here, with no answer and no work done for it.
    }
}

async fn answer(mut stream: NamedPipeServer, shared: Arc<Shared>) {
    let mut line = String::new();
    let read = tokio::time::timeout(Duration::from_secs(2), async {
        BufReader::new((&mut stream).take(64))
            .read_line(&mut line)
            .await
    })
    .await;
    if !matches!(read, Ok(Ok(_))) {
        return;
    }
    let reply = if line.trim() == "save" {
        // The hook may take long: not on the thread that must answer pings.
        tokio::task::spawn_blocking(move || shared.answer("save"))
            .await
            .ok()
            .flatten()
    } else {
        shared.answer(&line)
    };
    if let Some(reply) = reply {
        let _ = stream.write_all(format!("{reply}\n").as_bytes()).await;
        let _ = stream.flush().await;
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "server_windows_tests.rs"]
mod tests;
