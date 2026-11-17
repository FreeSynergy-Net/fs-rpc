//! The rule for a data folder, on made-up facts: one test per rule.
//!
//! Pattern: Table tests. Mutants (each turns its test red): M1 the symlink rule removed; M2 the not-a-directory rule removed;
//! M3 the owner rule removed (a foreign folder is tightened, not refused); M4 `Tighten` returns `mode | 0o700` or the full
//! mode (bits are given, not taken); M5 `Keep` for a folder with group or other bits (the open folder stays open).

use super::{Action, DataDirCause as Cause, Meta, decide};

const ME: u32 = 1000;

fn dir(mode: u32) -> Meta {
    Meta {
        is_dir: true,
        is_symlink: false,
        uid: ME,
        mode,
    }
}

#[test]
fn a_private_folder_of_this_user_is_kept() {
    assert_eq!(decide(&dir(0o700), ME), Action::Keep);
    assert_eq!(decide(&dir(0o500), ME), Action::Keep);
    assert_eq!(
        decide(&dir(0o2700), ME),
        Action::Keep,
        "setgid alone is no access"
    );
}

#[test]
fn an_open_folder_of_this_user_loses_group_and_other_bits_and_gains_none() {
    assert_eq!(decide(&dir(0o755), ME), Action::Tighten(0o700));
    assert_eq!(decide(&dir(0o770), ME), Action::Tighten(0o700));
    assert_eq!(
        decide(&dir(0o007), ME),
        Action::Tighten(0o000),
        "nothing is added for the owner"
    );
    assert_eq!(decide(&dir(0o544), ME), Action::Tighten(0o500));
}

#[test]
fn a_link_a_file_and_a_foreign_folder_are_refused_with_their_cause() {
    let link = Meta {
        is_symlink: true,
        ..dir(0o700)
    };
    assert_eq!(decide(&link, ME), Action::Refuse(Cause::Symlink));
    let file = Meta {
        is_dir: false,
        ..dir(0o700)
    };
    assert_eq!(decide(&file, ME), Action::Refuse(Cause::NotDirectory));
    let foreign = Meta {
        uid: 0,
        ..dir(0o755)
    };
    assert_eq!(decide(&foreign, ME), Action::Refuse(Cause::ForeignOwner));
    let other_user = Meta {
        uid: ME + 1,
        ..dir(0o700)
    };
    assert_eq!(decide(&other_user, ME), Action::Refuse(Cause::ForeignOwner));
}

#[test]
fn the_tokens_are_fixed() {
    let tokens = [
        Cause::ForeignOwner,
        Cause::Mode,
        Cause::Symlink,
        Cause::Ancestor,
        Cause::NotDirectory,
    ]
    .map(Cause::token);
    assert_eq!(
        tokens,
        [
            "foreign-owner",
            "mode",
            "symlink",
            "ancestor",
            "not-directory"
        ]
    );
}
