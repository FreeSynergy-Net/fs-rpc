//! What the system says about a process: start time, program file, user (Linux `/proc`).
//!
//! Pattern: Gateway — the only place that reads `/proc`. The functions take the root of the tree, so tests use a made-up one.
//! Everything that cannot be read is `None`: the rule then refuses, it never guesses.

use std::os::unix::fs::MetadataExt;
use std::path::Path;

use super::rule::{Image, Own, Peer};

/// The real `/proc`.
pub(super) const PROC: &str = "/proc";

/// The state letter and the start time (ticks since boot) from `/proc/<pid>/stat`. The program name stands in brackets and may hold
/// spaces and brackets, so the fields count from the last `)`.
pub(super) fn parse_stat(text: &str) -> Option<(char, u64)> {
    let after = &text[text.rfind(')')? + 1..];
    let mut fields = after.split_whitespace();
    let state = fields.next()?.chars().next()?;
    // After the state: ppid pgrp session tty tpgid flags minflt cminflt majflt cmajflt utime stime cutime cstime priority nice
    // threads itrealvalue, then the start time (the 22nd field of the whole line).
    let start = fields.nth(18)?.parse().ok()?;
    Some((state, start))
}

/// The state and the start time of `pid`; `None` if there is no such process (or it is hidden).
pub(super) fn stat_of(root: &Path, pid: i32) -> Option<(char, u64)> {
    parse_stat(&std::fs::read_to_string(root.join(pid.to_string()).join("stat")).ok()?)
}

/// The program file behind a link such as `/proc/<pid>/exe`: the file itself (the link is followed) and its real path.
pub(super) fn image_of(link: &Path) -> Option<Image> {
    let meta = std::fs::metadata(link).ok()?;
    let path = std::fs::read_link(link).ok()?;
    let path = path.to_string_lossy();
    Some(Image {
        dev: meta.dev(),
        ino: meta.ino(),
        // ` (deleted)` is the kernel's word for a file that is gone (no link left). A file that merely has that text in its name
        // (a copy) is not cut: its real name is the name.
        path: if meta.nlink() == 0 {
            path.strip_suffix(" (deleted)").unwrap_or(&path).to_owned()
        } else {
            path.into_owned()
        },
    })
}

/// A program file given by its path (a link is followed).
pub(super) fn image_at(path: &Path) -> Option<Image> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Image {
        dev: meta.dev(),
        ino: meta.ino(),
        path: std::fs::canonicalize(path)
            .ok()?
            .to_string_lossy()
            .into_owned(),
    })
}

/// The facts about `pid`, found through the socket (`cred_uid` is what the socket said about its owner). The user is the owner of
/// `/proc/<pid>`; if that does not agree with the socket, the user counts as unknown.
pub(super) fn peer_in(root: &Path, pid: i32, cred_uid: u32) -> Peer {
    let dir = root.join(pid.to_string());
    let owner = std::fs::metadata(&dir).ok().map(|m| m.uid());
    Peer {
        pid,
        uid: owner.filter(|o| *o == cred_uid).unwrap_or(u32::MAX),
        start: stat_of(root, pid).map(|(_, start)| start),
        exe: image_of(&dir.join("exe")),
    }
}

/// The facts about this process; `exe` is the program file that counts as this program (default: the own one).
pub(super) fn own(exe: Option<&Path>) -> Option<Own> {
    Some(Own {
        pid: i32::try_from(std::process::id()).ok()?,
        uid: rustix::process::geteuid().as_raw(),
        exe: match exe {
            Some(path) => image_at(path)?,
            None => image_of(&Path::new(PROC).join("self").join("exe"))?,
        },
    })
}

/// Whether the process is gone for good: no such process, another process with the same number, or the WHOLE thread group is dead.
/// The main thread alone is not enough: it shows `Z` as soon as it has exited, while other threads still hold the socket and its lock
/// file; a new server started at that moment fails with "already running". So a `Z` leader counts only if every thread in
/// `/proc/<pid>/task/` is a zombie or dead too (no task directory: nothing else is left).
pub(super) fn is_gone(root: &Path, pid: i32, start: u64) -> bool {
    match stat_of(root, pid) {
        None | Some(('X', _)) => true,
        Some((_, now)) if now != start => true,
        Some(('Z', _)) => threads_dead(root, pid),
        Some(_) => false,
    }
}

/// Every thread of `pid` is a zombie or gone.
fn threads_dead(root: &Path, pid: i32) -> bool {
    let Ok(tasks) = std::fs::read_dir(root.join(pid.to_string()).join("task")) else {
        return true;
    };
    tasks.flatten().all(|task| {
        parse_stat_state(&std::fs::read_to_string(task.path().join("stat")).unwrap_or_default())
            .is_none_or(|state| state == 'Z' || state == 'X')
    })
}

fn parse_stat_state(text: &str) -> Option<char> {
    text.get(text.rfind(')')? + 1..)?
        .split_whitespace()
        .next()?
        .chars()
        .next()
}

/// Seconds since 1970 as `YYYY-MM-DDTHH:MM:SSZ` (the civil-date rule of H. Hinnant), for the log.
pub(super) fn utc(epoch: u64) -> String {
    let (days, secs) = (epoch / 86_400, epoch % 86_400);
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}
