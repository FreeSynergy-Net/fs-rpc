//! [AKTEUR-AUS-VERBINDUNG]: the actor of an admin call is `Caller::id` of the connection (named by the system, fs-rpc); a forged or empty
//! `by` is only a claim; one hub per connection, never the capability of another caller's.

mod support;

use std::sync::{Arc, Mutex};

use fs_rpc::{Caller, CallerError, PerConnection};
use fs_settings::{InstanceStore, Raw};
use fs_settings_rpc::{RpcInstanceAdmin, SettingsService};
use support::{Faked, Spy, dir, key, nobody, serve_on, serve_per, times};

const FORGED: &str = "https://example.org/people/forged";

fn write(admin: &RpcInstanceAdmin, n: i64) {
    admin
        .write_instance(&key("fs.look.style"), Some(&Raw::Int(n)), &nobody())
        .unwrap();
}

/// The caller the transport names is the actor; a forged and an empty `by` change nothing and every claim is handed to the host.
#[test]
fn forged_and_empty_by_do_not_change_the_actor() {
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let claims = Arc::new(Mutex::new(Vec::<String>::new()));
    let noted = claims.clone();
    let server = serve_per(&tmp.path().join("i.sock"), move || {
        let mut svc = SettingsService::new(shared.clone(), Some(shared));
        svc.claimed_by = Some(Arc::new(move |by: &str| {
            noted.lock().unwrap().push(by.to_owned());
        }));
        // a caller who is NOT the service's user, as a transport that admits others would name
        Faked {
            inner: svc.callers(),
            callers: vec![Caller::new("4242", Some("other".to_owned()))],
            seen: 0,
        }
    });
    for (n, claim) in [FORGED, "", "   "].into_iter().enumerate() {
        write(
            &RpcInstanceAdmin::open(&server.socket, claim, times()),
            i64::try_from(n).unwrap(),
        );
    }
    let seen = spy.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter()
            .all(|(user, _, by)| by == "urn:fs:os-user:4242" && user.as_deref() == Some("other")),
        "the actor is the caller's id, the user its name: {seen:?}"
    );
    let log = spy.inner.instance_log(None, 10);
    assert!(
        log.iter().all(|e| e.by == "urn:fs:os-user:4242"),
        "no claim reached the log as the actor"
    );
    assert_eq!(
        *claims.lock().unwrap(),
        [FORGED, "", "   "],
        "every claim is handed to the host, as written"
    );
}

/// One hub per connection: two callers on two connections get their own actor, also when the first one connected before.
#[test]
fn each_connection_gets_its_own_caller() {
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let server = serve_per(&tmp.path().join("i.sock"), move || Faked {
        inner: SettingsService::new(shared.clone(), Some(shared)).callers(),
        callers: vec![
            Caller::new("1001", Some("alice".to_owned())),
            Caller::new("1002", None),
        ],
        seen: 0,
    });
    let first = RpcInstanceAdmin::open(&server.socket, "x", times());
    write(&first, 1); // connects first: caller 1
    let second = RpcInstanceAdmin::open(&server.socket, "x", times());
    write(&second, 2); // connects second: caller 2
    write(&first, 3);
    let seen = spy.seen.lock().unwrap().clone();
    let who: Vec<(Option<&str>, &str)> = seen
        .iter()
        .map(|(u, _, by)| (u.as_deref(), by.as_str()))
        .collect();
    assert_eq!(
        who,
        [
            (Some("alice"), "urn:fs:os-user:1001"),
            (Some("1002"), "urn:fs:os-user:1002"), // no name: the number, one form
            (Some("alice"), "urn:fs:os-user:1001"),
        ]
    );
}

