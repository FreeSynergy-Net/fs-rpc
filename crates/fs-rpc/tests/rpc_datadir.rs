//! `prepare_data_dir` on real folders: made private, an open folder corrected, a link, a file and an open ancestor refused with
//! the code `data-dir-not-private` and its `cause`.
//!
//! Pattern: Table tests in a temporary folder (Unix). Mutants (each turns a test red): M1 the leaf-link check removed (a link to a
//! folder is followed); M2 `Tighten` not applied (an open folder stays open); M3 the correction gives bits (0o755 -> 0o777);
//! M4 the check of the ancestors removed; M5 `RpcError::from(&DataDirError)` maps to `socket-unreachable` (the old
//! `PermissionDenied` heuristic); M6 the path is made a parameter by the library (it is the program's to add).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;

use fs_rpc::{DataDirCause, DataDirError, Param, RpcError, RpcErrorCode, prepare_data_dir};

fn mode(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn cause(e: &DataDirError) -> DataDirCause {
    e.cause().expect("a refusal with a cause")
}

#[test]
fn an_open_folder_is_corrected_to_the_owner_bits_and_nothing_inside_changes() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir(&data).unwrap();
    std::fs::write(data.join("db"), "x").unwrap();
    std::fs::set_permissions(data.join("db"), PermissionsExt::from_mode(0o644)).unwrap();
    std::fs::set_permissions(&data, PermissionsExt::from_mode(0o775)).unwrap();
    assert_eq!(
        prepare_data_dir(&data).unwrap(),
        data.canonicalize().unwrap()
    );
    assert_eq!(mode(&data), 0o700);
    assert_eq!(
        mode(&data.join("db")),
        0o644,
        "what is inside stays as it is"
    );
    // Repeating changes nothing, and nothing is added to a stricter folder.
    std::fs::set_permissions(&data, PermissionsExt::from_mode(0o500)).unwrap();
    prepare_data_dir(&data).unwrap();
    assert_eq!(mode(&data), 0o500);
}

#[test]
fn a_link_as_the_data_folder_is_refused_as_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let e = prepare_data_dir(&link).unwrap_err();
    assert_eq!(cause(&e), DataDirCause::Symlink);
    assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
}

#[test]
fn a_file_is_refused_as_not_a_directory() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    std::fs::write(&file, "x").unwrap();
    // `create_private` leaves an existing path as it is; the rule then refuses it.
    let e = prepare_data_dir(&file).unwrap_err();
    assert_eq!(cause(&e), DataDirCause::NotDirectory);
}

#[test]
fn a_folder_below_a_writable_ancestor_is_refused_as_ancestor() {
    let dir = tempfile::tempdir().unwrap();
    let open = dir.path().join("open");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, PermissionsExt::from_mode(0o777)).unwrap();
    let e = prepare_data_dir(&open.join("data")).unwrap_err();
    assert_eq!(cause(&e), DataDirCause::Ancestor);
}

#[test]
fn the_code_is_data_dir_not_private_with_the_cause_and_no_path() {
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(dir.path(), &link).unwrap();
    let e = prepare_data_dir(&link).unwrap_err();
    let coded = RpcError::from(&e);
    assert_eq!(coded.code(), RpcErrorCode::DataDirNotPrivate);
    assert_eq!(coded.message_key(), "rpc-error-data-dir-not-private");
    assert_eq!(
        coded.param("cause").and_then(Param::as_text),
        Some("symlink")
    );
    assert_eq!(
        coded.params().count(),
        1,
        "the path is the program's to add"
    );
    assert!(!coded.to_string().contains(dir.path().to_str().unwrap()));
}

#[test]
fn a_system_failure_is_io_not_socket_unreachable() {
    // Below a file: the folder cannot be made (`NotADirectory`).
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f");
    std::fs::write(&file, "x").unwrap();
    let e = prepare_data_dir(&file.join("below")).unwrap_err();
    assert!(e.cause().is_none());
    let coded = RpcError::from(&e);
    assert_eq!(coded.code(), RpcErrorCode::Io);
    assert!(coded.param("kind").is_some());
}

#[test]
fn it_converts_into_an_io_error_for_a_caller_that_answers_one() {
    fn caller(p: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
        Ok(prepare_data_dir(p)?)
    }
    let dir = tempfile::tempdir().unwrap();
    let link = dir.path().join("l");
    std::os::unix::fs::symlink(dir.path(), &link).unwrap();
    assert_eq!(
        caller(&link).unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied
    );
}
