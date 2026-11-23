//! The Unix check against real files. The negative cases need no `for_test`: a folder of this test run is owned by whoever runs
//! the test, never by root, so the real system trust must refuse it. Pattern: Test.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::Path;

use fs_trusted_path::{Level, Owner, PlaceRefusal, Trust, system_owned};

fn mode(p: &Path, m: u32) {
    fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();
}

fn world() -> (tempfile::TempDir, std::path::PathBuf, u32) {
    let tmp = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let file = tmp.path().join("commands.json");
    fs::write(&file, "{}").unwrap();
    mode(tmp.path(), 0o755);
    mode(&file, 0o644);
    let uid = fs::metadata(&file).unwrap().uid();
    (tmp, file, uid)
}

#[test]
fn a_file_of_the_user_is_not_the_systems_without_for_test() {
    let (_t, file, uid) = world();
    let real = system_owned(&file, Level::Anchor);
    if uid == 0 {
        // root runs this: the file is root's and closed, so it does count
        assert_eq!(real, Ok(()));
    } else {
        assert_eq!(real, Err(PlaceRefusal::WrongOwner));
    }
    // a real system place of the machine is the system's: the folder of the loader is root's and closed
    assert_eq!(system_owned(Path::new("/usr"), Level::Anchor), Ok(()));
}

#[test]
fn a_real_file_of_somebody_else_is_refused_whatever_its_mode() {
    let (_t, file, uid) = world();
    if uid == 0 {
        return;
    }
    mode(&file, 0o600);
    assert_eq!(
        Trust::system().system_owned(&file, Level::Anchor),
        Err(PlaceRefusal::WrongOwner)
    );
    // a real folder of root that everybody may write is not closed
    assert_eq!(
        system_owned(Path::new("/tmp"), Level::Anchor),
        Err(PlaceRefusal::Writable)
    );
}

#[test]
fn an_owned_closed_file_counts_and_a_writable_or_linked_one_does_not() {
    let (t, file, uid) = world();
    let trust = Trust::for_test(vec![Owner::uid(uid)]);
    assert_eq!(trust.system_owned(&file, Level::Anchor), Ok(()));
    for m in [0o664, 0o646] {
        mode(&file, m);
        assert_eq!(
            trust.system_owned(&file, Level::Anchor),
            Err(PlaceRefusal::Writable)
        );
    }
    mode(&file, 0o644);
    let link = t.path().join("link.json");
    symlink(&file, &link).unwrap();
    assert_eq!(
        trust.system_owned(&link, Level::Anchor),
        Err(PlaceRefusal::Symlink)
    );
    assert_eq!(
        trust.system_owned(&t.path().join("none"), Level::Anchor),
        Err(PlaceRefusal::Unreadable)
    );
    // a trusted owner that is not the file's
    let other = Trust::for_test(vec![Owner::uid(uid.wrapping_add(1))]);
    assert_eq!(
        other.system_owned(&file, Level::Anchor),
        Err(PlaceRefusal::WrongOwner)
    );
}
