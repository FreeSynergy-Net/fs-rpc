//! `Guarded<M>`, part 1: the wrapper that asks the rules of a module BEFORE every change (Stufe 4, part 2; G1-G4 of the draft).
//!
//! Pattern: Test (in-process: the capability of a connection is called directly, no socket; one `LocalSet` per test).
//! G1 only the owner changes (fake of fs-interfaces) · G2 origin from `introduce`, person from the connection · G3 two
//! connections, two claims · G4 `introduce` changes no right. G5-G8, conflict and `watch` are in `guarded_more.rs`.
#![allow(
    clippy::many_single_char_names,
    clippy::field_reassign_with_default,
    clippy::similar_names
)]

mod guarded_support;

use fs_interfaces::module::fake::{CREATE as FAKE_CREATE, OwnerOnlyModule, SET as FAKE_SET};
use fs_interfaces::owner::fake::person;
use fs_rpc::Guarded;
use guarded_support::client::*;
use guarded_support::{CREATE, SET};

/// G1: a refused call never reaches `apply`; the owner may.
#[test]
fn g1_only_the_owner_changes_and_apply_is_not_called_for_the_stranger() {
    run(async {
        let mut g = Guarded::new(OwnerOnlyModule::new(), ports());
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        let made = invoke(&alice, FAKE_CREATE, "", &[("text", "hello")]).await;
        assert!(!made.failed);
        assert_eq!(made.record, "note-1");
        assert_eq!(g.core().applied(), 1);

        let refused = invoke(&bob, FAKE_SET, "note-1", &[("text", "mine now")]).await;
        assert!(refused.failed);
        assert_eq!(refused.kind, "form-source-forbidden");
        assert_eq!(refused.status, 403);
        assert!(refused.record.is_empty(), "a refused write names no record");
        let refused2 = invoke_if(&bob, FAKE_SET, "note-1", &[("text", "x")], "").await;
        assert_eq!((refused2.failed, refused2.status), (true, 403));
        assert_eq!(
            g.core().applied(),
            1,
            "apply was never called for the stranger"
        );

        let allowed = invoke_if(&alice, FAKE_SET, "note-1", &[("text", "changed")], "").await;
        assert!(!allowed.failed, "{allowed:?}");
        assert_eq!(g.core().applied(), 2);
    });
}

/// G2: an allowed change carries the origin: program from `introduce`, person from the connection (never from an argument).
#[test]
fn g2_origin_program_from_introduce_person_from_the_connection() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        introduce(&alice, "net.freesynergy.planning", "win-7").await;
        // An argument that claims to be somebody else changes nothing.
        let w = invoke_if(&alice, CREATE, "", &[("text", "hi"), ("person", BOB)], "").await;
        assert!(!w.failed, "{w:?}");
        assert_eq!(w.program.as_deref(), Some("net.freesynergy.planning"));
        assert_eq!(w.origin_person.as_deref(), Some(ALICE));
        assert!(!w.activity_zero);
        let cx = g.core().applied.borrow()[0].clone();
        assert_eq!(cx.person, person(ALICE));
        assert_eq!(cx.claimed.instance, "win-7");
        // `changes` of the asking program finds it.
        let r = alice.changes_request().send().promise.await.unwrap();
        assert_eq!(r.get().unwrap().get_items().unwrap().len(), 1);
    });
}

/// G3: two connections, two claims; no global state; a connection without `introduce` has an empty claim.
#[test]
fn g3_two_connections_two_claims() {
    run(async {
        let (mut g, _) = notes();
        let (a1, a2, a3) = (
            connect(&mut g, "alice"),
            connect(&mut g, "alice"),
            connect(&mut g, "alice"),
        );
        introduce(&a1, "one", "i1").await;
        introduce(&a2, "two", "i2").await;
        introduce(&a2, "three", "i2").await; // the second value holds
        for c in [&a1, &a2, &a3] {
            assert!(!invoke(c, CREATE, "", &[("text", "x")]).await.failed);
        }
        let seen: Vec<String> = g
            .core()
            .applied
            .borrow()
            .iter()
            .map(|c| c.claimed.program.clone())
            .collect();
        assert_eq!(seen, ["one", "three", ""]);
    });
}

/// G4: `introduce` with a big name changes no right: the same answer, and the module sees the same person and actor.
#[test]
fn g4_introduce_changes_no_right() {
    run(async {
        let (mut g, rules) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        invoke(&alice, CREATE, "", &[("text", "mine")]).await;
        let before = invoke(&bob, SET, "note-1", &[("text", "x")]).await;
        introduce(&bob, "settings", "i").await;
        let after = invoke(&bob, SET, "note-1", &[("text", "x")]).await;
        assert_eq!(
            (before.failed, before.status, &before.kind),
            (after.failed, after.status, &after.kind)
        );
        assert_eq!(after.status, 403);
        let last = rules.asked.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.person, person(BOB));
        assert_eq!(
            last.claimed_program, "settings",
            "the rules can see the claim; they must not use it"
        );
        // A claim that is too long is refused, and the old claim stays.
        let mut req = bob.introduce_request();
        req.get().set_program("p".repeat(300));
        assert!(req.send().promise.await.is_err());
        // ... and the refused introduce does not wipe the old claim: bob still claims "settings" afterwards.
        invoke(&bob, SET, "note-1", &[("text", "x")]).await;
        let last = rules.asked.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.claimed_program, "settings", "the old claim stays");
    });
}
