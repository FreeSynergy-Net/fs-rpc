//! [AKTEUR-AUS-VERBINDUNG]: the actor of an admin call is the identity of the connection; a forged or empty `by` is only a claim.

mod support;

use std::sync::{Arc, Mutex};

use fs_settings::{InstanceStore, Raw};
use fs_settings_rpc::{RpcInstanceAdmin, SettingsService, name_in_passwd};
use support::{Spy, dir, key, nobody, reference, serve_on, times};

const FORGED: &str = "https://example.org/people/forged";

/// A forged and an empty `by` do not change the actor; the claims are recorded for the host's log.
#[test]
fn forged_and_empty_by_do_not_change_the_actor() {
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let claims = Arc::new(Mutex::new(Vec::<String>::new()));
    let noted = claims.clone();
    let server = serve_on(&tmp.path().join("i.sock"), move || {
        let mut svc = SettingsService::new(shared.clone(), Some(shared));
        svc.actor_of_connection = Arc::new(|| reference("urn:fs:os-user:conn"));
        svc.claimed_by = Some(Arc::new(move |by: &str| {
            noted.lock().unwrap().push(by.to_owned());
        }));
        svc
    });
    for (n, claim) in [FORGED, "", "   "].into_iter().enumerate() {
        let admin = RpcInstanceAdmin::open(&server.socket, claim, times());
        admin
            .write_instance(
                &key("fs.look.style"),
                Some(&Raw::Int(i64::try_from(n).unwrap())),
                &nobody(),
            )
            .unwrap();
    }
    let seen = spy.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter().all(|(_, _, by)| by == "urn:fs:os-user:conn"),
        "the actor is the connection's: {seen:?}"
    );
    let log = spy.inner.instance_log(None, 10);
    assert!(
        log.iter().all(|e| e.by == "urn:fs:os-user:conn"),
        "no claim reached the log as the actor"
    );
    assert_eq!(
        *claims.lock().unwrap(),
        [FORGED, "", "   "],
        "every claim is handed to the host, as written"
    );
}

/// Without any injection the actor is `urn:fs:os-user:<name>` of the connection's OS user, whatever the client claims.
#[test]
fn default_actor_is_the_os_user_of_the_connection() {
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let server = serve_on(&tmp.path().join("i.sock"), move || {
        SettingsService::new(shared.clone(), Some(shared))
    });
    let admin = RpcInstanceAdmin::open(&server.socket, FORGED, times());
    admin
        .write_instance(&key("fs.look.style"), Some(&Raw::Int(1)), &nobody())
        .unwrap();
    let seen = spy.seen.lock().unwrap().clone();
    let name = fs_settings_rpc::os_user_name();
    assert_eq!(seen[0].2, format!("urn:fs:os-user:{name}"));
    assert_ne!(seen[0].2, FORGED);
}

#[test]
fn name_in_passwd_finds_the_name_or_nothing() {
    let text = "root:x:0:0:root:/root:/bin/bash\nagents:x:1002:1002::/home/agents:/bin/bash\nbroken\n:x:7:7\n";
    assert_eq!(name_in_passwd(text, 0).as_deref(), Some("root"));
    assert_eq!(name_in_passwd(text, 1002).as_deref(), Some("agents"));
    assert_eq!(name_in_passwd(text, 42), None);
    assert_eq!(
        name_in_passwd(text, 7),
        None,
        "a line without a name names nobody"
    );
}
