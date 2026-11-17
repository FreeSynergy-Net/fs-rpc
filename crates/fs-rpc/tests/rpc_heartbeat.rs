//! The heartbeat: answered by a thread of its own, busy and alive told apart, a blocked work thread seen, a save run on the heartbeat
//! thread.
//!
//! Pattern: Real sockets in a temporary folder. Mutants (each turns its test red): M1 the heartbeat is answered by the thread that
//! owns the program (the ping blocks while that thread blocks): `the_heartbeat_answers_while_the_work_thread_is_blocked`; M2 the
//! tick age is not reported (a blocked work thread looks alive): the same test; M3 `busy` keys are not cleaned (a sentence goes on
//! the wire): `a_busy_state_is_told_as_a_text_key_and_goes_away_with_idle`; M4 the hook runs on the thread that asks (not the
//! heartbeat thread): `the_save_runs_on_the_heartbeat_thread`; M5 the file stays after drop or is world-readable:
//! `the_socket_is_private_and_goes_with_the_heartbeat`.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fs_rpc::{Beat, HangTimes, Heartbeat, Hung, Saved, Verdict, Watch, ask_beat, ask_save};

fn block_on<T>(f: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

const LIMIT: Duration = Duration::from_secs(2);

#[test]
fn a_busy_state_is_told_as_a_text_key_and_goes_away_with_idle() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("p.sock");
    let hb = Heartbeat::start(&socket).unwrap();
    assert_eq!(
        block_on(ask_beat(&socket, LIMIT)).unwrap().beat,
        Beat::Alive
    );
    hb.busy("Importing the Calendar!", 41);
    let pulse = block_on(ask_beat(&socket, LIMIT)).unwrap();
    assert_eq!(
        pulse.beat,
        Beat::Busy {
            what: "-mporting-the--alendar-".into(),
            progress: 41
        },
        "a text key: lower case, digits and dashes"
    );
    hb.busy("importing", 42);
    assert!(matches!(
        block_on(ask_beat(&socket, LIMIT)).unwrap().beat,
        Beat::Busy { progress: 42, .. }
    ));
    hb.idle();
    assert_eq!(
        block_on(ask_beat(&socket, LIMIT)).unwrap().beat,
        Beat::Alive
    );
    // A program that never calls `tick()` is counted from its start: tick() is mandatory, the age grows.
    let first = block_on(ask_beat(&socket, LIMIT))
        .unwrap()
        .tick_age
        .expect("always reported");
    std::thread::sleep(Duration::from_millis(200));
    let later = block_on(ask_beat(&socket, LIMIT))
        .unwrap()
        .tick_age
        .unwrap();
    assert!(
        later >= first + Duration::from_millis(150),
        "{first:?} {later:?}"
    );
}

#[test]
fn the_heartbeat_answers_while_the_work_thread_is_blocked_and_the_tick_age_shows_it() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("p.sock");
    let hb = Heartbeat::start(&socket).unwrap();
    hb.tick();
    // The thread of the test is the work thread and it blocks (in `join`); a thread of its own asks the heartbeat meanwhile.
    let asker = {
        let socket = socket.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(400));
            block_on(ask_beat(&socket, LIMIT))
        })
    };
    let pulse = asker.join().unwrap().expect("the heartbeat thread answers");
    let age = pulse.tick_age.expect("a ticking program reports the age");
    assert!(age >= Duration::from_millis(350), "{age:?}");
    // That is a hang, not health.
    let times = HangTimes {
        ping: Duration::from_millis(100),
        no_beat: Duration::from_secs(10),
        no_progress: Duration::from_secs(90),
        no_tick: Duration::from_millis(300),
    };
    let mut watch = Watch::new(times, Instant::now());
    assert_eq!(
        watch.observe(Instant::now(), Some(&pulse)),
        Verdict::Hung(Hung::NoTick)
    );
    // A work thread that ticks is healthy.
    hb.tick();
    let fresh = block_on(ask_beat(&socket, LIMIT)).unwrap();
    assert!(fresh.tick_age.unwrap() < Duration::from_millis(300));
    assert_eq!(watch.observe(Instant::now(), Some(&fresh)), Verdict::Alive);
}

#[test]
fn nothing_there_is_an_error_not_a_hang() {
    let dir = tempfile::tempdir().unwrap();
    let began = Instant::now();
    let e = block_on(ask_beat(&dir.path().join("none.sock"), LIMIT)).unwrap_err();
    assert!(matches!(
        e.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
    ));
    assert!(began.elapsed() < Duration::from_secs(1));
}

#[test]
fn the_save_runs_on_the_heartbeat_thread() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("p.sock");
    let hb = Heartbeat::start(&socket).unwrap();
    assert_eq!(
        block_on(ask_save(&socket, LIMIT)).unwrap(),
        Saved::Nothing,
        "no hook"
    );
    let seen = Arc::new(Mutex::new(None));
    let record = seen.clone();
    hb.on_save(move || {
        *record.lock().unwrap() = Some(std::thread::current().id());
        Saved::Saved
    });
    assert_eq!(block_on(ask_save(&socket, LIMIT)).unwrap(), Saved::Saved);
    let ran_on = seen.lock().unwrap().expect("the hook ran");
    assert_ne!(ran_on, std::thread::current().id());
    hb.on_save(|| Saved::Failed);
    assert_eq!(block_on(ask_save(&socket, LIMIT)).unwrap(), Saved::Failed);
    assert_eq!(
        hb.save_now(),
        Saved::Failed,
        "the program's own stop path runs the same hook"
    );
}

#[test]
fn the_socket_is_private_and_goes_with_the_heartbeat() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("p.sock");
    let hb = Heartbeat::start(&socket).unwrap();
    let path = hb.path().to_path_buf();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(
        Heartbeat::start(&socket).is_err(),
        "one heartbeat per socket"
    );
    drop(hb);
    assert!(!path.exists());
    drop(Heartbeat::start(&socket).unwrap()); // a new one can start
}
