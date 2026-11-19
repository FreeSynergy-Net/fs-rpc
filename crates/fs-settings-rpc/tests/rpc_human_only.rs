//! K6-b: "only a person". A key declared `human_only` does not exist for a bot connection, in EVERY way the service offers (read, write,
//! clear, instance, admin write/lock, log), and a bot answer is the one of a key nobody declared. Who is a bot comes from the connection
//! (the port `ActorOfConnection`, fail closed by default), never from a field of the client. A log read that is cut and empty is
//! `history-cut`, and its repeats for the same pointer end in `history-stuck` (the livelock guard).

mod support;

use std::sync::Arc;

use fs_rpc::Caller;
use fs_settings::{
    Actor, Catalog, Declaration, GroupId, InstanceStore, Kind, LockChange, MemoryStore, Place, Raw,
    Scope, SettingKey, SettingsStore, Value,
};
use fs_settings_rpc::{
    ActorOfConnection, FailClosed, LIVELOCK_MAX, OsUserIsPerson, RpcInstanceAdmin,
    RpcSettingsStore, SettingsService,
};
use support::{Faked, Server, code, dir, key, nobody, serve_on, serve_per, text, times};
use tempfile::TempDir;

const HIDDEN: &str = "fs.mcp.enabled";
const OPEN: &str = "fs.look.compact";
const NOPE: &str = "fs.look.nothing";

fn catalog() -> Catalog {
    let decl = |k: &str, g: &str| {
        Declaration::new(
            key(k),
            Kind::Flag,
            Value::Flag(false),
            GroupId::new(g).unwrap(),
        )
    };
    Catalog::of(vec![decl(HIDDEN, "ai").human_only(), decl(OPEN, "look")]).unwrap()
}

type Svc = SettingsService<dyn SettingsStore, MemoryStore>;

fn service(s: &MemoryStore, person: bool) -> Svc {
    let port: Arc<dyn SettingsStore> = Arc::new(s.clone());
    let inst = Some(Arc::new(s.clone()));
    let mut svc = if person {
        SettingsService::new_os_user_is_person(port, inst)
    } else {
        SettingsService::new(port, inst)
    };
    svc.catalog = catalog();
    svc
}

struct World {
    _tmp: TempDir,
    store: MemoryStore,
    bot: Server,
    person: Server,
}

/// Both keys set in the person's profile, as instance defaults and in the log; the hidden one is locked.
fn world(locked: bool) -> World {
    let (tmp, store) = (dir(), MemoryStore::new());
    for k in [HIDDEN, OPEN] {
        let raw = Raw::Bool(true);
        store
            .write(&Scope::General, Place::Profile, 0, &key(k), Some(&raw))
            .unwrap();
        store
            .write_instance(&key(k), Some(&raw), &nobody())
            .unwrap();
    }
    if locked {
        let lock = LockChange::Lock {
            value: Raw::Bool(true),
            reason: "kept".to_owned(),
        };
        store.write_lock(&key(HIDDEN), &lock, &nobody()).unwrap();
    }
    let (b, p) = (store.clone(), store.clone());
    World {
        bot: serve_on(&tmp.path().join("b.sock"), move || service(&b, false)),
        person: serve_on(&tmp.path().join("p.sock"), move || service(&p, true)),
        store,
        _tmp: tmp,
    }
}

fn manage(s: &Server) -> RpcSettingsStore {
    RpcSettingsStore::manage(&s.socket, times())
}

fn admin(s: &Server) -> RpcInstanceAdmin {
    RpcInstanceAdmin::open(&s.socket, "claim", times())
}

fn keys_of(snap: &fs_settings::Snapshot) -> Vec<String> {
    snap.values.keys().map(|k| k.as_str().to_owned()).collect()
}

fn log_keys(a: &RpcInstanceAdmin) -> Vec<String> {
    a.instance_log(None, 50)
        .iter()
        .map(|e| e.key.as_str().to_owned())
        .collect()
}

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

