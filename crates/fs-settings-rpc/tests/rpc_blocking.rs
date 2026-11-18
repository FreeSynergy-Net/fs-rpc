//! KB21 (MC11): a blocking port must not stop the service. The port is sync and may block (a database); the service runs its
//! calls on the blocking pool, so a second caller is answered while the first still waits.

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use fs_settings::{MemoryStore, Place, Scope, SettingsStore};
use fs_settings_rpc::{RpcSettingsStore, SettingsService};
use support::{Slow, dir, program, serve_on, times};

const DELAY: Duration = Duration::from_millis(1500);

#[test]
fn kb21_a_blocking_port_does_not_stop_other_callers() {
    let tmp = dir();
    let server = serve_on(&tmp.path().join("s.sock"), || {
        let port = Arc::new(Slow {
            inner: MemoryStore::new(),
            delay: DELAY,
        });
        SettingsService::new(port, None::<Arc<MemoryStore>>)
    });
    let slow = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    let quick = RpcSettingsStore::open(&server.socket, &program("tasks"), times());
    // Both are connected before the slow call starts.
    let _ = quick.stamp();
    let _ = slow.stamp();
    let started = Instant::now();
    let blocked = std::thread::spawn(move || {
        let _ = slow.profile(&Scope::General, Place::Profile, 0);
        started.elapsed()
    });
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    let _ = quick.stamp();
    let answered = t.elapsed();
    assert!(
        answered < Duration::from_millis(700),
        "the second caller waited {answered:?}: the blocking port call held the service back"
    );
    assert!(
        blocked.join().unwrap() >= DELAY,
        "the slow call really took its time"
    );
}
