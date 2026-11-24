//! What a hold keeps and how: the module's socket, what it must say, how to start it again.
//!
//! Pattern: Builder (a value that grows by appending: more fields later without a break).

use std::path::PathBuf;

use super::{Expected, ReconnectTimes, ServerStarter};

/// Pattern: Builder - what is held and how: the module's socket, what it must say, how to start it again.
#[non_exhaustive]
pub struct HoldSpec {
    /// For the thread name `fs-hold-<name>` and error texts.
    pub name: String,
    /// The module's socket.
    pub socket: PathBuf,
    /// What the server must say about itself (`Expected::new("")` for any).
    pub expected: Expected,
    /// Starts the module again when it is gone; only the service of the module folder.
    pub starter: Box<dyn ServerStarter + Send>,
    /// The waits of connecting again (the defaults of [`keep_connected`]).
    pub times: ReconnectTimes,
}

impl HoldSpec {
    /// A hold of `socket` with the default times.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        socket: impl Into<PathBuf>,
        expected: Expected,
        starter: Box<dyn ServerStarter + Send>,
    ) -> Self {
        Self {
            name: name.into(),
            socket: socket.into(),
            expected,
            starter,
            times: ReconnectTimes::default(),
        }
    }

    /// The same with other times.
    #[must_use]
    pub fn with_times(mut self, times: ReconnectTimes) -> Self {
        self.times = times;
        self
    }
}
