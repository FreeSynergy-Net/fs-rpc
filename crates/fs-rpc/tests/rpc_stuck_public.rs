//! `Stuck` and `WHY_TOKENS` as another crate sees them: only `fs_rpc::…` names, no access to the module inside.
//!
//! Pattern: Consumer test (the public interface is the only door). Mutants: a token left out of `WHY_TOKENS` (the list is compared by
//! value), `WHY_TOKENS` not exported (this file does not compile), a token the rule or a program says that does not read back.

use std::time::Duration;

use fs_rpc::{RpcError, RpcErrorCode, Stuck, WHY_TOKENS};

#[test]
fn the_reasons_are_a_public_list_with_exactly_these_ten_tokens() {
    assert_eq!(
        WHY_TOKENS,
        [
            "itself",
            "foreign-user",
            "unreadable",
            "pid-reused",
            "other-program",
            "same-program",
            "beating",
            "not-ending",
            "no-control",
            "unsupported",
        ]
    );
}

#[test]
fn a_program_checks_a_reason_and_every_reason_makes_the_round_trip_through_the_error() {
    for why in WHY_TOKENS {
        let mut stuck = Stuck::new(why, Duration::from_millis(1500));
        stuck.pid = Some(4242);
        stuck.can_ask = why == "same-program";
        let error = RpcError::from(&stuck);
        assert_eq!(error.code(), RpcErrorCode::ServerUnresponsive);
        assert_eq!(Stuck::from_error(&error), Some(stuck), "{why}");
    }
}

#[test]
fn the_fs_tasks_take_over_case_is_a_reason_a_consumer_can_look_up() {
    assert!(WHY_TOKENS.contains(&"same-program"));
    let mut stuck = Stuck::new("same-program", Duration::from_secs(10));
    stuck.start = Some(99);
    stuck.since = Some("2026-11-18T11:00:00Z".into());
    stuck.can_ask = true;
    let back = Stuck::from_error(&RpcError::from(&stuck)).expect("reads back");
    assert!(back.can_ask);
    assert_eq!(back.why, "same-program");
    assert_eq!(back, stuck);
}

#[cfg(all(unix, target_os = "linux"))]
#[test]
fn every_reason_the_rule_gives_is_in_the_list() {
    use fs_rpc::Why;
    for why in [
        Why::Itself,
        Why::ForeignUser,
        Why::Unreadable,
        Why::PidReused,
        Why::OtherProgram,
    ] {
        assert!(WHY_TOKENS.contains(&why.code()), "{}", why.code());
    }
}
