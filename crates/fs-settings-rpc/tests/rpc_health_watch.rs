//! The health the service says (N13) and the watchers of administrator writes (N17).

mod support;

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use fs_settings::{Health, InstanceStore, LockChange, MemoryStore, Raw, SettingsStore};
use fs_settings_rpc::{RpcInstanceAdmin, RpcSettingsStore, SettingsService};
use support::{Degraded, Spy, degraded_codes, dir, key, nobody, program, serve_on, times};

/// A port in emergency mode: the client sees `Defaults` with the same codes and parameters, never a silent `Ok`.
#[test]
fn a_port_in_emergency_mode_is_seen_as_defaults() {
    let tmp = dir();
    let server = serve_on(&tmp.path().join("s.sock"), || {
        let port = Arc::new(Degraded {
            inner: MemoryStore::new(),
        });
        SettingsService::new(port, None::<Arc<MemoryStore>>)
    });
    let client = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    assert_eq!(client.health(), Health::Defaults(degraded_codes()));
}

/// An administrator's write (`set`, `lock`, `unlock`) is told to the watchers like a person's.
#[test]
fn admin_writes_reach_the_watchers() {
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let server = serve_on(&tmp.path().join("i.sock"), move || {
        SettingsService::new(shared.clone(), Some(shared))
    });
    let watcher = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let _watching = watcher
        .watch(Arc::new(move |s| {
            let _ = tx.lock().unwrap().send(s);
        }))
        .unwrap();
    let admin = RpcInstanceAdmin::open(&server.socket, "https://example.net/admin", times());
    let name = key("fs.look.style");
    admin
        .write_instance(&name, Some(&Raw::Str("calm".into())), &nobody())
        .unwrap();
    let after_set = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("`set` is told to the watchers");
    assert_eq!(after_set.0, spy.stamp().0);
    let lock = LockChange::Lock {
        value: Raw::Str("dark".into()),
        reason: "house".into(),
    };
    admin.write_lock(&name, &lock, &nobody()).unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5)).is_ok(),
        "`lock` is told to the watchers"
    );
    admin
        .write_lock(
            &name,
            &LockChange::Unlock {
                reason: "done".into(),
            },
            &nobody(),
        )
        .unwrap();
    assert!(
        rx.recv_timeout(Duration::from_secs(5)).is_ok(),
        "`unlock` is told to the watchers"
    );
    // a write that changes nothing tells nobody
    admin
        .write_instance(&name, Some(&Raw::Str("dark".into())), &nobody())
        .unwrap();
    while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
    admin
        .write_instance(&name, Some(&Raw::Str("dark".into())), &nobody())
        .unwrap();
    assert!(
        rx.recv_timeout(Duration::from_millis(500)).is_err(),
        "same value: no notice"
    );
}
