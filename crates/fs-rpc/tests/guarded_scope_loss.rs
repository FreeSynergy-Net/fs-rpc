//! `Guarded`, Stufe 4 V2: the scope is resolved to its fixed id BEFORE `follows`/`changes` (unknown = exception); a lost or ended feed
//! ends every `watch` (`WatchHub::end_all`); `scrub` fails closed on `FieldClash::record`.
//!
//! Pattern: Test (in-process: the capability of a connection is called directly, no socket; one `LocalSet` per test).
#![allow(clippy::field_reassign_with_default, clippy::unused_async_trait_impl)]

mod guarded_support;

use std::time::Duration;

use fs_interfaces::module::FeedLoss;
use fs_interfaces::owner::fake::person;
use guarded_support::client::*;
use guarded_support::scope::ALIAS;
use guarded_support::{CREATE, SET};

/// An unknown scope is an exception at `changes` and at `watch`, before the module is asked for changes or anything is subscribed.
#[test]
fn an_unknown_scope_is_an_exception_at_changes_and_watch() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        let mut req = alice.changes_request();
        req.get().set_scope("nowhere");
        let Err(err) = req.send().promise.await else {
            panic!("an unknown scope must not give an empty page");
        };
        assert!(
            err.to_string().contains("form-source-unknown-scope"),
            "{err}"
        );
        assert!(
            g.core().queries.borrow().is_empty(),
            "the module was not asked"
        );
        let (_, _, answer) = watch_in(&alice, "nowhere").await;
        assert!(answer.is_err(), "an unknown scope must not subscribe");
        assert_eq!(g.hub().watchers(), 0);
    });
}

/// `follows` and the subscription get the id `resolve_scope` returned, not the text of the caller (a rename must not move a subscription).
#[test]
fn watch_follows_the_resolved_id() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        let (seen, _, answer) = watch_in(&alice, ALIAS).await;
        let _subscription = answer.unwrap();
        assert_eq!(
            *g.core().spy.followed.borrow(),
            ["note-2"],
            "follows gets the id"
        );
        invoke(&alice, CREATE, "", &[("text", "one")]).await; // note-1: not followed
        invoke(&alice, CREATE, "", &[("text", "two")]).await; // note-2: followed
        wait_for(|| !seen.borrow().is_empty()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(*seen.borrow(), ["note-2"]);
    });
}

/// A lagged feed ends EVERY subscription (`ended`), a new `watch` listens again; the break sink was registered BEFORE the feed sink.
#[test]
fn a_lagged_feed_ends_all_watches_and_a_new_watch_listens() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        assert_eq!(*g.core().spy.calls.borrow(), ["break", "feed"]);
        let (_, first, a) = watch_in(&alice, "").await;
        let (_, second, b) = watch_in(&alice, ALIAS).await;
        let _keep = (a.unwrap(), b.unwrap());
        g.core().spy.lose(FeedLoss::Lagged { missed: 3 });
        wait_for(|| first.get() && second.get()).await;
        let (seen, ended, answer) = watch_in(&alice, "").await;
        let _again = answer.unwrap();
        invoke(&alice, CREATE, "", &[("text", "one")]).await;
        wait_for(|| !seen.borrow().is_empty()).await;
        assert!(!ended.get(), "the new subscription lives");
    });
}

/// A closed feed ends the subscriptions, and a new `watch` is refused (503): nothing could ever be told to it.
#[test]
fn a_closed_feed_ends_the_watches_and_a_new_watch_is_503() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        let (_, ended, answer) = watch_in(&alice, "").await;
        let _keep = answer.unwrap();
        g.core().spy.lose(FeedLoss::Closed);
        wait_for(|| ended.get()).await;
        let (_, _, answer) = watch_in(&alice, "").await;
        let Err(err) = answer else {
            panic!("a closed feed takes no new subscriber");
        };
        assert!(err.to_string().contains("feed-closed"), "{err}");
        assert!(err.to_string().contains("503"), "{err}");
    });
}

/// Bob owns `note-2` (his `secret` is visible to him), may also set `note-1` (Alice's), and meets a conflict on `note-2`.
async fn bobs_clash(
    record_named: Option<&str>,
    expect: &str,
) -> (Vec<String>, guarded_support::Rules) {
    let (mut g, rules) = notes();
    let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
    invoke(&alice, CREATE, "", &[("text", "t"), ("secret", "s3cret")]).await; // note-1
    invoke(&bob, CREATE, "", &[("text", "b"), ("secret", "bobs")]).await; // note-2
    rules.setters.lock().unwrap().push(person(BOB));
    *g.core().spy.clash_record.borrow_mut() = record_named.map(Into::into);
    let w = invoke_if(&bob, SET, "note-2", &[("secret", "guess")], expect).await;
    assert_eq!((w.failed, w.status), (true, 409), "{w:?}");
    (w.clash_current, rules)
}

/// The module names no record for the field: fail closed, `current` is emptied although the field is visible to the person.
#[test]
fn a_clash_without_a_record_is_emptied() {
    run(async {
        let (current, _) = bobs_clash(Some(""), "99").await;
        assert_eq!(current, [""]);
    });
}

/// The module names a record the person may see: `current` is kept.
#[test]
fn a_clash_on_a_visible_record_keeps_current() {
    run(async {
        let (current, _) = bobs_clash(None, "99").await;
        assert_eq!(current, ["bobs"]);
    });
}

/// The module names ANOTHER record, where the field is hidden for the person (`by` and the call are on `note-2`): emptied.
#[test]
fn a_clash_on_a_hidden_record_is_emptied() {
    run(async {
        let (current, _) = bobs_clash(Some("note-1"), "99").await;
        assert_eq!(current, [""]);
    });
}
