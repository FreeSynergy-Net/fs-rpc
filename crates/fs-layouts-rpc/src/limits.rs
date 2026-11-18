//! The limits of the service as SETTINGS (rule 57): what it takes in one call and how many watchers it keeps. The declared values
//! are the defaults of the settings `fs.forms.layout.page-max` and `fs.forms.layout.watchers-max` and `fs.forms.layout.watchers-per-actor-max` (not measured); the program that
//! hosts the service reads the person's values and the instance ceilings from fs-settings and passes them in, with the source of
//! each. Every refusal names the limit and where it came from (`by`), never a bare number. Only the 64 MiB of one message are fixed:
//! `fs_rpc::MAX_MESSAGE_BYTES` (fs-rpc `page.rs:15`) is the transport's own limit.
//!
//! Pattern: Value Object (settings snapshot), passed in, never read from a global.

use fs_forms_layout::LimitSource;

/// The effective limits of one service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServiceLimits {
    /// Largest `size` of a `list` or `log` call.
    pub page_max: u32,
    /// How many listeners (`watch`) the hub keeps at once, over ALL connections and sockets of the store.
    pub watchers_max: u32,
    /// How many of them one caller (`Caller::id`, from the system) may hold: a bot cannot take the places of the person.
    pub watchers_per_actor_max: u32,
    /// Where the values came from: the weakest source of the pair is not guessed, one source for both.
    pub by: LimitSource,
}

impl ServiceLimits {
    /// The declared defaults: page 200, 16 watchers in all, 8 per caller, source `default`.
    #[must_use]
    pub fn declared() -> Self {
        Self {
            page_max: 200,
            watchers_max: 16,
            watchers_per_actor_max: 8,
            by: LimitSource::Default,
        }
    }
}

impl Default for ServiceLimits {
    fn default() -> Self {
        Self::declared()
    }
}
