//! The world of the tests of `SettingsCore`: a catalog, a `Guarded` over it, the clients of a person and of a bot, the calls.
//!
//! Pattern: Test double helpers - in-process, no socket; the capability of a connection is called directly.
#![allow(dead_code, clippy::cast_possible_truncation)]

mod fixture;

use std::future::Future;
use std::sync::Arc;

use capnp::capability::FromClientHook;
use fs_interfaces::formsource_capnp::form_source;
use fs_interfaces::module::{Actor, ModuleCore};
use fs_interfaces::owner::fake::person;
use fs_rpc::{Caller, Guarded, PerConnection, Ports};
use fs_settings::{
    Catalog, Declaration, GroupId, InstanceStore, Kind, LockChange, MemoryStore, ProgramName, Raw,
    SettingKey, Stampd, Value,
};
use fs_settings_rpc::SettingsCore;

#[allow(unused_imports)] // not every test file uses the fixture
pub use fixture::SettingsFixture;

pub const FORM: &str = "fields-in-groups@1.0.0";
pub const WEEK: &str = "program.tasks.week-start";
pub const DENSITY: &str = "program.tasks.density";
pub const AI: &str = "program.tasks.ai-switch";
pub const PINNED: &str = "program.tasks.pinned";
const PERSON: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e01";

pub fn run<F: Future>(f: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, f)
}

pub fn key(text: &str) -> SettingKey {
    SettingKey::parse(text).unwrap()
}

/// Four settings: a number, a choice, a switch only a person may touch, and one the admin pins.
pub fn catalog() -> Catalog {
    let d = |k: &str, kind, default, group: &str| {
        Declaration::new(key(k), kind, default, GroupId::new(group).unwrap())
    };
    Catalog::of(vec![
        d(
            WEEK,
            Kind::Whole { min: 1, max: 7 },
            Value::Whole(1),
            "calendar",
        ),
        d(
            DENSITY,
            Kind::Choice {
                keys: vec!["compact".into(), "roomy".into()],
            },
            Value::Choice("compact".into()),
            "look",
        ),
        d(AI, Kind::Flag, Value::Flag(false), "privacy").human_only(),
        d(PINNED, Kind::Flag, Value::Flag(false), "privacy"),
    ])
    .unwrap()
}

/// The admin pins `PINNED` to `true` (a lock: a state of the data).
pub fn lock_pinned(store: &MemoryStore) {
    let who = Stampd {
        by: fs_types::BasicValue::parse("https://example.org/people/boss").unwrap(),
        at: fs_types::Utc::from_millis(1_700_000_000_123),
    };
    let lock = LockChange::Lock {
        value: Raw::Bool(true),
        reason: "house rule".into(),
    };
    store.write_lock(&key(PINNED), &lock, &who).unwrap();
}

pub fn core(store: &MemoryStore) -> SettingsCore<MemoryStore> {
    let program = ProgramName::new("tasks").unwrap();
    SettingsCore::new(program, catalog(), Arc::new(store.clone()))
}

/// The ports: the system id `bot` is a bot, everybody else a person (the actor comes from the connection, never from a field).
pub fn ports() -> Ports {
    Ports::new(
        |c: &Caller| {
            if c.id() == "bot" {
                Actor::Bot
            } else {
                Actor::Person
            }
        },
        |_: &Caller| Some(person(PERSON)),
    )
}

pub fn connect<M: ModuleCore>(g: &mut Guarded<M>, who: &str) -> form_source::Client {
    FromClientHook::new(g.bootstrap(&Caller::new(who, None)).hook)
}

/// What a write answered.
#[derive(Debug)]
pub struct Wrote {
    pub failed: bool,
    pub kind: String,
    pub status: u16,
    pub record: String,
}

pub async fn invoke(
    c: &form_source::Client,
    cmd: &str,
    record: &str,
    list: &[(&str, &str)],
) -> Wrote {
    let mut req = c.invoke_request();
    req.get().set_command(cmd);
    req.get().set_record(record);
    let mut out = req.get().init_args(list.len() as u32);
    for (i, (p, t)) in list.iter().enumerate() {
        let mut e = out.reborrow().get(i as u32);
        e.set_path(p);
        e.set_text(t);
    }
    let r = req.send().promise.await.unwrap();
    let a = r.get().unwrap();
    let text = |t: capnp::Result<capnp::text::Reader<'_>>| t.unwrap().to_string().unwrap();
    Wrote {
        failed: a.get_failed(),
        kind: text(a.get_problem().unwrap().get_type()),
        status: a.get_problem().unwrap().get_status(),
        record: text(a.get_result().unwrap().get_record()),
    }
}

/// The rows of the form as (id, value of the field `value`).
pub async fn rows(c: &form_source::Client) -> Vec<(String, String)> {
    let mut req = c.page_request();
    req.get().set_form(FORM);
    req.get().set_limit(100);
    let r = req.send().promise.await.unwrap();
    let page = r.get().unwrap().get_page().unwrap();
    let text = |t: capnp::Result<capnp::text::Reader<'_>>| t.unwrap().to_string().unwrap();
    page.get_records()
        .unwrap()
        .iter()
        .map(|rec| {
            let value = rec
                .get_values()
                .unwrap()
                .iter()
                .find(|v| text(v.get_path()) == "value")
                .map(|v| text(v.get_text()))
                .unwrap_or_default();
            (text(rec.get_id()), value)
        })
        .collect()
}
