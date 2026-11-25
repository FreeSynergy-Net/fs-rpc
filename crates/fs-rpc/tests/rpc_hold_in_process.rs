//! `hold_in_process`: a module held in the program itself (no socket): writing, the refusal of a stranger, undo and watch.
//!
//! Pattern: Test (in-process, jobs of `Hold::call` on the thread of the hold; the module is built there, it is `!Send`).
#![allow(clippy::field_reassign_with_default, clippy::cast_possible_truncation)]

mod guarded_support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use capnp::capability::FromClientHook;
use fs_interfaces::formsource_capnp::form_source;
use fs_interfaces::module::Actor;
use fs_interfaces::owner::fake::person;
use fs_rpc::{Caller, FixedPerson, Guarded, Hold, PerConnection, Ports, hold_in_process};
use guarded_support::client::*;
use guarded_support::{CREATE, Notes, Rules, SET};

const LIMIT: Duration = Duration::from_secs(10);

fn held(rules: &Rules, who: &'static str) -> Hold {
    let rules = rules.clone();
    hold_in_process("test", move || {
        let ports = Ports::new(|_: &Caller| Actor::Person, FixedPerson(person(who)));
        let mut g = Guarded::new(Notes::new(rules), ports);
        g.bootstrap(&Caller::new("in-process", None))
    })
    .unwrap()
}

/// Runs `f` on the thread of the hold with the `FormSource` of the module.
fn call<T: Send + 'static, F>(
    hold: &Hold,
    f: impl FnOnce(form_source::Client) -> F + Send + 'static,
) -> T
where
    F: std::future::Future<Output = T> + 'static,
{
    hold.call(
        Box::new(move |c| Box::pin(async move { Ok(f(FromClientHook::new(c.hook)).await) })),
        LIMIT,
    )
    .unwrap()
}

/// The fixed person writes without a socket; a stranger (another fixed person, same rules) is refused with 403.
#[test]
fn the_owner_writes_in_process_and_the_stranger_is_refused() {
    let rules = Rules::default();
    let (alice, bob) = (held(&rules, ALICE), held(&rules, BOB));
    let made = call(&alice, |c| async move {
        let w = invoke_if(&c, CREATE, "", &[("text", "hi")], "").await;
        (w.failed, w.record)
    });
    assert_eq!(made, (false, "note-1".to_owned()));
    let own = call(&alice, |c| async move {
        invoke_if(&c, SET, "note-1", &[("text", "mine")], "")
            .await
            .failed
    });
    assert!(!own);
    let refused = call(&bob, |c| async move {
        let w = invoke_if(&c, SET, "note-1", &[("text", "stolen")], "").await;
        (w.failed, w.status, w.kind)
    });
    assert_eq!(refused, (true, 403, "form-source-forbidden".to_owned()));
}

/// Undo asks the rules (`change.revert`) and then runs for the program that introduced itself.
#[test]
fn undo_runs_in_process_after_the_rules_allow_it() {
    let rules = Rules::default();
    rules.revert_ok.store(true, Ordering::SeqCst);
    let hold = held(&rules, BOB);
    let change = "018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9eaa";
    let text = call(&hold, move |c| async move {
        introduce(&c, "undo-app", "i").await;
        let mut req = c.revert_request();
        let id = uuid::Uuid::parse_str(change).unwrap().as_u128();
        let mut u = req.get().init_change();
        u.set_hi((id >> 64) as u64);
        u.set_lo(id as u64);
        let r = req.send().promise.await.unwrap();
        let a = r.get().unwrap();
        (
            a.get_failed(),
            a.get_result()
                .unwrap()
                .get_text()
                .unwrap()
                .to_string()
                .unwrap(),
        )
    });
    assert_eq!(text, (false, format!("reverted {change} for undo-app")));
}

/// A change of the fixed person reaches its own `watch`.
#[test]
fn watch_hears_the_change_in_process() {
    let rules = Rules::default();
    let hold = held(&rules, ALICE);
    let seen = call(&hold, |c| async move {
        let (seen, _, _sub) = watch(&c).await;
        invoke(&c, CREATE, "", &[("text", "x")]).await;
        wait_for(|| seen.borrow().len() == 1).await;
        seen.take()
    });
    assert_eq!(seen, vec!["note-1".to_owned()]);
}

/// Dropping the handle ends the thread and does not hang.
#[test]
fn dropping_the_handle_ends_the_thread() {
    let rules = Rules::default();
    drop(held(&rules, ALICE));
}
