//! `ensure_service`: one layouts service per person.
//!
//! Pattern: Facade over fs-rpc `ensure_single` (the same rule as the starter's `SingleGate`, as `fs-settings-rpc`). The program
//! file `fs-forms` is looked for next to the running program, then in `/usr/bin` (never on `PATH`: a planted file in a folder of
//! the search path must not become the server). A failure is `unreachable{socket, cause}`; the host shows that, never an empty
//! window without a reason.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use fs_forms_layout::{Coded, LayoutCode};
use fs_rpc::{CommandStarter, Expected, ServerStarter, SingleError, SingleTimes, ensure_single};

use crate::layouts_capnp::layouts;

/// The file name of the service program.
pub const SERVICE_PROGRAM: &str = "fs-forms";

/// Where the service program is: next to `running` (the running program's own path), then `usr_bin`; `None` if there is no regular
/// file in either. Never searches `PATH`.
#[must_use]
pub fn find_service_program(running: &Path, usr_bin: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "fs-forms.exe"
    } else {
        SERVICE_PROGRAM
    };
    let beside = running.parent().map(|d| d.join(name));
    [beside, Some(usr_bin.join(name))]
        .into_iter()
        .flatten()
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
}

/// `unreachable{socket}`.
#[must_use]
pub fn unreachable_at(socket: &Path) -> Coded<LayoutCode> {
    Coded::new(LayoutCode::Unreachable).with("socket", socket.display())
}

fn cause(e: &SingleError) -> &'static str {
    fs_rpc::RpcError::from(e).code().as_str()
}

/// Makes sure the person's layouts service listens on `socket`: reaches the one that runs, or starts `fs-forms serve --socket S`.
///
/// Call it before the program's own runtime exists: it builds a runtime of its own and panics inside an async context.
///
/// # Errors
/// `unreachable{socket, cause}`; `cause` is `no-server` when there is no program file to start, else the fs-rpc code.
pub fn ensure_service(
    socket: &Path,
    expected: &Expected,
    running: &Path,
    times: &SingleTimes,
) -> Result<(), Coded<LayoutCode>> {
    let usr_bin = if cfg!(windows) {
        Path::new("")
    } else {
        Path::new("/usr/bin")
    };
    let Some(program) = find_service_program(running, usr_bin) else {
        return Err(unreachable_at(socket).with("cause", "no-server"));
    };
    let starter = CommandStarter::new(
        &program,
        [
            OsStr::new("serve"),
            OsStr::new("--socket"),
            socket.as_os_str(),
        ],
    );
    ensure_service_with(socket, expected, &starter, Some(&program), times)
}

/// [`ensure_service`] with a starter and the program file of your own.
///
/// # Errors
/// As for [`ensure_service`].
pub fn ensure_service_with(
    socket: &Path,
    expected: &Expected,
    starter: &dyn ServerStarter,
    exe: Option<&Path>,
    times: &SingleTimes,
) -> Result<(), Coded<LayoutCode>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| unreachable_at(socket).with("cause", "runtime"))?;
    let local = tokio::task::LocalSet::new();
    local
        .block_on(
            &runtime,
            ensure_single::<layouts::Client>(socket, expected, starter, times, exe),
        )
        .map(drop)
        .map_err(|e| unreachable_at(socket).with("cause", cause(&e)))
}
