//! The start-failure description: code, parameters and the way back, for every variant of `SingleError`.
//! Pattern: Value-object tests (a fully set value, a bare one, every reason) + Mutant-proof (each field and each token is
//! checked by its own value, so a dropped field, a swapped code or a changed sign fails a test).

use std::time::Duration;

use super::error::{SingleError, Stuck};
use super::stuck::WHY_TOKENS;
use crate::{Param, RpcError, RpcErrorCode};

fn full() -> Stuck {
    Stuck {
        pid: Some(4_000_000_000), // above i32::MAX: a signed type would turn it negative
        start: Some(123_456_789),
        since: Some("2026-11-18T10:00:00Z".into()),
        exe: Some("/usr/bin/fs-tasks".into()),
        can_ask: true,
        ..Stuck::new("beating", Duration::from_millis(2500))
    }
}

fn text(e: &RpcError, key: &str) -> Option<String> {
    e.param(key).and_then(Param::as_text).map(str::to_owned)
}

fn number(e: &RpcError, key: &str) -> Option<u64> {
    e.param(key).and_then(Param::as_number)
}

#[test]
fn a_stuck_server_is_the_code_server_unresponsive_with_every_field_as_a_parameter() {
    let e = RpcError::from(&full());
    assert_eq!(e.code(), RpcErrorCode::ServerUnresponsive);
    assert_eq!(e.code().as_str(), "server-unresponsive");
    assert_eq!(e.message_key(), "rpc-error-server-unresponsive");
    assert_eq!(number(&e, "pid"), Some(4_000_000_000));
    assert_eq!(number(&e, "start"), Some(123_456_789));
    assert_eq!(text(&e, "since").as_deref(), Some("2026-11-18T10:00:00Z"));
    assert_eq!(text(&e, "exe").as_deref(), Some("/usr/bin/fs-tasks"));
    assert_eq!(text(&e, "can-ask").as_deref(), Some("yes"));
    assert_eq!(text(&e, "why").as_deref(), Some("beating"));
    assert_eq!(number(&e, "limit-ms"), Some(2500));
    assert_eq!(e.params().count(), 7, "an unexpected parameter");
}

#[test]
fn what_is_not_known_is_left_out_and_never_a_sentence() {
    let e = RpcError::from(&Stuck::new("unreadable", Duration::from_secs(1)));
    for key in ["pid", "start", "since", "exe", "can-ask"] {
        assert!(e.param(key).is_none(), "{key} was set without a value");
    }
    assert_eq!(text(&e, "why").as_deref(), Some("unreadable"));
    assert_eq!(number(&e, "limit-ms"), Some(1000));
    assert_eq!(e.params().count(), 2);
}

#[test]
fn the_description_survives_the_way_through_the_error_and_back() {
    for stuck in [full(), Stuck::new("itself", Duration::from_millis(1))] {
        let back = Stuck::from_error(&RpcError::from(&stuck));
        assert_eq!(back.as_ref(), Some(&stuck));
    }
}

#[test]
fn every_reason_keeps_its_token_through_the_error() {
    assert_eq!(WHY_TOKENS.len(), 10);
    for why in WHY_TOKENS {
        let e = RpcError::from(&Stuck::new(why, Duration::from_secs(1)));
        assert_eq!(text(&e, "why").as_deref(), Some(why));
        assert_eq!(Stuck::from_error(&e).unwrap().why, why);
    }
    for why in ["same-program", "beating", "not-ending", "no-control"] {
        assert!(WHY_TOKENS.contains(&why), "{why} is not a reason");
    }
}

#[test]
fn a_process_number_above_the_signed_range_is_kept_as_it_is() {
    let stuck = Stuck {
        pid: Some(u32::MAX),
        ..Stuck::new("foreign-user", Duration::from_secs(1))
    };
    let e = RpcError::from(&stuck);
    assert_eq!(number(&e, "pid"), Some(u64::from(u32::MAX)));
    assert_eq!(Stuck::from_error(&e).unwrap().pid, Some(u32::MAX));
}

