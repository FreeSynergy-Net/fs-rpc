//! The typed conflict problem of `texts` round-trips over the error text (moved from the `texts` contract test of
//! `fs-interfaces`, which no longer knows `Problem`).

use fs_interfaces::texts_capnp as t;
use fs_rpc::{Problem, problem_keys as keys};

#[test]
fn the_typed_conflict_round_trips_over_the_error_text() {
    let p = Problem::new(
        t::CONFLICT_PROBLEM_TYPE,
        "Conflict",
        "conflict: text changed",
        409,
    )
    .with_ext(keys::ID, "0190-abc")
    .with_ext(keys::KIND, "text")
    .with_ext(keys::ISSUE, t::STALE_REVISION_ISSUE)
    .with_ext_u64(t::CONFLICT_REVISION_KEY, 42)
    .with_ext_u64(t::CONFLICT_EXPECTED_KEY, 41);
    let back = Problem::from_error(&p.clone().into_error()).expect("problem");
    assert_eq!(back, p);
    assert_eq!(back.problem_type, t::CONFLICT_PROBLEM_TYPE);
    assert_eq!(back.ext_u64(t::CONFLICT_REVISION_KEY), Some(42));
    assert_eq!(back.ext(keys::ISSUE), Some("stale-revision"));
    assert!(back.detail.starts_with("conflict:"));
}
