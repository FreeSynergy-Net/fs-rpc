//! The ONE place that says where the program file of a module is (INST-2a): `<modules>/<name>/service[.exe]`. The host (fs-starter) and
//! every client that starts a module service call [`module_service`]; nobody makes this path on their own.
//!
//! Rules (taken over from fs-starter `sources/known.rs` `Known::service_with`, same words):
//! - the file is `<folder>/service` (Windows `service.exe`), never found through `PATH` or a word of a description;
//! - the folder and the file are no link, the file is a regular file; where program files have an ending, a bare `service` next to
//!   `service.exe` is a second candidate the system could start instead: refused (`ambiguous-service`);
//! - a module of the SYSTEM folder (package manager, installer) also has to pass [`SystemPlaces::check`] for the file (owner, not
//!   writable by others, no link on the way, Windows: local disk);
//! - the bundled folder (`modules/` next to the running program) is read first (as the host does), the system folder second; a place
//!   that does not have the folder of the module is skipped, a place that has it decides (a refusal never falls through to the next).
//!
//! Pattern: Specification (a check over the file system metadata; no state) + Value ([`ModuleRefusal`] names the border hit).

use std::fmt;
use std::path::{Path, PathBuf};

use crate::layout::{
    EXE_SUFFIX, SERVICE_STEM, bundled_modules_dir, is_ambiguous, plain_name, program_file_with,
    system_modules_dir,
};
use crate::places::SystemPlaces;

/// Why there is no program file to start for a module (one border each; the cause word is the text of [`Display`](fmt::Display)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleRefusal {
    /// The name is not a plain folder name (`[a-z0-9_-]+`).
    BadName,
    /// No place has a folder or a file for this module.
    NoService,
    /// The folder or the file is a link.
    Symlink,
    /// The file is not a regular file.
    NotAFile,
    /// Where program files have an ending, a bare `service` stands next to `service.exe`.
    AmbiguousService,
    /// A module of the system folder fails the system check (owner, rights, link on the way, disk).
    NotSystem,
}

impl fmt::Display for ModuleRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadName => "bad-name",
            Self::NoService => "no-service",
            Self::Symlink => "symlink",
            Self::NotAFile => "not-a-file",
            Self::AmbiguousService => "ambiguous-service",
            Self::NotSystem => "not-system",
        })
    }
}

/// The places where a module folder may be, and how its file is checked.
#[derive(Clone, Debug)]
pub struct ModulePlaces {
    bundled: Option<PathBuf>,
    system: Option<PathBuf>,
    trust: SystemPlaces,
    suffix: String,
}

impl ModulePlaces {
    /// The real places: `modules/` next to the running program and the system folder of the modules, the real system check.
    #[must_use]
    pub fn standard() -> Self {
        let system = system_modules_dir();
        let bundled = bundled_modules_dir().filter(|b| {
            // Windows: `modules\` next to the starter is the system folder after the install: it counts once, as the system's.
            !system.as_deref().is_some_and(|s| same_folder(b, s))
        });
        Self::new(bundled, system, SystemPlaces::standard())
    }

    /// Other folders and another check (tests; a program uses [`Self::standard`]).
    #[must_use]
    pub fn new(bundled: Option<PathBuf>, system: Option<PathBuf>, trust: SystemPlaces) -> Self {
        Self {
            bundled,
            system,
            trust,
            suffix: EXE_SUFFIX.to_owned(),
        }
    }

    /// The same with another ending of program files (tests run the Windows form under Linux, too).
    #[doc(hidden)]
    #[must_use]
    pub fn with_suffix(mut self, suffix: &str) -> Self {
        suffix.clone_into(&mut self.suffix);
        self
    }

    /// The program file of the module `name`, checked: the file that is returned is the file to start.
    ///
    /// # Errors
    /// The first border that is hit, see [`ModuleRefusal`].
    pub fn service(&self, name: &str) -> Result<PathBuf, ModuleRefusal> {
        if !plain_name(name) {
            return Err(ModuleRefusal::BadName);
        }
        let places = [(&self.bundled, false), (&self.system, true)];
        for (dir, packaged) in places {
            let Some(dir) = dir else { continue };
            let folder = dir.join(name);
            if std::fs::symlink_metadata(&folder).is_err() {
                continue;
            }
            return self.check_folder(&folder, packaged);
        }
        Err(ModuleRefusal::NoService)
    }

    fn check_folder(&self, folder: &Path, packaged: bool) -> Result<PathBuf, ModuleRefusal> {
        if is_ambiguous(folder, SERVICE_STEM, &self.suffix) {
            return Err(ModuleRefusal::AmbiguousService);
        }
        let file = program_file_with(folder, SERVICE_STEM, &self.suffix);
        for p in [folder, file.as_path()] {
            let m = std::fs::symlink_metadata(p).map_err(|_| ModuleRefusal::NoService)?;
            if m.file_type().is_symlink() {
                return Err(ModuleRefusal::Symlink);
            }
        }
        if !std::fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
            return Err(ModuleRefusal::NotAFile);
        }
        if packaged && self.trust.check(&file).is_err() {
            return Err(ModuleRefusal::NotSystem);
        }
        Ok(file)
    }
}

/// The program file of the module `name` (`<modules>/<name>/service[.exe]`) in the real places, checked. The one call for host and clients.
///
/// # Errors
/// The first border that is hit, see [`ModuleRefusal`].
pub fn module_service(name: &str) -> Result<PathBuf, ModuleRefusal> {
    ModulePlaces::standard().service(name)
}

fn same_folder(a: &Path, b: &Path) -> bool {
    a == b
        || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}
