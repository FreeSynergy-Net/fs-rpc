//! Client <-> service over a real socket: KB10, KB11, KB14, KB15.

mod support;

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use fs_settings::{
    Health, InstanceStore, MemoryStore, Place, ProfileRef, Raw, Scope, SettingsStore,
};
use fs_settings_rpc::RpcSettingsStore;
use support::{code, create, dir, ids, key, person, program, text, times};

/// KB10: write, read, clear, index over the wire; the stamp rises; the cache is dropped by a new stamp.
#[test]
fn kb10_roundtrip_and_stamp() {
    let d = dir();
    let store = MemoryStore::new();
    let server = person(&d.path().join("s.sock"), &store, false);
    let c = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let (own, k) = (
        Scope::Program(program("tasks")),
        key("program.tasks.week-start"),
    );
    let s0 = c.stamp();
    assert_eq!(c.health(), Health::Ok);
    c.write(&own, Place::Profile, 0, &k, Some(&Raw::Int(3)))
        .unwrap();
    assert_ne!(c.stamp(), s0, "stamp rises");
    assert_eq!(
        c.profile(&own, Place::Profile, 0).values.get(&k),
        Some(&Raw::Int(3))
    );
    c.write(
        &Scope::General,
        Place::Device,
        0,
        &key("fs.look.style"),
        Some(&Raw::Str("calm".into())),
    )
    .unwrap();
    assert_eq!(
        c.profile(&Scope::General, Place::Device, 0)
            .values
            .get(&key("fs.look.style")),
        Some(&Raw::Str("calm".into()))
    );
    assert!(
        c.profile(&Scope::General, Place::Profile, 0)
            .values
            .is_empty(),
        "other place"
    );
    c.clear(&own, 0).unwrap();
    assert!(c.profile(&own, Place::Profile, 0).values.is_empty());
    let set = create(&c, "Arbeit").unwrap();
    assert_eq!(ids(&set), [("Arbeit".to_owned(), 1)]);
    assert_eq!(ids(&c.index().0), [("Arbeit".to_owned(), 1)]);
    // The cache holds what was read at one stamp: a second client changes, the first sees it after `stamp`.
    let other = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    assert!(c.profile(&own, Place::Profile, 0).values.is_empty());
    other
        .write(&own, Place::Profile, 0, &k, Some(&Raw::Int(5)))
        .unwrap();
    let before = c.stamp();
    assert_eq!(
        c.profile(&own, Place::Profile, 0).values.get(&k),
        Some(&Raw::Int(5)),
        "new stamp drops the cache"
    );
    assert_eq!(c.stamp(), before);
}

/// KB11: `open("tasks")` neither writes nor reads the scope of `people`; `manage()` does.
#[test]
fn kb11_program_is_bound_to_its_scopes() {
    let d = dir();
    let store = MemoryStore::new();
    let server = person(&d.path().join("s.sock"), &store, false);
    let tasks = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let people = Scope::Program(program("people"));
    let k = key("program.people.week-start");
    let e = tasks
        .write(&people, Place::Profile, 0, &k, Some(&Raw::Int(1)))
        .unwrap_err();
    assert_eq!(
        (code(&e), text(&e, "kind")),
        ("write-failed", Some("not-own"))
    );
    let snap = tasks.profile(&people, Place::Profile, 0);
    assert!(snap.values.is_empty());
    assert_eq!(
        snap.problems.iter().map(code).collect::<Vec<_>>(),
        ["write-failed"]
    );
    assert!(tasks.clear(&people, 0).is_err());
    assert!(
        store.profile(&people, Place::Profile, 0).values.is_empty(),
        "nothing written"
    );
    tasks
        .write(
            &Scope::General,
            Place::Profile,
            0,
            &key("fs.look.style"),
            Some(&Raw::Int(1)),
        )
        .unwrap();
    let all = RpcSettingsStore::manage(&server.socket, times());
    all.write(&people, Place::Profile, 0, &k, Some(&Raw::Int(2)))
        .unwrap();
    assert_eq!(
        all.profile(&people, Place::Profile, 0).values.get(&k),
        Some(&Raw::Int(2))
    );
}

/// KB14: a successor that reuses an id is refused with `profile{cause=id-invalid}`, the index is unchanged.
#[test]
fn kb14_reused_id_is_refused() {
    let d = dir();
    let store = MemoryStore::new();
    let server = person(&d.path().join("s.sock"), &store, false);
    let c = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    create(&c, "A").unwrap();
    let e = c
        .update_index(&mut |set| {
            let n = fs_settings::ProfileName::new("B").unwrap();
            set.create(n.clone(), None)?;
            *set.get_mut(&ProfileRef::Named(n)).unwrap() = 1;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(
        (code(&e), text(&e, "cause")),
        ("profile", Some("id-invalid"))
    );
    assert_eq!(ids(&c.index().0), [("A".to_owned(), 1)]);
    assert_eq!(ids(&store.index().0), [("A".to_owned(), 1)]);
}

/// KB15: a client that watches hears the writes of another one; dropping the subscription ends it.
#[test]
fn kb15_watch_hears_other_writers() {
    let d = dir();
    let store = MemoryStore::new();
    let server = person(&d.path().join("s.sock"), &store, false);
    let a = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let b = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let watching = b
        .watch(Arc::new(move |s| {
            let _ = tx.lock().unwrap().send(s);
        }))
        .unwrap();
    let k = key("fs.look.style");
    a.write(&Scope::General, Place::Profile, 0, &k, Some(&Raw::Int(1)))
        .unwrap();
    let heard = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("changed within the call time");
    assert_eq!(heard.0, store.stamp().0);
    drop(watching);
    std::thread::sleep(Duration::from_millis(200));
    while rx.try_recv().is_ok() {}
    a.write(&Scope::General, Place::Profile, 0, &k, Some(&Raw::Int(2)))
        .unwrap();
    assert!(
        rx.recv_timeout(Duration::from_millis(500)).is_err(),
        "no more after the subscription ended"
    );
}

/// A locked key is refused by the service (second guard), also for the person's program.
#[test]
fn locked_key_is_refused_over_the_wire() {
    let d = dir();
    let store = MemoryStore::new();
    let server = person(&d.path().join("s.sock"), &store, true);
    let k = key("fs.look.style");
    let who = support::nobody();
    store
        .write_lock(
            &k,
            &fs_settings::LockChange::Lock {
                value: Raw::Str("calm".into()),
                reason: "house".into(),
            },
            &who,
        )
        .unwrap();
    let c = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let e = c
        .write(
            &Scope::General,
            Place::Profile,
            0,
            &k,
            Some(&Raw::Str("x".into())),
        )
        .unwrap_err();
    assert_eq!(
        (code(&e), text(&e, "reason"), text(&e, "key")),
        ("locked", Some("house"), Some("fs.look.style"))
    );
    assert!(c.instance().locks.contains_key(&k));
}
