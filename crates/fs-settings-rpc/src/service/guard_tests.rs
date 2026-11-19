//! The guard on its own: the filter of problems and snapshots, the clear of a profile, the read limit of the log.
//!
//! Pattern: none, tests only (every server way run as a bot and as a person against the guard of `guard.rs`).

use std::sync::Arc;

use fs_settings::{
    Actor, Catalog, Coded, Declaration, GroupId, InstanceStore, Kind, MemoryStore, Place, Raw,
    Scope, SettingCode, SettingKey, SettingsStore, Snapshot, Stampd, Value, keys,
};
use fs_types::coded::Param;
use fs_types::{BasicValue, Reference, Utc};

use super::guard::Guard;

fn key(t: &str) -> SettingKey {
    SettingKey::parse(t).unwrap()
}

fn guard() -> Guard {
    let decl = |k: &str, g: &str| {
        Declaration::new(
            key(k),
            Kind::Flag,
            Value::Flag(false),
            GroupId::new(g).unwrap(),
        )
    };
    Guard::new(
        Catalog::of(vec![
            decl("fs.mcp.enabled", "ai").human_only(),
            decl("fs.look.compact", "look"),
        ])
        .unwrap(),
    )
}

fn problem(k: &str, line: u64) -> Coded<SettingCode> {
    Coded::new(SettingCode::WriteFailed)
        .with_text(keys::KEY, k)
        .with_number(keys::LINE, line)
}

fn has(p: &Coded<SettingCode>, name: &str) -> bool {
    p.params().any(|(k, _)| k == name)
}

#[test]
fn problems_of_a_hidden_key_and_every_line_number_are_left_out_for_a_bot() {
    let list = vec![
        problem("fs.mcp.enabled", 3),
        problem("fs.look.compact", 4),
        Coded::new(SettingCode::UnknownKey).with_text(keys::KEY, "fs.look.nothing"),
    ];
    let bot = guard().problems(Actor::Bot, list.clone());
    assert_eq!(
        bot.len(),
        2,
        "the problem naming the hidden key is dropped: {bot:?}"
    );
    assert!(
        bot.iter().all(|p| !has(p, keys::LINE)),
        "no line number: {bot:?}"
    );
    let named = |p: &Coded<SettingCode>| {
        p.params().find_map(|(k, v)| match v {
            Param::Text(t) if k == keys::KEY => Some(t.clone()),
            _ => None,
        })
    };
    assert_eq!(named(&bot[0]).as_deref(), Some("fs.look.compact"));
    assert_eq!(
        guard().problems(Actor::Person, list.clone()),
        list,
        "a person gets all of it"
    );
}

#[test]
fn a_snapshot_for_a_bot_holds_only_declared_visible_keys() {
    let mut snap = Snapshot::default();
    for k in ["fs.mcp.enabled", "fs.look.compact", "fs.look.nothing"] {
        snap.values.insert(key(k), Raw::Bool(true));
    }
    let bot: Vec<_> = guard()
        .snapshot(Actor::Bot, snap.clone())
        .values
        .into_keys()
        .collect();
    assert_eq!(
        bot,
        [key("fs.look.compact")],
        "an undeclared key is no key for a bot (fail closed)"
    );
    assert_eq!(guard().snapshot(Actor::Person, snap.clone()), snap);
}

#[test]
fn a_clear_for_a_bot_removes_what_it_sees_and_keeps_the_hidden_key() {
    let store = MemoryStore::new();
    let (hidden, open) = (key("fs.mcp.enabled"), key("fs.look.compact"));
    for place in [Place::Profile, Place::Device] {
        for k in [&hidden, &open] {
            store
                .write(&Scope::General, place, 0, k, Some(&Raw::Bool(true)))
                .unwrap();
        }
    }
    guard()
        .clear(Actor::Bot, &store, &Scope::General, 0)
        .unwrap();
    for place in [Place::Profile, Place::Device] {
        let left: Vec<_> = store
            .profile(&Scope::General, place, 0)
            .values
            .into_keys()
            .collect();
        assert_eq!(left, std::slice::from_ref(&hidden), "{place:?}");
    }
}

/// The read limit: more hidden lines than one call reads, then a page that is empty AND cut (the service turns it into `history-cut`).
#[test]
fn a_log_of_only_hidden_lines_is_cut_and_empty_for_a_bot_and_whole_for_a_person() {
    let store = Arc::new(MemoryStore::new());
    let who = Stampd {
        by: Reference::parse("urn:fs:os-user:1").unwrap(),
        at: Utc::from_millis(1),
    };
    for i in 0..fs_settings::MAX_LINES_READ + 4 {
        store
            .write_instance(&key("fs.mcp.enabled"), Some(&Raw::Bool(i % 2 == 0)), &who)
            .unwrap();
    }
    let g = guard();
    let bot = g.log(Actor::Bot, &store, None, 10);
    assert!(bot.lines.is_empty() && bot.cut, "cut and empty");
    let person = g.log(Actor::Person, &store, None, 10);
    assert_eq!(person.lines.len(), 10);
    assert!(!person.cut);
}
