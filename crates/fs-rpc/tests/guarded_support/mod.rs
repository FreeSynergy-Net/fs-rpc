//! A recording notes module for the tests of `Guarded`: owners, a hidden field, revisions, a conflict, a change feed.
//!
//! Pattern: Test double (Fake + Spy) - it records every ask of its rules and every context its `apply` saw.
#![allow(
    dead_code,
    clippy::field_reassign_with_default,
    clippy::unused_async_trait_impl,
    clippy::unnecessary_literal_bound,
    clippy::collapsible_if,
    clippy::struct_field_names,
    clippy::assigning_clones
)]

pub mod client;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use fs_interfaces::module::{
    Applied, Ask, Blob, ChangeFeed, ChangeKind, ChangePage, ChangeRef, ChangesQuery, Conflict,
    Context, FieldClash, HiddenReason, Invocation, ModuleCore, Origin, Page, PageQuery, RecordView,
    Refusal, RightsRules, StepRef, Value, Verdict,
};
use fs_types::{BasicValue, PersonRef, Utc};

pub const CREATE: &str = "note.create";
pub const SET: &str = "note.set";

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
        let owners = self.owners.lock().unwrap();
        (field == "secret" && owners.get(record) != Some(&who.person))
            .then_some(HiddenReason::NoRight)
    }
}

struct Note {
    text: String,
    secret: String,
    rev: u32,
}

pub struct Notes {
    pub rules: Rules,
    notes: RefCell<HashMap<String, Note>>,
    log: RefCell<Vec<ChangeRef>>,
    sinks: RefCell<Vec<ChangeFeed>>,
    /// How often `apply` ran, and the contexts it saw.
    pub applied: RefCell<Vec<Context>>,
}

impl Notes {
    pub fn new(rules: Rules) -> Self {
        Self {
            rules,
            notes: RefCell::default(),
            log: RefCell::default(),
            sinks: RefCell::default(),
            applied: RefCell::default(),
        }
    }

    fn change(&self, cx: &Context, object: &str, rev: u32, kind: ChangeKind) -> ChangeRef {
        let n = self.log.borrow().len() + 1;
        let id = format!("00000000-0000-7000-8000-{n:012}");
        let origin = Origin::new(
            &cx.person.id.canonical(),
            &cx.claimed.program,
            &cx.claimed.instance,
            &cx.claimed.device,
        );
        let before = if rev > 1 {
            (rev - 1).to_string()
        } else {
            String::new()
        };
        ChangeRef::new(
            &id,
            object,
            &rev.to_string(),
            &before,
            origin,
            kind,
            Utc::from_millis(1),
        )
    }

    /// Tell every sink; no borrow of the module's own state is held while they run (the contract of `ChangeFeed`).
    fn announce(&self, change: &ChangeRef) {
        for sink in self.sinks.borrow().iter() {
            sink(change);
        }
    }
}

fn value<'a>(inv: &'a Invocation, path: &str) -> Option<&'a str> {
    inv.args
        .iter()
        .find(|v| v.path == path)
        .map(|v| v.text.as_str())
}

impl ModuleCore for Notes {
    fn level(&self) -> u32 {
        2
    }
    fn provider(&self) -> &str {
        "test.notes"
    }
    fn rules(&self) -> &dyn RightsRules {
        &self.rules
    }

    async fn page(&self, _: &Context, _: PageQuery) -> Result<Page, Refusal> {
        let notes = self.notes.borrow();
        let mut ids: Vec<&String> = notes.keys().collect();
        ids.sort();
        let mut page = Page::default();
        page.may = vec![CREATE.into(), SET.into()];
        page.records = ids
            .into_iter()
            .map(|id| {
                let n = &notes[id];
                let mut r = RecordView::default();
                r.id = id.clone();
                r.may = vec![SET.into()];
                r.values = vec![Value::new("text", &n.text), Value::new("secret", &n.secret)];
                r
            })
            .collect();
        Ok(page)
    }

    async fn blob(&self, _: &Context, _: &str, _: &str) -> Result<Blob, Refusal> {
        let mut b = Blob::default();
        b.media_type = "text/plain".into();
        b.bytes = b"scribble".to_vec();
        Ok(b)
    }

    async fn apply(&self, cx: &Context, cmd: Invocation) -> Result<Applied, Refusal> {
        self.applied.borrow_mut().push(cx.clone());
        let (id, rev, kind) = if cmd.command == CREATE {
            let id = format!("note-{}", self.notes.borrow().len() + 1);
            self.rules
                .owners
                .lock()
                .unwrap()
                .insert(id.clone(), cx.person.clone());
            self.notes.borrow_mut().insert(
                id.clone(),
                Note {
                    text: value(&cmd, "text").unwrap_or_default().into(),
                    secret: value(&cmd, "secret").unwrap_or_default().into(),
                    rev: 1,
                },
            );
            (id, 1, ChangeKind::Create)
        } else {
            let id = cmd.record.clone().unwrap_or_default();
            let mut notes = self.notes.borrow_mut();
            let Some(note) = notes.get_mut(&id) else {
                let mut r = Refusal::default();
                r.kind = "unknown-record".into();
                r.status = 404;
                return Err(r);
            };
            if let Some(expect) = &cmd.expect {
                if *expect != note.rev.to_string() {
                    let by = ChangeRef::new(
                        "00000000-0000-7000-8000-0000000000ff",
                        &id,
                        &note.rev.to_string(),
                        expect,
                        Origin::new("urn:uuid:other", "other", "", ""),
                        ChangeKind::Update,
                        Utc::from_millis(1),
                    );
                    let clash = FieldClash::new(
                        "text",
                        &note.text,
                        value(&cmd, "text").unwrap_or_default(),
                    );
                    return Err(Refusal::conflict(
                        "changed-since",
                        Conflict::new(by, vec![clash]),
                    ));
                }
            }
            note.text = value(&cmd, "text").unwrap_or_default().into();
            note.rev += 1;
            (id, note.rev, ChangeKind::Update)
        };
        let change = self.change(cx, &id, rev, kind);
        self.log.borrow_mut().push(change.clone());
        self.announce(&change);
        let mut applied = Applied::default();
        applied.record = id;
        applied.change = Some(change);
        Ok(applied)
    }

    async fn revert(&self, cx: &Context, change: &str, _: StepRef) -> Result<Applied, Refusal> {
        let mut applied = Applied::default();
        applied.text = format!("reverted {change} for {}", cx.claimed.program);
        Ok(applied)
    }

    async fn changes(&self, cx: &Context, q: ChangesQuery) -> Result<ChangePage, Refusal> {
        let mut page = ChangePage::default();
        page.changes = self
            .log
            .borrow()
            .iter()
            .filter(|c| q.run.is_empty() && c.origin.program == cx.claimed.program)
            .cloned()
            .collect();
        Ok(page)
    }

    fn feed(&self, sink: ChangeFeed) {
        self.sinks.borrow_mut().push(sink);
    }
}
