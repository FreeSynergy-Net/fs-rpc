//! A caller whose id the system gives unusably is REFUSED (code `caller-unknown`, a line in the host's log) and never served under the
//! name: the actor of such an id is `unknown`.

mod support;

use std::sync::{Arc, Mutex};

use fs_rpc::Caller;
use fs_settings::{InstanceStore, Raw};
use fs_settings_rpc::{RpcInstanceAdmin, SettingsService};
use support::{Faked, Spy, dir, key, nobody, serve_per, times};

fn write(admin: &RpcInstanceAdmin, n: i64) {
    admin
        .write_instance(&key("fs.look.style"), Some(&Raw::Int(n)), &nobody())
        .unwrap();
}

/// An empty or unusable id never makes an actor of its own and NEVER falls back to the name (a display name can be chosen by whoever set
/// up the account): the actor is `unknown`.
#[test]
fn an_unusable_id_is_unknown_never_the_name() {
    for id in ["", "a b", "x/y", "bad id\n", "ü"] {
        for name in [Some("alice"), Some("urn:fs:os-user:1"), None] {
            let caller = Caller::new(id, name.map(str::to_owned));
            assert!(!fs_settings_rpc::id_usable(id));
            assert_eq!(
                fs_types::BasicValue::canonical(&fs_settings_rpc::actor_of(&caller)),
                "urn:fs:os-user:unknown",
                "id {id:?} with name {name:?}"
            );
        }
    }
    assert!(fs_settings_rpc::id_usable("1002") && fs_settings_rpc::id_usable("S-1-5-21-1.2_3"));
}

/// A connection whose caller has no usable id is REFUSED: every call answers `caller-unknown` (a client sees `store-broken{cause=
/// caller-unknown}`), the host's log hears it, and nothing is ever written under `unknown` or under the name. A usable id works.
#[test]
fn an_unusable_id_is_refused_with_a_code_and_a_log_line() {
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let noted = heard.clone();
    let server = serve_per(&tmp.path().join("i.sock"), move || {
        let mut svc = SettingsService::new(shared.clone(), Some(shared));
        svc.refused_note = Some(Arc::new(move |code: &str| {
            noted.lock().unwrap().push(code.to_owned());
        }));
        Faked {
            inner: svc.callers(),
            callers: vec![
                Caller::new("", Some("alice".to_owned())),
                Caller::new("a b/c", Some("bob".to_owned())),
                Caller::new("1002", Some("carol".to_owned())),
            ],
            seen: 0,
        }
    });
    for _ in 0..2 {
        let admin = RpcInstanceAdmin::open(&server.socket, "x", times());
        let refused = admin
            .write_instance(&key("fs.look.style"), Some(&Raw::Int(1)), &nobody())
            .unwrap_err();
        assert_eq!(support::code(&refused), "store-broken");
        assert_eq!(support::text(&refused, "cause"), Some("caller-unknown"));
    }
    write(&RpcInstanceAdmin::open(&server.socket, "x", times()), 3);
    let seen = spy.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1, "only the usable caller wrote: {seen:?}");
    assert_eq!(
        (seen[0].0.as_deref(), seen[0].2.as_str()),
        (Some("carol"), "urn:fs:os-user:1002")
    );
    assert!(
        seen.iter().all(|(_, _, by)| !by.contains("unknown")
            && !by.contains("alice")
            && !by.contains("bob"))
    );
    assert_eq!(
        *heard.lock().unwrap(),
        ["caller-unknown", "caller-unknown"],
        "exactly the refused ones, in the log hook"
    );
    let line = fs_settings_rpc::refused_line("caller-unknown");
    assert!(line.contains("caller-unknown"), "{line}");
    assert!(
        SettingsService::new(spy.clone(), Some(spy))
            .refused_note
            .is_some(),
        "a service as it comes from `new` logs a refusal by default"
    );
}

/// Every method of a refused hub answers the code: a client cannot find one that works (`instance()` included, which the port client
/// would swallow into an empty answer).
#[test]
fn every_method_of_a_refused_hub_answers_the_code() {
    use fs_settings_rpc::settings_capnp::settings_hub;
    let tmp = dir();
    let spy = Spy::new();
    let shared = spy.clone();
    let server = serve_per(&tmp.path().join("i.sock"), move || Faked {
        inner: SettingsService::new(shared.clone(), Some(shared)).callers(),
        callers: vec![Caller::new("", Some("alice".to_owned()))],
        seen: 0,
    });
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let local = tokio::task::LocalSet::new();
    let types = local.block_on(&rt, async {
        let conn = fs_rpc::connect::<settings_hub::Client>(&server.socket)
            .await
            .unwrap();
        let hub = conn.client.clone();
        let mut open = hub.open_request();
        open.get().set_program("tasks");
        let mut admin = hub.admin_request();
        admin.get().set_by("x");
        let errors = [
            open.send().promise.await.err(),
            hub.manage_request().send().promise.await.err(),
            admin.send().promise.await.err(),
            hub.instance_request().send().promise.await.err(),
        ];
        errors.map(|e| {
            e.and_then(|e| fs_rpc::Problem::from_error(&e))
                .map(|p| p.problem_type)
        })
    });
    let want = Some("setting-error-caller-unknown".to_owned());
    assert_eq!(types, [want.clone(), want.clone(), want.clone(), want]);
}
