//! KB20 (security, c0): the OS user in the log comes from the CONNECTION (only the same OS user is let in, so it is the kernel's
//! answer for the peer), never from the environment. One test only in this binary: it changes the environment.
#![cfg(unix)]

mod support;

use std::sync::Arc;

use fs_settings::{InstanceStore, Raw};
use fs_settings_rpc::{RpcInstanceAdmin, SettingsService};
use fs_types::Utc;
use support::{Spy, dir, key, nobody, serve_on, times};

#[test]
fn kb20_os_user_is_the_peer_not_the_environment() {
    for var in ["USER", "LOGNAME", "USERNAME"] {
        // SAFETY: this binary has one test; nothing else reads the environment at the same time.
        unsafe { std::env::set_var(var, "mallory-from-environment") };
    }
    let d = dir();
    // One form: the name the system has for the user (the number when it has none), asked of `id`, not of the code under test.
    let id = |flag: &str| {
        let out = std::process::Command::new("id")
            .arg(flag)
            .output()
            .expect("id");
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).unwrap().trim().to_owned())
    };
    let expected = id("-un").unwrap_or_else(|| id("-u").expect("id -u"));
    let spy = Spy::new();
    let shared = spy.clone();
    let server = serve_on(&d.path().join("i.sock"), move || {
        let mut svc = SettingsService::new(shared.clone(), Some(shared));
        svc.now = Arc::new(|| Utc::from_millis(1));
        svc
    });
    let admin = RpcInstanceAdmin::open(&server.socket, "https://example.net/admin", times());
    admin
        .write_instance(
            &key("fs.look.style"),
            Some(&Raw::Str("calm".into())),
            &nobody(),
        )
        .unwrap();
    let seen = spy.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(
        seen[0].0.as_deref(),
        Some(expected.as_str()),
        "the kernel's peer user"
    );
    assert!(!seen[0].0.as_deref().unwrap().contains("mallory"));
}
