//! `Guarded` and `follows` (Stufe 4, B2b): `watch` stores the `Follow` per subscriber, `changes` passes form/scope on and filters with it;
//! `scrub` of a conflict judges the record of the call too; `limit` above 1000 is clamped.
//!
//! Pattern: Test (in-process: the capability of a connection is called directly, no socket; one `LocalSet` per test).
#![allow(clippy::field_reassign_with_default, clippy::unused_async_trait_impl)]

mod guarded_support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use fs_interfaces::formsource_capnp::{change_sink, form_source};
use fs_interfaces::owner::fake::person;
use guarded_support::client::*;
use guarded_support::{CREATE, SET};

struct Seen(Rc<RefCell<Vec<String>>>);
impl change_sink::Server for Seen {
    async fn changed(
        self: Rc<Self>,
        params: change_sink::ChangedParams,
    ) -> Result<(), capnp::Error> {
        let object = params.get()?.get_change()?.get_object()?.to_string()?;
        self.0.borrow_mut().push(object);
        Ok(())
    }
    async fn ended(
        self: Rc<Self>,
        _: change_sink::EndedParams,
        _: change_sink::EndedResults,
    ) -> Result<(), capnp::Error> {
        Ok(())
    }
}

/// `watch` with a form and a scope; the answer is the subscription or the exception.
async fn watch_for(
    c: &form_source::Client,
    form: &str,
    scope: &str,
) -> (
    Rc<RefCell<Vec<String>>>,
    Result<form_source::watch_results::Reader<'static>, capnp::Error>,
) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut req = c.watch_request();
    req.get().set_form(form);
    req.get().set_scope(scope);
    req.get()
        .set_sink(capnp_rpc::new_client(Seen(seen.clone())));
    let answer = req.send().promise.await.map(|r| {
        // Keep the subscription alive for the test by leaking the response (the test ends with the runtime).
        let r: &'static _ = Box::leak(Box::new(r));
        r.get().unwrap()
    });
    (seen, answer)
}

/// `follows` refuses the form: `watch` is an RPC exception (like `page`) and nothing is subscribed.
#[test]
fn watch_with_a_form_that_follows_refuses_is_an_error() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        let (_, answer) = watch_for(&alice, "other@1.0.0", "").await;
        let err = answer.err().expect("an unknown form must not subscribe");
        assert!(err.to_string().contains("form-source-unknown-form"), "{err}");
        assert_eq!(g.hub().watchers(), 0);
        let (_, answer) = watch_for(&alice, "notes@1.0.0", "").await;
        assert!(answer.is_ok());
        assert_eq!(g.hub().watchers(), 1);
    });
}

/// The `Follow` of a subscriber is kept: a subscriber of one scope hears that scope only.
#[test]
fn a_foreign_scope_hears_nothing() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        let (seen, answer) = watch_for(&alice, "notes@1.0.0", "note-2").await;
        answer.unwrap();
        invoke(&alice, CREATE, "", &[("text", "one")]).await; // note-1: not followed
        invoke(&alice, CREATE, "", &[("text", "two")]).await; // note-2: followed
        wait_for(|| !seen.borrow().is_empty()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(*seen.borrow(), ["note-2"]);
    });
}

/// `changes` passes form and scope on to the module and shows only what `follows` admits; `limit` above 1000 is clamped.
#[test]
fn changes_with_a_scope_are_filtered_and_the_limit_is_clamped() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        introduce(&alice, "net.freesynergy.planning", "a").await;
        invoke(&alice, CREATE, "", &[("text", "one")]).await;
        invoke(&alice, CREATE, "", &[("text", "two")]).await;
        let count = |scope: &'static str, limit: u32| {
            let c = alice.clone();
            async move {
                let mut req = c.changes_request();
                req.get().set_scope(scope);
                req.get().set_limit(limit);
                let r = req.send().promise.await.unwrap();
                r.get().unwrap().get_items().unwrap().len()
            }
        };
        assert_eq!(count("", 0).await, 2, "empty scope = all");
        assert_eq!(count("note-2", 0).await, 1, "only the followed scope");
        assert_eq!(count("nowhere", 0).await, 0);
        count("", 5000).await;
        let queries = g.core().queries.borrow();
        assert_eq!(queries[1].scope, "note-2", "the scope reaches the module");
        assert_eq!(queries[3].limit, 1000, "limit is clamped");
        assert_eq!(queries[2].limit, 0, "a small limit stays");
    });
}

/// `scrub` at `revert`: a conflict does not tell a field the person may not see (the `by` record is judged).
#[test]
fn a_revert_conflict_does_not_leak_a_hidden_field() {
    run(async {
        let (mut g, rules) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        invoke(&alice, CREATE, "", &[("text", "t"), ("secret", "s3cret")]).await;
        rules
            .revert_ok
            .store(true, std::sync::atomic::Ordering::SeqCst);
        g.core().revert_conflict.set(true);
        let clash = |c: form_source::Client| async move {
            let mut req = c.revert_request();
            req.get().init_change().set_lo(7);
            let r = req.send().promise.await.unwrap();
            let a = r.get().unwrap();
            assert!(a.get_failed());
            let fields = a.get_conflict().unwrap().get_fields().unwrap();
            fields
                .iter()
                .map(|f| f.get_current().unwrap().to_string().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(clash(bob).await, [""], "the value must not travel");
        assert_eq!(clash(alice).await, ["s3cret"], "the owner sees it");
    });
}

/// `scrub` at `invokeIf`: `by` names a change of ANOTHER record (one the person may see); the field is judged on the record of the call too.
#[test]
fn a_conflict_naming_another_record_does_not_leak_the_hidden_field() {
    run(async {
        let (mut g, rules) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        invoke(&alice, CREATE, "", &[("text", "t"), ("secret", "s3cret")]).await; // note-1 (alice)
        invoke(&bob, CREATE, "", &[("text", "b")]).await; // note-2 (bob: his `secret` is visible to him)
        rules.setters.lock().unwrap().push(person(BOB));
        *g.core().conflict_by.borrow_mut() = Some("note-2".into());
        let w = invoke_if(&bob, SET, "note-1", &[("secret", "guess")], "99").await;
        assert_eq!((w.failed, w.status), (true, 409), "{w:?}");
        assert_eq!(w.clash_paths, ["secret"]);
        assert_eq!(w.clash_current, [""], "judged on the record of the call");
    });
}

/// An answer heavier than the limit is `too-large` (an exception), not a broken connection.
#[test]
fn changes_heavier_than_the_answer_limit_are_too_large() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        invoke(&alice, CREATE, "", &[("text", "one")]).await;
        g.core().huge_change.set(true);
        let err = alice
            .changes_request()
            .send()
            .promise
            .await
            .err()
            .expect("too heavy");
        assert!(err.to_string().contains("too-large"), "{err}");
        g.core().huge_change.set(false);
        assert!(alice.changes_request().send().promise.await.is_ok());
    });
}