/// K3/K4: every writing way, in turn; the answer is the one of a key nobody declared and nothing changed.
#[test]
fn every_writing_way_refuses_a_bot_like_an_undeclared_key_and_changes_nothing() {
    let w = world(false);
    let (m, a) = (manage(&w.bot), admin(&w.bot));
    let (hidden, nope) = (key(HIDDEN), key(NOPE));
    let raw = Raw::Bool(false);
    let writes: [(
        &str,
        &dyn Fn(&SettingKey) -> Result<(), fs_settings::Coded<fs_settings::SettingCode>>,
    ); 6] = [
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
    store
        .write(
            &Scope::General,
            Place::Profile,
            0,
            &key(OPEN),
            Some(&Raw::Bool(true)),
        )
        .unwrap();
    let s = store.clone();
    let server = serve_on(&tmp.path().join("d.sock"), move || {
        let port: Arc<dyn SettingsStore> = Arc::new(s);
        SettingsService::new(port, None::<Arc<MemoryStore>>)
    });
    assert!(
        manage(&server)
            .profile(&Scope::General, Place::Profile, 0)
            .values
            .is_empty()
    );
    assert_eq!(FailClosed.actor(&Caller::new("1000", None)), Actor::Bot);
    let own = OsUserIsPerson;
    assert_eq!(own.actor(&Caller::new("1000", None)), Actor::Person);
    for unusable in ["", "a/b", "x y"] {
        assert_eq!(
            own.actor(&Caller::new(unusable, None)),
            Actor::Bot,
            "{unusable:?}"
        );
    }
}

/// [AKTEUR-AUS-VERBINDUNG]: the actor comes from the caller of the connection; a `by` that claims to be the person changes nothing.
#[test]
fn the_actor_is_the_connections_never_the_claim_of_the_client() {
    let (tmp, store) = (dir(), MemoryStore::new());
    store
        .write_instance(&key(HIDDEN), Some(&Raw::Bool(true)), &nobody())
        .unwrap();
    store
        .write_instance(&key(OPEN), Some(&Raw::Bool(true)), &nobody())
        .unwrap();
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

/// The livelock guard: cut + empty page + the same pointer; the wire turns it into `history-cut`, the client counts, the last is `history-stuck`.
#[test]
fn a_cut_empty_page_for_the_same_pointer_ends_in_history_stuck() {
    let w = world(false);
    for i in 0..fs_settings::MAX_LINES_READ + 4 {
        w.store
            .write_instance(&key(HIDDEN), Some(&Raw::Bool(i % 2 == 1)), &nobody())
            .unwrap();
    }
    let person = admin(&w.person);
    // lines 0 and 1 are the two seeds (the second is the visible one); after it only hidden lines follow
    let lines = person.instance_log(None, 5);
    let (after_open, later) = (Some(lines[1].id), Some(lines[3].id));
    let bot = admin(&w.bot);
    assert!(
        bot.instance_log(after_open, 10).is_empty(),
        "the port form: an empty page, no loop"
    );
    let cut = |p| {
        let page = bot.instance_log_page(p, 10).unwrap();
        assert!(page.cut && page.lines.is_empty());
    };
    let stuck = |p| bot.instance_log_page(p, 10).unwrap_err();
    for _ in 1..LIVELOCK_MAX {
        cut(after_open);
    }
    let e = stuck(after_open);
    assert_eq!(
        (code(&e), text(&e, "cause")),
        ("store-broken", Some("history-stuck"))
    );
    // another pointer starts the count again
    for _ in 1..LIVELOCK_MAX {
        cut(later);
    }
    // so does an answer that is no cut (the visible seed line)
    assert_eq!(bot.instance_log_page(None, 10).unwrap().lines.len(), 1);
    for _ in 1..LIVELOCK_MAX {
        cut(later);
    }
    assert_eq!(text(&stuck(later), "cause"), Some("history-stuck"));
    let page = person.instance_log_page(None, 10).unwrap();
    assert!(
        !page.cut && page.lines.len() == 10,
        "a person reads the log whole"
    );
}

/// The texts of the two codes exist in German and English (`store-broken{cause}`, the code of the module).
#[test]
fn the_stuck_code_has_its_text_in_both_languages() {
    for texts in [fs_settings::TEXT_DE, fs_settings::TEXT_EN] {
        assert!(texts.contains("setting-error-store-broken"));
    }
}
