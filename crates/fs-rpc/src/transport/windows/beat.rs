//! The pipe of the heartbeat: `<pipe>.beat`, its instances and its client, for [`crate::Heartbeat`] and [`crate::ask_beat`].
//! Pattern: Facade over the one place that makes pipe instances (`acl::create_pipe`: open to this user alone, the first-instance
//! rule decides the owner) and the one place that looks at the process on the other end (`sid`), so the heartbeat has no rights
//! code of its own.

use std::io;
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::path::Path;
use std::time::Duration;

use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer};

use super::{ACCESS_DENIED, NOT_FOUND_TRIES, NOT_FOUND_WAIT, PIPE_BUSY, acl, my_pipe_name, sid};

/// A check of the process on the other end of a pipe instance: `true` if it runs as this user.
pub(crate) type Admit = fn(RawHandle) -> bool;

/// The server's check of its client.
pub(crate) const CLIENT_IS_ME: Admit = sid::client_is_me;
/// The client's check of its server.
pub(crate) const SERVER_IS_ME: Admit = sid::server_is_me;

/// The pipe of the heartbeat of the server at `socket`: its own name, so a ping never meets the work thread's pipe.
pub(crate) fn pipe_name(socket: &Path) -> io::Result<String> {
    Ok(format!("{}.beat", my_pipe_name(socket)?))
}

/// The first instance of the heartbeat pipe: whoever makes it owns the name.
///
/// # Errors
/// A live heartbeat owns the name (`AddrInUse`), or the system refuses.
pub(crate) fn first_instance(name: &str) -> io::Result<NamedPipeServer> {
    acl::create_pipe(name, true).map_err(|e| match e.raw_os_error() {
        Some(ACCESS_DENIED | PIPE_BUSY) => {
            io::Error::new(io::ErrorKind::AddrInUse, "a heartbeat answers already")
        }
        _ => e,
    })
}

/// Another instance of the pipe `name`, for the next caller.
pub(crate) fn next_instance(name: &str) -> io::Result<NamedPipeServer> {
    acl::create_pipe(name, false)
}

/// Open the heartbeat pipe of the server at `socket` and look at the process behind it with `server_ok`.
///
/// # Errors
/// `NotFound` (no heartbeat), `PermissionDenied` (the server is not this user's), or what the system says. A pipe that is
/// busy is tried again until the caller's own limit ends the wait.
pub(crate) async fn connect(socket: &Path, server_ok: Admit) -> io::Result<NamedPipeClient> {
    let name = pipe_name(socket)?;
    let mut missing = 0;
    let client = loop {
        match ClientOptions::new().open(&name) {
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound && missing < NOT_FOUND_TRIES => {
                missing += 1;
                tokio::time::sleep(NOT_FOUND_WAIT).await;
            }
            other => break other?,
        }
    };
    if !server_ok(client.as_raw_handle()) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the heartbeat is served by another user",
        ));
    }
    Ok(client)
}

/// Does `instance` have a client of this user? (`admit` is the check; the heartbeat passes [`CLIENT_IS_ME`].)
pub(crate) fn client_admitted(instance: &NamedPipeServer, admit: Admit) -> bool {
    admit(instance.as_raw_handle())
}
