//! KB20 (security, c0): the OS user in the log comes from the CONNECTION (only the same OS user is let in, so it is the kernel's
//! answer for the peer), never from the environment. One test only in this binary: it changes the environment.
#![cfg(unix)]

mod support;

use std::os::unix::fs::MetadataExt;
use std::sync::Arc;

use fs_settings::{InstanceStore, MemoryStore, Raw, SettingsStore, Stampd};
use fs_settings_rpc::{RpcInstanceAdmin, SettingsService};
use fs_types::Utc;
use support::{dir, key, serve_on, times};

#[test]
fn kb20_os_user_is_the_peer_not_the_environment() {
    for var in ["USER", "LOGNAME", "USERNAME"] {
        // SAFETY: this binary has one test; nothing else reads the environment at the same time.
        unsafe { std::env::set_var(var, "mallory-from-environment") };
    }
    let d = dir();
    let uid = std::fs::metadata(d.path()).unwrap().uid().to_string();
    let store = MemoryStore::new();
    let s = store.clone();
    let server = serve_on(&d.path().join("i.sock"), move || {
        let mut svc = SettingsService::new(
            Arc::new(s.clone()) as Arc<dyn SettingsStore>,
            Some(Arc::new(s)),
        );
        svc.now = Arc::new(|| Utc::from_millis(1));
        svc
    });
    let admin = RpcInstanceAdmin::open(&server.socket, "https://example.net/admin", times());
    let who = Stampd {
        by: "x".into(),
        at: Utc::from_millis(0),
        os_user: Some("caller-claims".into()),
    };
    admin
        .write_instance(&key("fs.look.style"), Some(&Raw::Str("calm".into())), &who)
        .unwrap();
    let log = admin.instance_log(None, 5);
    assert_eq!(log.len(), 1);
    assert_eq!(
        log[0].os_user.as_deref(),
        Some(uid.as_str()),
        "the kernel's peer user"
    );
    assert!(!log[0].os_user.as_deref().unwrap().contains("mallory"));
}
