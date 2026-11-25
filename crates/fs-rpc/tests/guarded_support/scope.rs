//! The spy of the notes module for scope and feed loss (Stufe 4 V2): what `resolve_scope` and `follows` were asked, the break sinks,
//! the order of the registrations, and which record a conflict names.
//!
//! Pattern: Test double (Spy + Stub) - split from `mod.rs` (300-line rule).
#![allow(dead_code, clippy::result_large_err)] // `Refusal` is the contract's own value

use std::cell::RefCell;

use fs_interfaces::module::{FeedBreak, FeedLoss, Refusal};

/// The scopes of the notes module: the ids of its first notes (an alias resolves to an id, anything else is unknown).
const KNOWN: [&str; 3] = ["note-1", "note-2", "note-3"];
/// A name that stands for the id `note-2` (like a board name for a board id).
pub const ALIAS: &str = "second";

#[derive(Default)]
pub struct Spy {
    /// Every scope `resolve_scope` was asked (as the caller wrote it).
    pub resolved: RefCell<Vec<String>>,
    /// Every scope `follows` was asked (it must be the id `resolve_scope` returned).
    pub followed: RefCell<Vec<String>>,
    /// The break sinks the wrapper registered.
    pub breaks: RefCell<Vec<FeedBreak>>,
    /// `break` and `feed`, in the order the wrapper registered them.
    pub calls: RefCell<Vec<&'static str>>,
    /// The record a conflict names in `FieldClash::record`: `None` = the record of the call; `Some("")` = none named.
    pub clash_record: RefCell<Option<String>>,
}

impl Spy {
    /// Empty = all; the alias = `note-2`; a known id = itself; anything else = `unknown-scope` (404).
    pub fn resolve(&self, scope: &str) -> Result<String, Refusal> {
        self.resolved.borrow_mut().push(scope.to_owned());
        match scope {
            "" => Ok(String::new()),
            ALIAS => Ok("note-2".into()),
            s if KNOWN.contains(&s) => Ok(s.into()),
            s => Err(Refusal::unknown_scope(s)),
        }
    }

    /// The module loses its feed: every registered break sink hears it.
    pub fn lose(&self, loss: FeedLoss) {
        for sink in self.breaks.borrow().iter() {
            sink(loss);
        }
    }

    /// The record a conflict at `apply` of `record` names.
    pub fn record_named(&self, record: &str) -> String {
        self.clash_record
            .borrow()
            .clone()
            .unwrap_or_else(|| record.to_owned())
    }
}
