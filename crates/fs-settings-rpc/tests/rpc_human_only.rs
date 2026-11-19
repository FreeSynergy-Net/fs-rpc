//! K6-b: "only a person". A key declared `human_only` does not exist for a bot connection, in EVERY way the service offers (read, write,
//! clear, instance, admin write/lock, log), and a bot answer is the one of a key nobody declared. Who is a bot comes from the connection
//! (the port `ActorOfConnection`, fail closed by default), never from a field of the client. (The livelock guard of the log: `rpc_history_stuck.rs`.)

mod support;

use std::sync::Arc;

use fs_rpc::Caller;
use fs_settings::{
    Actor, InstanceStore, LockChange, MemoryStore, Place, Raw, Scope, SettingKey, SettingsStore,
};
use fs_settings_rpc::{
    ActorOfConnection, FailClosed, OsUserIsPerson, RpcInstanceAdmin, SettingsService,
};
use support::{
    Faked, HIDDEN, NOPE, OPEN, Server, admin, code, dir, key, keys_of, log_keys, manage, nobody,
    serve_on, serve_per, service, text, times, world,
};

/// K3/K4: every reading way, in turn, as a bot and as a person.
#[test]
fn every_reading_way_hides_the_key_from_a_bot_and_shows_it_to_a_person() {
    let w = world(true);
    let seen = |s: &Server| {
        let m = manage(s);
        let snap = m.profile(&Scope::General, Place::Profile, 0);
        let inst = m.instance();
        let mut instance_keys = keys_of(&inst.values);
        instance_keys.extend(inst.locks.keys().map(|k| k.as_str().to_owned()));
        (keys_of(&snap), instance_keys, log_keys(&admin(s)))
    };
    let (profile, instance, log) = seen(&w.bot);
    assert_eq!(profile, [OPEN], "profile read");
    assert_eq!(instance, [OPEN], "instance values and locks");
    assert!(
        !log.is_empty() && log.iter().all(|k| k == OPEN),
        "log: {log:?}"
    );
    let (profile, instance, log) = seen(&w.person);
    assert_eq!(profile, [OPEN, HIDDEN]);
    assert!(
        instance.iter().filter(|k| *k == HIDDEN).count() == 2,
        "value and lock: {instance:?}"
    );
    assert!(
        log.iter().any(|k| k == HIDDEN),
        "the person reads the log of it"
    );
}

/// One way to change a key, as the client offers it.
type Way<'a> = dyn Fn(&SettingKey) -> Result<(), fs_settings::Coded<fs_settings::SettingCode>> + 'a;

