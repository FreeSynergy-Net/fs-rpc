//! `Guarded<M>`, part 2: G5 `Ask.at` · G6 `revert` asks `change.revert` · G7 `page` fills `may` and `hidden` · G8 no known
//! person is refused (fail closed) · conflict 409 · `watch`. G1-G4 are in `guarded.rs`.
//!
//! Pattern: Test (in-process: the capability of a connection is called directly, no socket; one `LocalSet` per test).
#![allow(
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::field_reassign_with_default,
    clippy::similar_names
)]

mod guarded_support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use fs_interfaces::formsource_capnp::form_source;
use fs_rpc::Guarded;
use fs_types::Utc;
use guarded_support::client::*;
use guarded_support::{CREATE, Notes, Rules, SET};

/// G5: `Ask.at` is "now" at a living call, and a rule "the right ends at T" refuses after T.
#[test]
fn g5_ask_at_is_now_and_a_right_can_end() {
    run(async {
        // The system clock, within a second.
        let (mut g, rules) = notes();
        let alice = connect(&mut g, "alice");
        let t0 = chrono::Utc::now().timestamp_millis();
        invoke(&alice, CREATE, "", &[("text", "x")]).await;
        let at = rules.asked.lock().unwrap()[0].at.millis();
        assert!((at - t0).abs() < 1000, "at = {at}, now = {t0}");

        // An own clock: allowed before the end, refused after it.
        let now = Rc::new(Cell::new(1_000_i64));
        let clock = now.clone();
        let rules = Rules::default();
        *rules.cutoff.lock().unwrap() = Some(Utc::from_millis(2_000));
        let mut g = Guarded::with_clock(
            Notes::new(rules),
            ports(),
            Rc::new(move || Utc::from_millis(clock.get())),
        );
        let alice = connect(&mut g, "alice");
        assert!(!invoke(&alice, CREATE, "", &[("text", "a")]).await.failed);
        now.set(3_000);
        let after = invoke(&alice, CREATE, "", &[("text", "b")]).await;
        assert!(after.failed);
        assert_eq!(after.status, 403);
        assert_eq!(g.core().applied.borrow().len(), 1);
    });
}

/// G6: `revert` asks the rules with `change.revert` before the module is called.
#[test]
fn g6_revert_asks_the_rules() {
    run(async {
        let (mut g, rules) = notes();
        let bob = connect(&mut g, "bob");
        let change = "018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9eaa";
        let revert = |c: &form_source::Client| {
            let mut req = c.revert_request();
            let id = uuid::Uuid::parse_str(change).unwrap().as_u128();
            let mut u = req.get().init_change();
            u.set_hi((id >> 64) as u64);
            u.set_lo(id as u64);
            req.send().promise
        };
        let refused = revert(&bob).await.unwrap();
        let a = refused.get().unwrap();
        assert!(a.get_failed());
        assert_eq!(
            a.get_problem()
                .unwrap()
                .get_type()
                .unwrap()
                .to_str()
                .unwrap(),
            "form-source-forbidden"
        );
        assert!(g.core().applied.borrow().is_empty());
        {
            let asked = rules.asked.lock().unwrap();
            assert_eq!(asked[0].command, fs_rpc::REVERT_COMMAND);
            assert_eq!(asked[0].record.as_deref(), Some(change));
        }
        rules
            .revert_ok
            .store(true, std::sync::atomic::Ordering::SeqCst);
        introduce(&bob, "undo-app", "i").await;
        let done = revert(&bob).await.unwrap();
        let a = done.get().unwrap();
        assert!(!a.get_failed());
        assert_eq!(
            a.get_result()
                .unwrap()
                .get_text()
                .unwrap()
                .to_str()
                .unwrap(),
            format!("reverted {change} for undo-app")
        );
    });
}

/// G7: `page` fills `may` per record and for the set from the rules, and a hidden field loses its value (and `blob` is refused).
#[test]
fn g7_page_fills_may_and_hidden() {
    run(async {
        let (mut g, _) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        invoke(&alice, CREATE, "", &[("text", "t"), ("secret", "s3cret")]).await;

        let r = page(&alice).await;
        let p = r.get().unwrap().get_page().unwrap();
        let rec = p.get_records().unwrap().get(0);
        assert_eq!(
            rec.get_may().unwrap().get(0).unwrap().to_str().unwrap(),
            SET
        );
        assert_eq!(rec.get_hidden().unwrap().len(), 0);
        assert_eq!(rec.get_values().unwrap().len(), 2);

        let r = page(&bob).await;
        let p = r.get().unwrap().get_page().unwrap();
        // For the set: only what the rules allow without a record (`note.create`).
        let may: Vec<_> = p
            .get_may()
            .unwrap()
            .iter()
            .map(|c| c.unwrap().to_string().unwrap())
            .collect();
        assert_eq!(may, [CREATE]);
        let rec = p.get_records().unwrap().get(0);
        assert_eq!(rec.get_may().unwrap().len(), 0, "the stranger may not set");
        assert_eq!(rec.get_hidden().unwrap().len(), 1);
        assert_eq!(
            rec.get_hidden()
                .unwrap()
                .get(0)
                .get_path()
                .unwrap()
                .to_str()
                .unwrap(),
            "secret"
        );
        let values: Vec<_> = rec
            .get_values()
            .unwrap()
            .iter()
            .map(|v| v.get_path().unwrap().to_string().unwrap())
            .collect();
        assert_eq!(values, ["text"], "the hidden value is not sent");

        let mut req = bob.blob_request();
        req.get().set_record("note-1");
        req.get().set_field("secret");
        assert!(req.send().promise.await.is_err());
        let mut req = alice.blob_request();
        req.get().set_record("note-1");
        req.get().set_field("secret");
        let b = req.send().promise.await.unwrap();
        let size = b
            .get()
            .unwrap()
            .get_blob()
            .unwrap()
            .size_request()
            .send()
            .promise
            .await
            .unwrap();
        assert_eq!(size.get().unwrap().get_bytes(), 8);
    });
}

