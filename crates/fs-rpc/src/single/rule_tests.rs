//! The rule for a server that does not answer, on made-up facts: one test per rule.
//!
//! Pattern: Table tests — each rule of `decide` with the one fact that breaks it. Mutants (each turns its test red): M1 the check of
//! pid <= 1 or own pid removed; M2 the user check removed; M3 the start-time comparison removed (a reused number is ended); M4 an
//! unreadable start time or program file is accepted; M5 the program is compared by file name; M6 a link or a replaced file
//! (same inode / same real path) is refused.

use super::rule::{Decision, Image, Own, Peer, Why, decide};

const ME: u32 = 1000;

fn image(ino: u64, path: &str) -> Image {
    Image {
        dev: 7,
        ino,
        path: path.into(),
    }
}

fn own() -> Own {
    Own {
        pid: 500,
        uid: ME,
        exe: image(10, "/usr/bin/fs-x"),
    }
}

fn peer() -> Peer {
    Peer {
        pid: 900,
        uid: ME,
        start: Some(4242),
        exe: Some(image(10, "/usr/bin/fs-x")),
    }
}

#[test]
fn the_same_program_of_the_same_user_seen_before_is_taken_over() {
    assert_eq!(decide(Some(4242), &peer(), &own()), Decision::TakeOver);
}

#[test]
fn never_the_system_or_this_process() {
    for pid in [1, 0, -1, 500] {
        let p = Peer { pid, ..peer() };
        assert_eq!(
            decide(Some(4242), &p, &own()),
            Decision::Refuse(Why::Itself),
            "{pid}"
        );
    }
}

#[test]
fn never_a_process_of_another_user_or_of_an_unclear_one() {
    for uid in [0, ME + 1, u32::MAX] {
        let p = Peer { uid, ..peer() };
        assert_eq!(
            decide(Some(4242), &p, &own()),
            Decision::Refuse(Why::ForeignUser),
            "{uid}"
        );
    }
}

#[test]
fn a_reused_process_number_is_not_the_process_that_was_seen() {
    let p = Peer {
        start: Some(9999),
        ..peer()
    };
    assert_eq!(
        decide(Some(4242), &p, &own()),
        Decision::Refuse(Why::PidReused)
    );
}

#[test]
fn what_cannot_be_read_cannot_be_verified() {
    let no_start = Peer {
        start: None,
        ..peer()
    };
    assert_eq!(
        decide(Some(4242), &no_start, &own()),
        Decision::Refuse(Why::Unreadable)
    );
    assert_eq!(
        decide(None, &peer(), &own()),
        Decision::Refuse(Why::Unreadable)
    );
    let no_exe = Peer {
        exe: None,
        ..peer()
    };
    assert_eq!(
        decide(Some(4242), &no_exe, &own()),
        Decision::Refuse(Why::Unreadable)
    );
}

#[test]
fn another_program_file_is_refused_even_with_the_same_name() {
    let p = Peer {
        exe: Some(image(11, "/home/x/bin/fs-x")),
        ..peer()
    };
    assert_eq!(
        decide(Some(4242), &p, &own()),
        Decision::Refuse(Why::OtherProgram)
    );
}

#[test]
fn a_link_a_hard_link_and_a_replaced_file_are_this_program() {
    let linked = Peer {
        exe: Some(image(10, "/home/x/link-to-x")),
        ..peer()
    };
    assert_eq!(decide(Some(4242), &linked, &own()), Decision::TakeOver);
    let updated = Peer {
        exe: Some(image(99, "/usr/bin/fs-x")),
        ..peer()
    };
    assert_eq!(decide(Some(4242), &updated, &own()), Decision::TakeOver);
}

#[test]
fn the_tokens_are_fixed() {
    let tokens = [
        Why::Itself,
        Why::ForeignUser,
        Why::Unreadable,
        Why::PidReused,
        Why::OtherProgram,
    ]
    .map(Why::code);
    assert_eq!(
        tokens,
        [
            "itself",
            "foreign-user",
            "unreadable",
            "pid-reused",
            "other-program"
        ]
    );
}
