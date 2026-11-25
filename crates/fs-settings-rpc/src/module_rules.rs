//! The rights of the settings module: a key declared `human_only` exists only for a person.
//!
//! Pattern: Strategy - `SettingsRules` is the `RightsRules` of ONE data kind (settings); the wrapper `fs_rpc::Guarded` asks it before
//! every read and every change, the module (`SettingsCore`) never checks a right itself. Fail closed: everything that is not a
//! person (`Actor::Bot` and every actor added later) is restricted.

use std::collections::BTreeSet;

use fs_interfaces::module::{Actor, Ask, Context, HiddenReason, RightsRules, Verdict};
use fs_rpc::{READ_COMMAND, WATCH_COMMAND};
use fs_settings::Catalog;

/// The prefix of the commands of this module (`value.set`, `value.reset`).
pub(crate) const VALUE_PREFIX: &str = "value.";
/// The reason code of a refusal by these rules (a code, never a sentence).
pub const HUMAN_ONLY: &str = "human-only";

/// Pattern: Strategy (`RightsRules`) - the rights of the settings of one catalog.
///
/// `human_only` key and an actor that is not a person: `Deny` for `value.*` and for `form.read` (the record is struck out, it does not
/// exist for that actor); `change.watch` is asked like `form.read`; every other question is `Allow` (a command the module does not
/// know ends in `unknown-command`, not in a right).
#[derive(Clone, Debug, Default)]
pub struct SettingsRules {
    human_only: BTreeSet<String>,
}

impl SettingsRules {
    /// The rules of `catalog`: the keys it declares `human_only`.
    #[must_use]
    pub fn of(catalog: &Catalog) -> Self {
        Self {
            human_only: catalog
                .iter()
                .filter(|d| d.human_only)
                .map(|d| d.key.as_str().to_owned())
                .collect(),
        }
    }

    fn restricted(who: &Context, record: &str) -> bool {
        // `!=`, never `== Bot`: an actor added later is restricted too.
        who.actor != Actor::Person && !record.is_empty()
    }

    fn guards(command: &str) -> bool {
        command.starts_with(VALUE_PREFIX) || command == READ_COMMAND || command == WATCH_COMMAND
    }
}

impl RightsRules for SettingsRules {
    fn may(&self, ask: &Ask<'_>) -> Verdict {
        let record = ask.record.unwrap_or_default();
        if Self::guards(ask.command)
            && self.human_only.contains(record)
            && Self::restricted(ask.who, record)
        {
            return Verdict::deny(HUMAN_ONLY);
        }
        Verdict::Allow
    }

    fn hidden(&self, who: &Context, record: &str, _field: &str) -> Option<HiddenReason> {
        (self.human_only.contains(record) && Self::restricted(who, record))
            .then_some(HiddenReason::NoRight)
    }
}
