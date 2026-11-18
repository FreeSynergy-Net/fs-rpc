//! Who decides about a server that was not ended: the setting, the surface, the reasons that are asked about.
//! Pattern: Table tests over (setting, surface, reason) + Mutant-proof (each cell by its own value: a default that flips, a detection that
//! flips, a setting that is ignored, a choice that is missing fail a cell).

use std::time::Duration;

use super::ask::{AskPolicy, SingleOptions, surface_from};
use super::stuck::{CHOICE_TOKENS, Choices, Stuck, WHY_TOKENS};
use crate::{RpcError, RpcErrorCode};

fn stuck(why: &'static str) -> Stuck {
    Stuck::new(why, Duration::from_secs(3))
}

#[test]
fn the_default_is_to_ask_the_person() {
    assert_eq!(SingleOptions::default().ask, AskPolicy::Ask);
    assert_eq!(AskPolicy::from(true), AskPolicy::Ask);
    assert_eq!(AskPolicy::from(false), AskPolicy::Never);
}

#[test]
fn the_choices_are_wait_replace_abort() {
    assert_eq!(CHOICE_TOKENS, ["wait", "replace", "abort"]);
}

#[test]
fn with_a_surface_and_the_setting_on_no_control_and_not_ending_are_a_question_with_three_choices() {
    let options = SingleOptions::new(AskPolicy::Ask, true);
    for why in ["no-control", "not-ending"] {
        let s = options.mark(stuck(why));
        assert!(s.can_ask, "{why}");
        assert_eq!(s.choices.tokens(), ["wait", "replace", "abort"], "{why}");
        let e = RpcError::from(&s);
        assert_eq!(e.code(), RpcErrorCode::ServerUnresponsive);
        assert_eq!(
            e.param("can-ask").and_then(crate::Param::as_text),
            Some("yes")
        );
        assert_eq!(
            e.param("choices").and_then(crate::Param::as_text),
            Some("wait,replace,abort")
        );
        assert_eq!(Stuck::from_error(&e), Some(s));
    }
}

#[test]
fn no_surface_or_the_setting_off_is_never_a_question() {
    for options in [
        SingleOptions::new(AskPolicy::Ask, false),
        SingleOptions::new(AskPolicy::Never, true),
        SingleOptions::new(AskPolicy::Never, false),
    ] {
        for why in WHY_TOKENS {
            let s = options.mark(stuck(why));
            assert!(
                !s.can_ask && s.choices == Choices::None,
                "{options:?} {why}"
            );
        }
    }
}

#[test]
fn the_other_reasons_are_not_a_question_even_with_a_surface() {
    let options = SingleOptions::new(AskPolicy::Ask, true);
    for why in WHY_TOKENS {
        if why == "no-control" || why == "not-ending" {
            continue;
        }
        let s = options.mark(stuck(why));
        assert_eq!(s, stuck(why), "{why}: unchanged");
    }
}

#[test]
fn marking_changes_nothing_but_can_ask_and_choices() {
    let mut s = stuck("no-control");
    s.pid = Some(7);
    s.start = Some(8);
    s.since = Some("2026-11-18T10:00:00Z".into());
    s.exe = Some("/usr/bin/fs-x".into());
    let marked = SingleOptions::new(AskPolicy::Ask, true).mark(s.clone());
    assert_eq!(
        marked,
        Stuck {
            can_ask: true,
            choices: Choices::WaitReplaceAbort,
            ..s
        }
    );
}

#[test]
fn a_surface_is_a_terminal_or_a_named_display() {
    assert!(surface_from(true, None, None));
    assert!(surface_from(false, Some(":0"), None));
    assert!(surface_from(false, None, Some("wayland-0")));
    assert!(!surface_from(false, None, None));
    assert!(
        !surface_from(false, Some(""), Some("")),
        "an empty name is none"
    );
}

#[test]
fn a_list_of_choices_that_is_none_reads_back_as_nothing() {
    let known = RpcError::from(&stuck("no-control"));
    let bad = known.with_text("choices", "wait,replace");
    assert_eq!(Stuck::from_error(&bad), None);
    let empty = RpcError::from(&stuck("no-control")).with_text("choices", "");
    assert_eq!(Stuck::from_error(&empty), None);
}