/// The actor comes from `Caller::id` only: the name is display, a name that looks like another id changes nothing.
#[test]
fn the_actor_is_the_id_not_the_name() {
    let caller = Caller::new("77", Some("urn:fs:os-user:1".to_owned()));
    assert_eq!(
        fs_types::BasicValue::canonical(&fs_settings_rpc::actor_of(&caller)),
        "urn:fs:os-user:77"
    );
    assert_eq!(
        fs_settings_rpc::os_user_of(&caller).as_deref(),
        Some("urn:fs:os-user:1")
    );
    assert_eq!(
        fs_settings_rpc::os_user_of(&Caller::new("77", None)).as_deref(),
        Some("77")
    );
    // an id that is no IRI part never becomes an actor of its own making
    let odd = fs_settings_rpc::actor_of(&Caller::new("a b/../c", None));
    assert_eq!(
        fs_types::BasicValue::canonical(&odd),
        "urn:fs:os-user:unknown"
    );
}

/// A connection the system could not name is reported (`caller-credentials`) and nothing is served: the hook of the service hears it.
#[test]
fn a_connection_without_a_caller_is_reported() {
    let spy = Spy::new();
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let noted = heard.clone();
    let mut svc = SettingsService::new(spy.clone(), Some(spy));
    svc.refused_note = Some(Arc::new(move |code: &str| {
        noted.lock().unwrap().push(code.to_owned());
    }));
    let mut callers = svc.callers();
    callers.refused(&CallerError::Credentials(std::io::Error::other(
        "no peer credentials",
    )));
    assert_eq!(*heard.lock().unwrap(), ["caller-credentials"]);
    assert!(fs_settings_rpc::refused_line("caller-credentials").contains("caller-credentials"));
}

/// Over the real transport the caller is the user the system names (`id`), as a NAME for the log and as the id for the actor; the
/// service as it comes from `new` logs a claim (the hook is there).
#[cfg(unix)]
#[test]
fn the_real_transport_names_the_user_and_the_default_hooks_are_there() {
    let id = |flag: &str| {
        let out = std::process::Command::new("id")
            .arg(flag)
            .output()
            .expect("id");
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).unwrap().trim().to_owned())
    };
    let (uid, name) = (id("-u").expect("id -u"), id("-un"));
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let server = serve_on(&tmp.path().join("i.sock"), move || {
        SettingsService::new(shared.clone(), Some(shared))
    });
    write(&RpcInstanceAdmin::open(&server.socket, FORGED, times()), 1);
    let seen = spy.seen.lock().unwrap().clone();
    assert_eq!(seen[0].2, format!("urn:fs:os-user:{uid}"));
    assert_eq!(
        seen[0].0.as_deref(),
        Some(name.unwrap_or(uid).as_str()),
        "one form: the name, the number when there is none"
    );
    let svc = SettingsService::new(spy.clone(), Some(spy));
    assert!(
        svc.claimed_by.is_some() && svc.refused_note.is_some(),
        "a service as it comes from `new` is not silent"
    );
    let line = fs_settings_rpc::claim_line(FORGED);
    assert!(line.contains(FORGED) && line.contains("ignored"), "{line}");
    assert!(
        fs_settings_rpc::claim_line("").contains("\"\""),
        "an empty claim is visible too"
    );
}

/// A claim at the PERSON's service (it has no administrator): the admin call is refused (`read-only{which=instance}`), the claim is
/// still handed to the host, and no actor is ever made of it (nothing reaches the instance port).
#[test]
fn a_claim_at_the_persons_service_is_reported_and_obeyed_by_nobody() {
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let claims = Arc::new(Mutex::new(Vec::<String>::new()));
    let noted = claims.clone();
    let server = serve_per(&tmp.path().join("p.sock"), move || {
        let mut svc = SettingsService::new(shared, None::<Arc<Spy>>);
        svc.claimed_by = Some(Arc::new(move |by: &str| {
            noted.lock().unwrap().push(by.to_owned());
        }));
        svc.callers()
    });
    let admin = RpcInstanceAdmin::open(&server.socket, FORGED, times());
    let refused = admin
        .write_instance(&key("fs.look.style"), Some(&Raw::Int(1)), &nobody())
        .unwrap_err();
    assert_eq!(support::code(&refused), "read-only");
    assert_eq!(support::text(&refused, "which"), Some("instance"));
    assert_eq!(
        *claims.lock().unwrap(),
        [FORGED],
        "the claim is reported although the call is refused"
    );
    assert!(
        spy.seen.lock().unwrap().is_empty(),
        "no write reached the port under any actor"
    );
}