/// K3/K4: every writing way, in turn; the answer is the one of a key nobody declared and nothing changed.
#[test]
fn every_writing_way_refuses_a_bot_like_an_undeclared_key_and_changes_nothing() {
    let w = world(false);
    let (m, a) = (manage(&w.bot), admin(&w.bot));
    let (hidden, nope) = (key(HIDDEN), key(NOPE));
    let raw = Raw::Bool(false);
    let writes: [(&str, &Way<'_>); 6] = [
        ("write", &|k| {
            m.write(&Scope::General, Place::Profile, 0, k, Some(&raw))
        }),
        ("remove", &|k| {
            m.write(&Scope::General, Place::Profile, 0, k, None)
        }),
        ("set", &|k| {
            a.write_instance(k, Some(&raw), &nobody()).map(drop)
        }),
        ("unset", &|k| a.write_instance(k, None, &nobody()).map(drop)),
        ("lock", &|k| {
            let change = LockChange::Lock {
                value: raw.clone(),
                reason: "x".to_owned(),
            };
            a.write_lock(k, &change, &nobody()).map(drop)
        }),
        ("unlock", &|k| {
            let change = LockChange::Unlock {
                reason: "x".to_owned(),
            };
            a.write_lock(k, &change, &nobody()).map(drop)
        }),
    ];
    for (way, f) in writes {
        let (e, same) = (f(&hidden).unwrap_err(), f(&nope).unwrap_err());
        assert_eq!(code(&e), "not-declared", "{way}");
        assert_eq!(code(&same), "not-declared", "{way}");
        assert_eq!(text(&e, "key"), Some(HIDDEN), "{way}");
    }
    let snap = w.store.profile(&Scope::General, Place::Profile, 0);
    assert_eq!(snap.values.get(&hidden), Some(&Raw::Bool(true)));
    assert_eq!(
        w.store.instance().values.values.get(&hidden),
        Some(&Raw::Bool(true))
    );
    // the person changes it
    let p = manage(&w.person);
    p.write(&Scope::General, Place::Profile, 0, &hidden, Some(&raw))
        .unwrap();
    assert_eq!(
        w.store
            .profile(&Scope::General, Place::Profile, 0)
            .values
            .get(&hidden),
        Some(&raw)
    );
}

/// K3/K4: `clear` of a profile for a bot removes what it sees and leaves the hidden key; for a person all of it.
#[test]
fn a_clear_by_a_bot_leaves_the_hidden_key_and_a_person_clears_all() {
    let w = world(false);
    manage(&w.bot).clear(&Scope::General, 0).unwrap();
    let left = w.store.profile(&Scope::General, Place::Profile, 0);
    assert_eq!(keys_of(&left), [HIDDEN]);
    manage(&w.person).clear(&Scope::General, 0).unwrap();
    assert!(
        w.store
            .profile(&Scope::General, Place::Profile, 0)
            .values
            .is_empty()
    );
}

/// Fail closed: the default service has no person, and without a catalog a bot sees no key at all.
#[test]
fn the_default_is_fail_closed_and_a_missing_catalog_shows_a_bot_nothing() {
    let (tmp, store) = (dir(), MemoryStore::new());
    let raw = Raw::Bool(true);
    store
        .write(&Scope::General, Place::Profile, 0, &key(OPEN), Some(&raw))
        .unwrap();
    let s = store.clone();
    let server = serve_on(&tmp.path().join("d.sock"), move || {
        let port: Arc<dyn SettingsStore> = Arc::new(s);
        SettingsService::new(port, None::<Arc<MemoryStore>>)
    });
    let general = manage(&server).profile(&Scope::General, Place::Profile, 0);
    assert!(general.values.is_empty());
    assert_eq!(FailClosed.actor(&Caller::new("1000", None)), Actor::Bot);
    let own = OsUserIsPerson;
    assert_eq!(own.actor(&Caller::new("1000", None)), Actor::Person);
    for unusable in ["", "a/b", "x y"] {
        let who = own.actor(&Caller::new(unusable, None));
        assert_eq!(who, Actor::Bot, "{unusable:?}");
    }
}

/// [AKTEUR-AUS-VERBINDUNG]: the actor comes from the caller of the connection; a `by` that claims to be the person changes nothing.
#[test]
fn the_actor_is_the_connections_never_the_claim_of_the_client() {
    let (tmp, store) = (dir(), MemoryStore::new());
    for k in [HIDDEN, OPEN] {
        let raw = Raw::Bool(true);
        store
            .write_instance(&key(k), Some(&raw), &nobody())
            .unwrap();
    }
    let s = store.clone();
    let server = serve_per(&tmp.path().join("c.sock"), move || {
        let mut svc = service(&s, false);
        svc.actors = Arc::new(|c: &Caller| {
            if c.id() == "1001" {
                Actor::Person
            } else {
                Actor::Bot
            }
        });
        Faked {
            inner: svc.callers(),
            callers: vec![Caller::new("4242", None), Caller::new("1001", None)],
            seen: 0,
        }
    });
    let claim = "urn:fs:os-user:1001";
    let first = RpcInstanceAdmin::open(&server.socket, claim, times());
    assert_eq!(
        log_keys(&first),
        [OPEN],
        "first connection: bot, whatever it claims"
    );
    let second = RpcInstanceAdmin::open(&server.socket, "", times());
    assert_eq!(
        log_keys(&second),
        [HIDDEN, OPEN],
        "second connection: the person"
    );
}
