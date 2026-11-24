//! `module_service`: the one place of `<modules>/<name>/service[.exe]` and its checks (INST-2a). Wegwerf-Ordner under the test tmp dir.
//! The owner of a test folder is the user of the test, so the circle of trust of the "system" is this user (`Trust::for_test`);
//! the real system trust is shown by the negative cases (a folder of the user is never the system's).
//! Pattern: Test (real files, no mocks).
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};

use fs_trusted_path::{
    ModulePlaces, ModuleRefusal, Owner, SystemPlaces, SystemRefusal, Trust, program_file_with,
};

fn mode(p: &Path, m: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
}

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    uid: u32,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let root = tmp.path().to_path_buf();
        mode(&root, 0o755);
        let uid = fs::metadata(&root).unwrap().uid();
        Self {
            _tmp: tmp,
            root,
            uid,
        }
    }

    /// `<root>/<place>/<name>/service` as a plain closed file.
    fn module(&self, place: &str, name: &str) -> PathBuf {
        let dir = self.root.join(place).join(name);
        fs::create_dir_all(&dir).unwrap();
        for d in [self.root.join(place), dir.clone()] {
            mode(&d, 0o755);
        }
        let file = dir.join("service");
        fs::write(&file, "x").unwrap();
        mode(&file, 0o755);
        file
    }

    fn places(&self, bundled: &str, system: &str) -> ModulePlaces {
        let trust = Trust::for_test(vec![Owner::uid(self.uid)]);
        let roots = vec![self.root.join(system)];
        ModulePlaces::new(
            Some(self.root.join(bundled)),
            Some(self.root.join(system)),
            SystemPlaces::for_test(roots, trust),
        )
    }
}

#[test]
fn the_bundled_module_is_found_first_and_the_system_one_second() {
    let w = World::new();
    let sys = w.module("sys", "planning");
    let b = w.module("bundled", "tasks");
    let p = w.places("bundled", "sys");
    assert_eq!(p.service("planning"), Ok(sys));
    assert_eq!(p.service("tasks"), Ok(b));
    // the same name in both: the bundled one wins, as in the host
    let both = w.module("bundled", "planning");
    assert_eq!(p.service("planning"), Ok(both));
    assert_eq!(p.service("nothing"), Err(ModuleRefusal::NoService));
}

#[test]
fn a_name_that_is_no_plain_folder_name_is_refused() {
    let w = World::new();
    w.module("sys", "planning");
    let p = w.places("bundled", "sys");
    for bad in [
        "",
        "..",
        "../sys/planning",
        "a/b",
        "Planning",
        "a.b",
        "pl anning",
    ] {
        assert_eq!(p.service(bad), Err(ModuleRefusal::BadName), "{bad:?}");
    }
}

#[test]
fn a_link_for_the_folder_or_the_file_is_refused() {
    let w = World::new();
    let real = w.module("sys", "planning");
    let p = w.places("bundled", "sys");
    // the file is a link
    let other = w.root.join("sys").join("planning").join("other");
    fs::rename(&real, &other).unwrap();
    symlink(&other, &real).unwrap();
    assert_eq!(p.service("planning"), Err(ModuleRefusal::Symlink));
    // the folder is a link
    let target = w.root.join("sys").join("planning");
    let moved = w.root.join("sys").join("moved");
    fs::rename(&target, &moved).unwrap();
    symlink(&moved, &target).unwrap();
    assert_eq!(p.service("planning"), Err(ModuleRefusal::Symlink));
}

#[test]
fn a_folder_without_the_file_or_with_a_folder_instead_is_refused() {
    let w = World::new();
    fs::create_dir_all(w.root.join("sys").join("empty")).unwrap();
    let p = w.places("bundled", "sys");
    assert_eq!(p.service("empty"), Err(ModuleRefusal::NoService));
    let dir = w.root.join("sys").join("odd");
    fs::create_dir_all(dir.join("service")).unwrap();
    assert_eq!(p.service("odd"), Err(ModuleRefusal::NotAFile));
}

