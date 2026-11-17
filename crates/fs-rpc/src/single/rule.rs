//! The rule for a server that does not answer: may it be ended? A pure decision over facts.
//!
//! Pattern: Policy + Guard (generalised from fs-tasks 0.0.10, `takeover.rs`) — `decide` looks only at facts of the process behind
//! the socket and says `TakeOver` (it is provably this program of this user: it may be ended) or `Refuse` (never). The facts are read
//! elsewhere (`procfs`), the ending is done elsewhere (`ending`), so every rule is tested on made-up facts: another user,
//! another program under the same name, a reused process number, a link to this program. Another program is only reported.

/// A program file: where it is and which file it really is (a link or a hard link to it is the same file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// The device of the file.
    pub dev: u64,
    /// The file number on the device.
    pub ino: u64,
    /// The real path (links resolved; a trailing ` (deleted)` of a replaced file is cut off).
    pub path: String,
}

impl Image {
    /// The same program file: the same file, or the same real path (the file was replaced by an update, the running process is the
    /// old copy). The name alone never counts: a file of the same name elsewhere is another program.
    #[must_use]
    pub fn same_as(&self, other: &Image) -> bool {
        (self.dev == other.dev && self.ino == other.ino) || self.path == other.path
    }
}

/// What is known about the process that holds the socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    /// Its process number.
    pub pid: i32,
    /// Its user (`u32::MAX` if the facts do not agree).
    pub uid: u32,
    /// When it started, in clock ticks since the boot; `None` if it cannot be read.
    pub start: Option<u64>,
    /// Its program file; `None` if it cannot be read.
    pub exe: Option<Image>,
}

/// This process, and the program file that may be ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Own {
    /// The process number.
    pub pid: i32,
    /// The user.
    pub uid: u32,
    /// The program file that counts as "this program" (by default the own one).
    pub exe: Image,
}

/// Why a process is not ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The process number is the system's (1 or lower) or this process itself.
    Itself,
    /// The process belongs to another user.
    ForeignUser,
    /// Its start time or program file cannot be read: nothing can be verified.
    Unreadable,
    /// It started at another time than the one seen first: the number was reused by another process.
    PidReused,
    /// It runs another program file. Only reported, never ended, also not with a yes.
    OtherProgram,
}

impl Why {
    /// The stable token (the parameter `why`).
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Why::Itself => "itself",
            Why::ForeignUser => "foreign-user",
            Why::Unreadable => "unreadable",
            Why::PidReused => "pid-reused",
            Why::OtherProgram => "other-program",
        }
    }
}

/// What to do with the process behind a socket that does not answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// It is this program of this user, the same process as seen first.
    TakeOver,
    /// Never.
    Refuse(Why),
}

/// Decide about `peer`. `seen` is the start time seen when the process was found first: one that started at another time is not
/// the one that was seen.
#[must_use]
pub fn decide(seen: Option<u64>, peer: &Peer, own: &Own) -> Decision {
    if peer.pid <= 1 || peer.pid == own.pid {
        return Decision::Refuse(Why::Itself);
    }
    if peer.uid != own.uid {
        return Decision::Refuse(Why::ForeignUser);
    }
    match (seen, peer.start) {
        (Some(first), Some(now)) if first == now => {}
        (Some(_), Some(_)) => return Decision::Refuse(Why::PidReused),
        _ => return Decision::Refuse(Why::Unreadable),
    }
    match &peer.exe {
        None => Decision::Refuse(Why::Unreadable),
        Some(exe) if exe.same_as(&own.exe) => Decision::TakeOver,
        Some(_) => Decision::Refuse(Why::OtherProgram),
    }
}
