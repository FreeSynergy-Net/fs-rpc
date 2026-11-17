//! The reading of `/proc` on a made-up tree: the start time, the program file (also ` (deleted)`), a zombie, a reused number.
//!
//! Pattern: Table tests on a temporary tree. Mutants: M1 the start time is read from the wrong field; M2 ` (deleted)` is not cut off;
//! M3 a zombie or another start time does not count as gone; M5 only the main thread is looked at; M4 the user is taken from the folder without comparing with the socket.

use super::procfs::{image_of, is_gone, own, parse_stat, peer_in};

#[test]
fn the_start_time_is_the_22nd_field_after_the_last_bracket() {
    let line =
        "4242 (fs-x (x) y) S 1 4242 4242 0 -1 4194560 100 0 0 0 5 3 0 0 20 0 8 0 987654 1 2 3";
    assert_eq!(parse_stat(line), Some(('S', 987_654)));
    assert_eq!(parse_stat("garbage"), None);
    assert_eq!(parse_stat("1 (x) S 1 2"), None);
}

#[test]
fn a_made_up_tree_gives_the_facts_and_a_zombie_or_a_new_start_is_gone() {
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("77");
    std::fs::create_dir(&dir).unwrap();
    let stat = |state: &str, start: u64| {
        format!("77 (x) {state} 1 77 77 0 -1 0 0 0 0 0 0 0 0 0 20 0 1 0 {start} 1")
    };
    std::fs::write(dir.join("stat"), stat("S", 500)).unwrap();
    std::os::unix::fs::symlink(std::env::current_exe().unwrap(), dir.join("exe")).unwrap();
    let me = rustix::process::geteuid().as_raw();
    let found = peer_in(root.path(), 77, me);
    assert_eq!((found.pid, found.uid, found.start), (77, me, Some(500)));
    assert!(found.exe.unwrap().same_as(&own(None).unwrap().exe));
    assert_eq!(
        peer_in(root.path(), 77, me + 1).uid,
        u32::MAX,
        "the socket named another user"
    );
    assert!(!is_gone(root.path(), 77, 500));
    assert!(
        is_gone(root.path(), 77, 501),
        "another start: another process"
    );
    std::fs::write(dir.join("stat"), stat("Z", 500)).unwrap();
    assert!(is_gone(root.path(), 77, 500), "a zombie holds nothing");
    assert!(is_gone(root.path(), 78, 500), "no such process");
}

#[test]
fn a_replaced_program_file_keeps_its_real_path_without_the_word_deleted() {
    // The real /proc: a copy of `sleep` runs, its file is removed (what an update does), the process stays.
    let source = std::path::Path::new("/usr/bin/sleep");
    if !source.exists() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let copy = dir.path().join("prog-under-test");
    std::fs::copy(source, &copy).unwrap();
    let mut child = None;
    for _ in 0..50 {
        match std::process::Command::new(&copy).arg("30").spawn() {
            Ok(c) => {
                child = Some(c);
                break;
            }
            Err(e) if e.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            Err(e) => panic!("{e}"),
        }
    }
    let mut child = child.expect("started");
    let real = std::fs::canonicalize(&copy).unwrap();
    let link = std::path::PathBuf::from(format!("/proc/{}/exe", child.id()));
    let before = image_of(&link).unwrap();
    std::fs::remove_file(&copy).unwrap();
    let after = image_of(&link).unwrap();
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(before.path, real.to_string_lossy());
    assert_eq!(after.path, real.to_string_lossy(), "no ` (deleted)`");
    assert_eq!((before.dev, before.ino), (after.dev, after.ino));
}

#[test]
fn a_zombie_main_thread_with_a_live_thread_is_not_gone() {
    // The main thread has exited (state Z) but another thread still holds the socket and its lock file: not gone yet; gone when every
    // thread is a zombie. Mutant: only the main thread is looked at (a new server then fails with "already running").
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("77");
    std::fs::create_dir_all(dir.join("task/77")).unwrap();
    std::fs::create_dir_all(dir.join("task/78")).unwrap();
    let stat = |tid: u32, state: &str| {
        format!("{tid} (x) {state} 1 77 77 0 -1 0 0 0 0 0 0 0 0 0 20 0 2 0 500 1")
    };
    std::fs::write(dir.join("stat"), stat(77, "Z")).unwrap();
    std::fs::write(dir.join("task/77/stat"), stat(77, "Z")).unwrap();
    std::fs::write(dir.join("task/78/stat"), stat(78, "S")).unwrap();
    assert!(!is_gone(root.path(), 77, 500), "a thread still lives");
    std::fs::write(dir.join("task/78/stat"), stat(78, "Z")).unwrap();
    assert!(is_gone(root.path(), 77, 500), "every thread is dead");
}