#[test]
fn a_system_module_that_somebody_else_can_write_is_not_the_systems() {
    let w = World::new();
    let file = w.module("sys", "planning");
    let p = w.places("bundled", "sys");
    assert_eq!(p.service("planning"), Ok(file.clone()));
    mode(&file, 0o775);
    assert_eq!(p.service("planning"), Err(ModuleRefusal::NotSystem));
    mode(&file, 0o757);
    assert_eq!(p.service("planning"), Err(ModuleRefusal::NotSystem));
    mode(&file, 0o755);
    // the folder of the module writable by others: it can swap the file
    mode(file.parent().unwrap(), 0o777);
    assert_eq!(p.service("planning"), Err(ModuleRefusal::NotSystem));
}

#[test]
fn a_bundled_module_is_not_held_to_the_system_owner_but_to_the_link_rules() {
    let w = World::new();
    let file = w.module("bundled", "planning");
    // no `for_test` for the system trust: a folder of the user is never the system's
    let real = ModulePlaces::new(
        Some(w.root.join("bundled")),
        Some(w.root.join("sys")),
        SystemPlaces::standard(),
    );
    assert_eq!(real.service("planning"), Ok(file));
}

#[test]
fn a_folder_of_the_user_is_not_a_system_module_with_the_real_trust() {
    let w = World::new();
    w.module("sys", "planning");
    let real = ModulePlaces::new(None, Some(w.root.join("sys")), SystemPlaces::standard());
    // outside the real places of the system
    assert_eq!(real.service("planning"), Err(ModuleRefusal::NotSystem));
    assert_eq!(
        SystemPlaces::standard().check(&w.root.join("sys")),
        Err(SystemRefusal::OutsideSystem)
    );
}

#[test]
fn the_system_check_walks_every_part_and_refuses_what_is_relative() {
    let w = World::new();
    let file = w.module("sys", "planning");
    let places = SystemPlaces::for_test(
        vec![w.root.join("sys")],
        Trust::for_test(vec![Owner::uid(w.uid)]),
    );
    assert_eq!(places.check(&file), Ok(()));
    assert_eq!(
        places.check(Path::new("sys/planning")),
        Err(SystemRefusal::NotAbsolute)
    );
    assert_eq!(
        places.check(&w.root.join("sys").join("..").join("sys")),
        Err(SystemRefusal::NotAbsolute)
    );
    assert_eq!(places.check(&w.root), Err(SystemRefusal::OutsideSystem));
    // the place itself writable by others
    mode(&w.root.join("sys"), 0o777);
    assert_eq!(
        places.check(&file),
        Err(SystemRefusal::Part(fs_trusted_path::PlaceRefusal::Writable))
    );
}

#[test]
fn the_windows_form_is_service_exe_and_a_bare_service_next_to_it_is_ambiguous() {
    let w = World::new();
    let dir = w.root.join("sys").join("planning");
    fs::create_dir_all(&dir).unwrap();
    mode(&w.root.join("sys"), 0o755);
    mode(&dir, 0o755);
    let exe = program_file_with(&dir, "service", ".exe");
    assert_eq!(exe, dir.join("service.exe"));
    fs::write(&exe, "x").unwrap();
    mode(&exe, 0o755);
    let p = w.places("bundled", "sys").with_suffix(".exe");
    assert_eq!(p.service("planning"), Ok(exe));
    // a bare `service` next to it: a second candidate, refused
    fs::write(dir.join("service"), "x").unwrap();
    assert_eq!(p.service("planning"), Err(ModuleRefusal::AmbiguousService));
    // without an ending the bare name IS the program file: nothing ambiguous
    fs::remove_file(dir.join("service.exe")).unwrap();
    mode(&dir.join("service"), 0o755);
    assert_eq!(
        w.places("bundled", "sys")
            .with_suffix("")
            .service("planning"),
        Ok(dir.join("service"))
    );
}

#[test]
fn the_refusal_words_are_the_words_of_the_host() {
    for (r, w) in [
        (ModuleRefusal::BadName, "bad-name"),
        (ModuleRefusal::NoService, "no-service"),
        (ModuleRefusal::Symlink, "symlink"),
        (ModuleRefusal::NotAFile, "not-a-file"),
        (ModuleRefusal::AmbiguousService, "ambiguous-service"),
        (ModuleRefusal::NotSystem, "not-system"),
    ] {
        assert_eq!(r.to_string(), w);
    }
}
