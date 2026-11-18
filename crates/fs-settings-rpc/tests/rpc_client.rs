//! The client without a service, the compare-and-set, the runtime guard: KB12, KB13, KB17.

mod support;

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use fs_settings::{Health, MemoryStore, Place, Raw, Scope, SettingsStore};
use fs_settings_rpc::{RpcSettingsStore, SettingsService};
use support::{Flaky, code, create, dir, ids, key, person, program, serve_on, text, times};

/// KB12: no service: `open` returns, health says why, reads are empty, writes are refused; when the service starts, the stamp moves
/// and the health is `Ok`.
#[test]
fn kb12_starts_without_service_and_connects_later() {
    let d = dir();
    let socket = d.path().join("s.sock");
    let c = RpcSettingsStore::open(&socket, &program("tasks"), times());
    let s0 = c.stamp();
    let Health::Defaults(list) = c.health() else {
        panic!("without a service the health must say so");
    };
    assert_eq!(
        list.iter().map(code).collect::<Vec<_>>(),
        ["store-unreachable"]
    );
    assert_eq!(text(&list[0], "socket"), Some(socket.to_str().unwrap()));
    assert_eq!(c.stamp(), c.stamp(), "stable while down");
    assert!(
        c.profile(&Scope::General, Place::Profile, 0)
            .values
            .is_empty()
    );
    let e = c
        .write(
            &Scope::General,
            Place::Profile,
            0,
            &key("fs.look.style"),
            Some(&Raw::Int(1)),
        )
        .unwrap_err();
    assert_eq!(code(&e), "store-unreachable");
    let store = MemoryStore::new();
    let _server = person(&socket, &store, false);
    std::thread::sleep(Duration::from_millis(120));
    assert_ne!(c.stamp(), s0, "the health change is visible in the stamp");
    assert_eq!(c.health(), Health::Ok);
    c.write(
        &Scope::General,
        Place::Profile,
        0,
        &key("fs.look.style"),
        Some(&Raw::Int(1)),
    )
    .unwrap();
}

fn with_watchdog<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || drop(tx.send(f())));
    rx.recv_timeout(Duration::from_secs(20))
        .expect("did not end: endless retry")
}

/// KB13: two clients create "Arbeit" and "Vortrag" at the same time: both exist, ids 1 and 2.
#[test]
fn kb13_concurrent_creation_keeps_both() {
    let d = dir();
    let store = MemoryStore::new();
    let server = person(&d.path().join("s.sock"), &store, false);
    let socket = server.socket.clone();
    let make = move |name: &'static str| {
        let socket = socket.clone();
        std::thread::spawn(move || {
            let c = RpcSettingsStore::open(&socket, &program("tasks"), times());
            create(&c, name).unwrap();
        })
    };
    let (a, b) = (make("Arbeit"), make("Vortrag"));
    a.join().unwrap();
    b.join().unwrap();
    let got = ids(&store.index().0);
    let mut numbers: Vec<u64> = got.iter().map(|x| x.1).collect();
    numbers.sort_unstable();
    assert_eq!(numbers, [1, 2], "{got:?}");
    assert_eq!(
        got.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        ["Arbeit", "Vortrag"]
    );
}

fn flaky(fail_first: usize, socket: &std::path::Path) -> (support::Server, Arc<Flaky>) {
    let port = Arc::new(Flaky {
        inner: MemoryStore::new(),
        fail_first,
        rounds: AtomicUsize::new(0),
    });
    let shared = port.clone();
    let server = serve_on(socket, move || {
        let p: Arc<dyn SettingsStore> = shared;
        SettingsService::new(p, None::<Arc<MemoryStore>>)
    });
    (server, port)
}

/// KB13: a service that answers `index-changed` once: the second round wins; always: after 3 rounds `store-busy`.
#[test]
fn kb13_retries_three_times_then_busy() {
    let d = dir();
    let (server, port) = flaky(1, &d.path().join("a.sock"));
    let c = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let socket = server.socket.clone();
    let set = with_watchdog(move || create(&c, "Arbeit").map(|s| (ids(&s), socket)));
    assert_eq!(set.unwrap().0, [("Arbeit".to_owned(), 1)]);
    assert_eq!(port.rounds.load(std::sync::atomic::Ordering::SeqCst), 2);

    let (server, port) = flaky(usize::MAX, &d.path().join("b.sock"));
    let c = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let e = with_watchdog(move || create(&c, "Arbeit").unwrap_err());
    assert_eq!(code(&e), "store-busy");
    assert_eq!(text(&e, "socket"), Some(server.socket.to_str().unwrap()));
    assert_eq!(
        port.rounds.load(std::sync::atomic::Ordering::SeqCst),
        3,
        "exactly three rounds"
    );
}

/// KB17: a port call from inside a runtime thread is a programming error (a debug build says so).
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "never from a runtime thread")]
fn kb17_never_from_a_runtime_thread() {
    let d = dir();
    let c = RpcSettingsStore::open(&d.path().join("s.sock"), &program("tasks"), times());
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        let _ = c.stamp();
    });
}
