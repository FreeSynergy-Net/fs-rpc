//! The administrator over the wire: KB16 and the ensure rules KB19.

mod support;

use std::path::Path;
use std::sync::Arc;

use fs_rpc::{Expected, SingleTimes};
use fs_settings::{InstanceStore, LockChange, MemoryStore, Raw, SettingsStore};
use fs_settings_rpc::{
    RpcInstanceAdmin, RpcSettingsStore, SettingsService, ensure_service, find_service_program,
};
use fs_types::Utc;
use support::{Spy, code, dir, key, nobody, person, program, serve_on, text, times};

const ADMIN: &str = "https://example.net/people/admin";
const THEN: i64 = 1_700_000_000_123;

/// KB16: the person's service has no administrator; the instance service answers; time and OS user are the SERVICE's.
#[test]
#[allow(clippy::too_many_lines)] // one story: the instance service from first write to the locked refusal
fn kb16_admin_only_on_instance_service_time_and_user_are_the_services() {
    let tmp = dir();
    let store = MemoryStore::new();
    let person_only = person(&tmp.path().join("p.sock"), &store, false);
    let none = RpcInstanceAdmin::open(&person_only.socket, ADMIN, times());
    assert_eq!(
        code(
            &none
                .write_instance(&key("fs.look.style"), Some(&Raw::Int(1)), &nobody())
                .unwrap_err()
        ),
        "read-only"
    );

    let spy = Spy::new();
    let shared = spy.clone();
    let server = serve_on(&tmp.path().join("i.sock"), move || {
        let mut svc = SettingsService::new(shared.clone(), Some(shared));
        svc.now = Arc::new(|| Utc::from_millis(THEN));
        svc.os_user = Arc::new(|| Some("service-says".to_owned()));
        svc
    });
    let admin = RpcInstanceAdmin::open(&server.socket, ADMIN, times());
    let name = key("fs.look.style");
    let value = Raw::Str("calm".into());
    assert!(
        admin
            .write_instance(&name, Some(&value), &nobody())
            .unwrap()
    );
    assert!(
        !admin
            .write_instance(&name, Some(&value), &nobody())
            .unwrap(),
        "same value: no change"
    );
    let lock = |reason: &str| LockChange::Lock {
        value: Raw::Str("dark".into()),
        reason: reason.into(),
    };
    let e = admin.write_lock(&name, &lock("  "), &nobody()).unwrap_err();
    assert_eq!(
        (code(&e), text(&e, "key")),
        ("reason-missing", Some("fs.look.style"))
    );
    assert!(
        admin
            .write_lock(&name, &lock("house style"), &nobody())
            .unwrap()
    );
    let e = admin
        .write_lock(&name, &LockChange::Unlock { reason: " ".into() }, &nobody())
        .unwrap_err();
    assert_eq!(code(&e), "reason-missing");
    assert!(
        admin
            .write_lock(
                &name,
                &LockChange::Unlock {
                    reason: "done".into()
                },
                &nobody()
            )
            .unwrap()
    );
    let log = admin.instance_log(None, 10);
    assert_eq!(
        log.iter().map(|e| e.action).collect::<Vec<_>>(),
        ["set", "lock", "unlock"]
    );
    for e in &log {
        assert_eq!(
            e.at,
            Utc::from_millis(THEN),
            "the service's clock, not the caller's"
        );
        assert_eq!(e.by, ADMIN);
    }
    // The port saw the service's OS user and clock for every write, whatever the caller sent.
    let seen = spy.seen.lock().unwrap().clone();
    assert_eq!(
        seen.len(),
        3,
        "the refused writes never reached the port: {seen:?}"
    );
    for (user, at, by) in &seen {
        assert_eq!(user.as_deref(), Some("service-says"));
        assert_eq!(*at, Utc::from_millis(THEN));
        assert_eq!(by, ADMIN);
    }
    assert_eq!(log[2].reason.as_deref(), Some("done"));
    let rest = admin.instance_log(Some(log[0].id), 10);
    assert_eq!(rest.len(), 2, "reads on after an id");
    // The instance layer is readable over the wire, and a person's write on a locked key is refused.
    admin.write_lock(&name, &lock("again"), &nobody()).unwrap();
    let client = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    assert_eq!(
        client
            .instance()
            .locks
            .get(&name)
            .map(|l| l.reason.as_str()),
        Some("again")
    );
    assert_eq!(
        client.instance().locks[&name].locked_at,
        Utc::from_millis(THEN)
    );
    let e = client
        .write(
            &fs_settings::Scope::General,
            fs_settings::Place::Profile,
            0,
            &name,
            Some(&Raw::Int(1)),
        )
        .unwrap_err();
    assert_eq!(code(&e), "locked");
}

/// KB19: no program file: `store-unreachable{socket, cause=no-server}`, fast; the search never uses `PATH`.
#[test]
fn kb19_ensure_without_program_file() {
    let d = dir();
    let socket = d.path().join("s.sock");
    let t = std::time::Instant::now();
    let e = ensure_service(
        &socket,
        &Expected::new("0.0.1"),
        Path::new("/nonexistent/fs-tasks"),
        &SingleTimes::default(),
    )
    .unwrap_err();
    assert!(t.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(code(&e), "store-unreachable");
    assert_eq!(text(&e, "socket"), Some(socket.to_str().unwrap()));
    assert_eq!(text(&e, "cause"), Some("no-server"));
}

#[test]
fn kb19_program_file_is_found_beside_then_in_usr_bin_never_on_path() {
    let d = dir();
    let (beside, usr) = (d.path().join("app"), d.path().join("usr"));
    std::fs::create_dir_all(&beside).unwrap();
    std::fs::create_dir_all(&usr).unwrap();
    let name = if cfg!(windows) {
        "fs-settings.exe"
    } else {
        "fs-settings"
    };
    let running = beside.join("fs-tasks");
    assert_eq!(find_service_program(&running, &usr), None);
    std::fs::write(usr.join(name), b"x").unwrap();
    assert_eq!(find_service_program(&running, &usr), Some(usr.join(name)));
    std::fs::write(beside.join(name), b"x").unwrap();
    assert_eq!(
        find_service_program(&running, &usr),
        Some(beside.join(name)),
        "beside wins"
    );
    // a folder with that name is no program
    std::fs::remove_file(beside.join(name)).unwrap();
    std::fs::create_dir(beside.join(name)).unwrap();
    assert_eq!(find_service_program(&running, &usr), Some(usr.join(name)));
}
