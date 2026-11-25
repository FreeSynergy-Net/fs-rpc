//! The rules of the notes module of the tests (owners, a cutoff, setters; every ask is recorded).
//!
//! Pattern: Test double (Spy + Fake) - split from `mod.rs` (300-line rule).
#![allow(dead_code, clippy::collapsible_if)]

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use fs_interfaces::module::{Ask, Context, HiddenReason, RightsRules, Verdict};
use fs_types::{PersonRef, Utc};

use super::{CREATE, SET};

/// One question the rules were asked.
#[derive(Clone, Debug)]
pub struct Asked {
    pub command: String,
    pub record: Option<String>,
    pub at: Utc,
    pub person: PersonRef,
    pub claimed_program: String,
}

#[derive(Clone, Default)]
pub struct Rules {
    pub owners: Arc<Mutex<HashMap<String, PersonRef>>>,
    pub asked: Arc<Mutex<Vec<Asked>>>,
    /// A rule "the right ends": a change at or after this time is refused (preparation of user 66).
    pub cutoff: Arc<Mutex<Option<Utc>>>,
    pub revert_ok: Arc<AtomicBool>,
    /// Persons who may run `note.set` on any record although they do not own it (a right to change, not to see).
    pub setters: Arc<Mutex<Vec<PersonRef>>>,
}

impl RightsRules for Rules {
    fn may(&self, ask: &Ask<'_>) -> Verdict {
        self.asked.lock().unwrap().push(Asked {
            command: ask.command.into(),
            record: ask.record.map(Into::into),
            at: ask.at,
            person: ask.who.person.clone(),
            claimed_program: ask.who.claimed.program.clone(),
        });
        if let Some(cutoff) = *self.cutoff.lock().unwrap() {
            if ask.at >= cutoff {
                return Verdict::deny("right-ended");
            }
        }
        match ask.command {
            CREATE => Verdict::Allow,
            SET if self.setters.lock().unwrap().contains(&ask.who.person) => Verdict::Allow,
            "change.revert" if self.revert_ok.load(Ordering::SeqCst) => Verdict::Allow,
            "change.revert" => Verdict::deny("no-revert"),
            _ => match ask
                .record
                .and_then(|r| self.owners.lock().unwrap().get(r).cloned())
            {
                Some(owner) if owner == ask.who.person => Verdict::Allow,
                Some(_) => Verdict::deny("not-owner"),
                None => Verdict::deny("unknown-record"),
            },
        }
    }

    fn hidden(&self, who: &Context, record: &str, field: &str) -> Option<HiddenReason> {
        // No record named: nothing is known to be hidden (the wrapper must not rely on this: it blanks the field itself).
        if record.is_empty() {
            return None;
        }
        let owners = self.owners.lock().unwrap();
        (field == "secret" && owners.get(record) != Some(&who.person))
            .then_some(HiddenReason::NoRight)
    }
}