/// G8: a connection whose person is not known gets nothing (fail closed), `hello` aside.
#[test]
fn g8_no_known_person_is_refused_everything() {
    run(async {
        let (mut g, _) = notes();
        let stranger = connect(&mut g, "mallory");
        let hello = stranger.hello_request().send().promise.await.unwrap();
        assert_eq!(hello.get().unwrap().get_level(), 2);
        let w = invoke(&stranger, CREATE, "", &[("text", "x")]).await;
        assert_eq!((w.failed, w.status), (true, 403));
        assert!(page_err(&stranger).await);
        assert!(g.core().applied.borrow().is_empty());
    });
}

/// Conflict: `expect` no longer true is a 409 with the typed conflict in the answer.
#[test]
fn conflict_is_409_with_the_typed_conflict() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        invoke(&alice, CREATE, "", &[("text", "one")]).await;
        let ok = invoke_if(&alice, SET, "note-1", &[("text", "two")], "1").await;
        assert!(!ok.failed, "{ok:?}");
        let stale = invoke_if(&alice, SET, "note-1", &[("text", "three")], "1").await;
        assert!(stale.failed);
        assert_eq!(
            (stale.status, stale.kind.as_str()),
            (409, "form-source-conflict")
        );
        assert_eq!(stale.clash_paths, ["text"]);
        assert!(!stale.activity_zero);
        assert_eq!(stale.record, "");
    });
}

/// Watch: a change reaches the subscribers the rules allow; the stranger hears nothing; dropping ends the subscription.
#[test]
fn watch_is_filtered_by_the_rules_and_ends_when_dropped() {
    run(async {
        let (mut g, _) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        let (alice_saw, _, alice_sub) = watch(&alice).await;
        let (bob_saw, _, bob_sub) = watch(&bob).await;
        assert_eq!(g.hub().watchers(), 2);
        // Alice makes a note: the rules know the owner only after `apply`, so the watch of the new record is allowed to its owner.
        invoke(&alice, CREATE, "", &[("text", "x")]).await;
        wait_for(|| alice_saw.borrow().len() == 1).await;
        assert_eq!(alice_saw.borrow()[0], "note-1");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(bob_saw.borrow().is_empty(), "the stranger is told nothing");
        drop(alice_sub);
        drop(bob_sub);
        wait_for(|| g.hub().watchers() == 0).await;
    });
}

/// `changes` is filtered by the rules like `watch`: a stranger who claims the program of the owner reads nothing the
/// rules deny him (`claimed` is no key to read).
#[test]
fn changes_are_filtered_by_the_rules_even_with_a_foreign_claim() {
    run(async {
        let (mut g, _) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        introduce(&alice, "net.freesynergy.planning", "a").await;
        introduce(&bob, "net.freesynergy.planning", "b").await;
        assert!(!invoke(&alice, CREATE, "", &[("text", "x")]).await.failed);
        let count = |c: form_source::Client| async move {
            let r = c.changes_request().send().promise.await.unwrap();
            r.get().unwrap().get_items().unwrap().len()
        };
        assert_eq!(count(alice).await, 1);
        assert_eq!(count(bob).await, 0);
    });
}

/// A conflict does not tell the current value of a field the person may not see (a right to change is no right to see).
#[test]
fn a_conflict_does_not_leak_a_hidden_field() {
    run(async {
        let (mut g, rules) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        invoke(&alice, CREATE, "", &[("text", "t"), ("secret", "s3cret")]).await;
        rules
            .setters
            .lock()
            .unwrap()
            .push(fs_interfaces::owner::fake::person(BOB));
        let w = invoke_if(&bob, SET, "note-1", &[("secret", "guess")], "99").await;
        assert_eq!((w.failed, w.status), (true, 409), "{w:?}");
        assert_eq!(w.clash_paths, ["secret"]);
        assert_eq!(w.clash_current, [""], "the value must not travel");
        // The owner sees it.
        let w = invoke_if(&alice, SET, "note-1", &[("secret", "guess")], "99").await;
        assert_eq!(w.clash_current, ["s3cret"]);
    });
}