#[test]
fn an_error_that_is_not_a_stuck_server_is_not_read_as_one() {
    let wrong_code = RpcError::new(RpcErrorCode::ServerBusy)
        .with_text("why", "beating")
        .with_number("limit-ms", 1);
    assert_eq!(Stuck::from_error(&wrong_code), None);
    let ok = || {
        RpcError::new(RpcErrorCode::ServerUnresponsive)
            .with_text("why", "itself")
            .with_number("limit-ms", 1)
    };
    assert!(Stuck::from_error(&ok()).is_some());
    let unknown_why = RpcError::new(RpcErrorCode::ServerUnresponsive)
        .with_text("why", "bored")
        .with_number("limit-ms", 1);
    assert_eq!(Stuck::from_error(&unknown_why), None);
    let no_why = RpcError::new(RpcErrorCode::ServerUnresponsive).with_number("limit-ms", 1);
    assert_eq!(Stuck::from_error(&no_why), None);
    let no_limit = RpcError::new(RpcErrorCode::ServerUnresponsive).with_text("why", "itself");
    assert_eq!(Stuck::from_error(&no_limit), None);
    let too_big_pid = ok().with_number("pid", u64::from(u32::MAX) + 1);
    assert_eq!(Stuck::from_error(&too_big_pid), None);
    let pid_as_text = ok().with_text("pid", "12");
    assert_eq!(Stuck::from_error(&pid_as_text), None);
}

#[test]
fn can_ask_is_yes_or_left_out_and_only_yes_reads_back_as_true() {
    let asked = RpcError::from(&full());
    assert!(Stuck::from_error(&asked).unwrap().can_ask);
    let plain = RpcError::from(&Stuck::new("itself", Duration::from_secs(1)));
    assert!(!Stuck::from_error(&plain).unwrap().can_ask);
    let no = RpcError::new(RpcErrorCode::ServerUnresponsive)
        .with_text("why", "itself")
        .with_text("can-ask", "no")
        .with_number("limit-ms", 1);
    assert!(!Stuck::from_error(&no).unwrap().can_ask);
}

#[test]
fn the_variants_of_the_single_error_each_give_their_own_code_and_parameters() {
    let silent = RpcError::from(&SingleError::Unresponsive(full()));
    assert_eq!(silent, RpcError::from(&full()));

    let busy = RpcError::from(&SingleError::Busy {
        pid: Some(77),
        what: "importing".into(),
        progress: 5,
    });
    assert_eq!(busy.code(), RpcErrorCode::ServerBusy);
    assert_eq!(number(&busy, "pid"), Some(77));
    assert_eq!(text(&busy, "what").as_deref(), Some("importing"));
    assert_eq!(number(&busy, "progress"), Some(5));
    let busy_no_pid = RpcError::from(&SingleError::Busy {
        pid: None,
        what: "x".into(),
        progress: 0,
    });
    assert!(busy_no_pid.param("pid").is_none());

    let long = RpcError::from(&SingleError::TooLong(Duration::from_secs(30)));
    assert_eq!(long.code(), RpcErrorCode::Timeout);
    assert_eq!(number(&long, "limit-ms"), Some(30_000));
    assert_eq!(long.params().count(), 1);

    // Busy and silent are told apart by code, never by a parameter.
    assert_ne!(silent.code(), busy.code());
    assert_ne!(busy.code(), long.code());
}

#[test]
fn the_take_over_case_of_fs_tasks_survives_the_way_back() {
    // fs-tasks (`Decision::TakeOver`): this program, this user, nothing unclear: the person may be asked.
    let s = Stuck {
        pid: Some(4242),
        start: Some(99),
        since: Some("2026-11-18T11:00:00Z".into()),
        can_ask: true,
        ..Stuck::new("same-program", Duration::from_secs(10))
    };
    let e = RpcError::from(&s);
    assert_eq!(text(&e, "why").as_deref(), Some("same-program"));
    assert_eq!(text(&e, "can-ask").as_deref(), Some("yes"));
    assert_eq!(Stuck::from_error(&e), Some(s));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "is not one of WHY_TOKENS")]
fn a_reason_that_would_not_read_back_is_refused_when_the_error_is_made() {
    let _ = RpcError::from(&Stuck::new("bored", Duration::from_secs(1)));
}
