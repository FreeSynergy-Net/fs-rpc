//! Who a server is and how it serves, in one value: the identity (`Control.version`) and the [`ServeOptions`] (limits, end after a time
//! without a connection). It is what `run_until_signal_serving` takes, so a module service gets its idle end through the signal way too.
//!
//! Pattern: Builder (the value grows by appending: a later stage adds what a service says it serves, nothing existing changes).

use std::time::Duration;

use super::control::ServerIdentity;
use super::serve_idle::ServeOptions;

/// The time a module service waits without a connection before it ends (one minute). A module takes it unless it says otherwise.
pub const MODULE_IDLE: Duration = Duration::from_secs(60);

/// Pattern: Builder - who the service is and how it serves, in one value that grows by appending.
///
/// `Serving::new(identity)` serves exactly as `run_until_signal_callers` did: no end but the signal.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Serving {
    /// What the service says about itself through `Control.version`.
    pub identity: ServerIdentity,
    /// Limits and the optional end after a time without a connection.
    pub options: ServeOptions,
}

impl Serving {
    /// A service with this identity and the default options (no idle end).
    #[must_use]
    pub fn new(identity: ServerIdentity) -> Self {
        Self {
            identity,
            options: ServeOptions::default(),
        }
    }

    /// The service ends (`Ok(())`, socket removed) after `idle` without a connection.
    #[must_use]
    pub fn with_idle_timeout(mut self, idle: Duration) -> Self {
        self.options = self.options.with_idle_timeout(idle);
        self
    }
}
