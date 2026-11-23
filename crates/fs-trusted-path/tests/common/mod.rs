//! Helpers of the Windows tests: a file with a real ACL made by icacls. Pattern: Test helper.
#![cfg(windows)]

use std::fs;
use std::path::Path;
use std::process::Command;

/// A file with exactly the rights `grants` (a SID text and a right each), no inheritance: a real ACL, made by icacls.
#[must_use]
pub fn file_with(dir: &Path, grants: &[(&str, &str)]) -> std::path::PathBuf {
    let file = dir.join("a.json");
    fs::write(&file, "{}").unwrap();
    icacls(&file, &["/inheritance:r"]);
    for (sid, right) in grants {
        icacls(&file, &["/grant:r", &format!("*{sid}:{right}")]);
    }
    file
}

pub fn icacls(file: &Path, args: &[&str]) {
    let out = Command::new("icacls")
        .arg(file)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "icacls {args:?}: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
