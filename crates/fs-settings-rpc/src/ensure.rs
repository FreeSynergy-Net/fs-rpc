//! `ensure_service`: one settings service per person.
//!
//! Pattern: Facade over fs-rpc `ensure_single` (the same rule as the starter's `SingleGate`, starter_v1). The program file
//! `fs-settings` is looked for next to the running program, then in `/usr/bin` (never on `PATH`: a planted file in a
//! folder of the search path must not become the server). No lease (fs-rpc has none, starter O3): the service runs until
//! `Control.stop` or logout. A failure is `store-unreachable{socket, cause}`; the program goes on with the defaults.

use std::path::{Path, PathBuf};

use std::ffi::OsStr;

use fs_rpc::{CommandStarter, Expected, ServerStarter, SingleError, SingleTimes, ensure_single};
use fs_settings::{Coded, SettingCode, keys};

use crate::settings_capnp::settings_hub;

/// The file name of the service program.
pub const SERVICE_PROGRAM: &str = "fs-settings";

/// Where the service program is: next to `running` (the running program's own path), then `usr_bin` (`/usr/bin`); `None` if
/// there is no regular file in either. Never searches `PATH`.
#[must_use]
pub fn find_service_program(running: &Path, usr_bin: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "fs-settings.exe"
    } else {
        SERVICE_PROGRAM
    };
    let beside = running.parent().map(|d| d.join(name));
    [beside, Some(usr_bin.join(name))]
        .into_iter()
        .flatten()
        .find(|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()))
}

/// The cause token of a failed ensure, in `store-unreachable{cause}`: the fs-rpc code.
fn cause(e: &SingleError) -> &'static str {
    fs_rpc::RpcError::from(e).code().as_str()
}

/// Makes sure the person's settings service listens on `socket`: reaches the one that runs, or starts `fs-settings serve --socket
/// S` (the program file found beside `running`, then in `/usr/bin`; never on `PATH`).
///
/// # Errors
/// `store-unreachable{socket, cause}`; `cause` is `no-server` when there is no program file to start, else the fs-rpc code
/// (`cannot-restart`, `server-unresponsive`, ...).
pub fn ensure_service(
    socket: &Path,
    expected: &Expected,
    running: &Path,
    times: &SingleTimes,
) -> Result<(), Coded<SettingCode>> {
    let usr_bin = if cfg!(windows) {
        Path::new("")
    } else {
        Path::new("/usr/bin")
    };
    let Some(program) = find_service_program(running, usr_bin) else {
        return Err(crate::unreachable_at(socket).with_text(keys::CAUSE, "no-server"));
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

/// [`ensure_service`] with a starter and the program file of your own (`exe` is the file that counts as the service for taking
/// over a hanging one; `None`: the file of the running process).
///
/// # Errors
/// As for [`ensure_service`].
pub fn ensure_service_with(
    socket: &Path,
    expected: &Expected,
    starter: &dyn ServerStarter,
    exe: Option<&Path>,
    times: &SingleTimes,
) -> Result<(), Coded<SettingCode>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| crate::unreachable_at(socket).with_text(keys::CAUSE, "runtime"))?;
    let local = tokio::task::LocalSet::new();
    local
        .block_on(
            &runtime,
            ensure_single::<settings_hub::Client>(socket, expected, starter, times, exe),
        )
        .map(drop)
        .map_err(|e| crate::unreachable_at(socket).with_text(keys::CAUSE, cause(&e)))
}
