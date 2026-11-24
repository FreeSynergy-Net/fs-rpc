//! Where program files and module folders are, in ONE place (WIN-HOST WH1, INST-2a; moved from fs-starter `layout.rs` and `system.rs`).
//!
//! The name of a program file is made only here: `service` under Unix, `service.exe` under Windows. The FULL name (with the ending)
//! goes to `Command`, so the file that was checked is the file that is started (Rust adds `.exe` to a path without ending under Windows
//! and would start `service.exe` although `service` was checked).
//!
//! Pattern: Value functions (no state, nothing read except where said).

use std::path::{Path, PathBuf};

/// The file stem of a module's program: `<module folder>/service[.exe]`.
pub const SERVICE_STEM: &str = "service";

/// The ending of a program file of this system (`""` under Unix, `".exe"` under Windows).
pub const EXE_SUFFIX: &str = std::env::consts::EXE_SUFFIX;

/// The folder of the starter binary of the rpm.
#[cfg(unix)]
pub const STARTER_DIR: &str = "/usr/bin";
/// The folder of the module binaries of the rpm (`<name>/service`).
#[cfg(unix)]
pub const MODULES_DIR: &str = "/usr/libexec/freesynergy/modules";
/// The folder of the self descriptions and frontends of the system level.
#[cfg(unix)]
pub const SHARE_DIR: &str = "/usr/share/freesynergy";

/// The program file `stem` in `dir`, with the ending of this system. The one place that makes this name.
#[must_use]
pub fn program_file(dir: &Path, stem: &str) -> PathBuf {
    program_file_with(dir, stem, EXE_SUFFIX)
}

/// [`program_file`] with an explicit ending (the tests run the Windows form under Linux, too).
#[doc(hidden)]
#[must_use]
pub fn program_file_with(dir: &Path, stem: &str, suffix: &str) -> PathBuf {
    dir.join(format!("{stem}{suffix}"))
}

/// Does a file with the bare name `stem` stand next to the program file `stem + suffix`? Where programs have an ending, that is a
/// second file the system could start instead of the checked one: ambiguous, never accepted. Without an ending the program file IS
/// the bare name, so nothing is ambiguous.
#[doc(hidden)]
#[must_use]
pub fn is_ambiguous(dir: &Path, stem: &str, suffix: &str) -> bool {
    !suffix.is_empty() && std::fs::symlink_metadata(dir.join(stem)).is_ok()
}

/// `<folder of the running program>/modules`: the bundled modules.
#[must_use]
pub fn bundled_modules_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("modules")))
}

/// The system folder of the modules brought by the package manager or the installer: Unix `/usr/libexec/freesynergy/modules`, Windows
/// `<anchor>\modules` (`<ProgramFiles>\FreeSynergy\modules`, from the shell, never from an environment variable); `None` where there is none.
#[must_use]
#[allow(clippy::unnecessary_wraps)] // one signature on every system: Unix always has the folder, Windows only if the shell tells
pub fn system_modules_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(PathBuf::from(MODULES_DIR))
    }
    #[cfg(windows)]
    {
        crate::known_folder::anchor().map(|a| a.join("modules"))
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

/// A plain folder name: `[a-z0-9_-]+` (no dot, no slash: no way out of the folder).
pub(crate) fn plain_name(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}
