//! [`RpcSettingsStore`]: the struct and how it starts. The port methods are in `port`.
//!
//! Pattern: Proxy (see the module above).

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fs_settings::ProgramName;

use super::cache::Cache;
use super::worker::{ClientTimes, Role, Worker};

/// The client. Owns ONE thread with a current-thread runtime (capnp-rpc is `!Send`); every port call is a message to it and a
/// blocking wait: never call it from inside a tokio runtime (a debug build panics: "never from a runtime thread").
///
/// Equal stamps mean nothing to read again: the cache holds what was read at one stamp; a changed stamp (seen by
/// [`stamp`](fs_settings::SettingsStore::stamp) or by this client's own write) drops it.
pub struct RpcSettingsStore {
    pub(crate) worker: Worker,
    pub(crate) socket: PathBuf,
    pub(crate) cache: Mutex<Cache>,
}

impl RpcSettingsStore {
    /// Connects for `program` (`SettingsHub.open`). Does NOT fail when no service answers: the store starts in
    /// `Health::Defaults[store-unreachable{socket}]` with empty snapshots and connects later (PLAN 5.4 (2)).
    #[must_use]
    pub fn open(socket: &Path, program: &ProgramName, times: ClientTimes) -> Self {
        Self::with_role(socket, Role::Program(program.as_str().to_owned()), times)
    }

    /// The program fs-settings itself: every scope (`SettingsHub.manage`).
    #[must_use]
    pub fn manage(socket: &Path, times: ClientTimes) -> Self {
        Self::with_role(socket, Role::Manage, times)
    }

    pub(crate) fn with_role(socket: &Path, role: Role, times: ClientTimes) -> Self {
        Self {
            worker: Worker::start(socket, role, times),
            socket: socket.to_path_buf(),
            cache: Mutex::new(Cache::default()),
        }
    }
}
