//! `Guarded<M>`: the wrapper that asks the rules of a module BEFORE every change (Stufe 4, part 2; G1-G8 of the draft).
//!
//! Pattern: Test (in-process: the capability of a connection is called directly, no socket; one `LocalSet` per test).
//! G1 only the owner changes (fake of fs-interfaces) · G2 origin from `introduce`, person from the connection · G3 two
//! connections, two claims · G4 `introduce` changes no right · G5 `Ask.at` · G6 `revert` asks `change.revert` · G7 `page`
//! fills `may` and `hidden` · G8 no known person: refused (fail closed); then conflict 409 and `watch`.

#![allow(
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::field_reassign_with_default,
    clippy::unused_async_trait_impl,
    clippy::similar_names
)]

mod guarded_support;

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use capnp::capability::FromClientHook;
use fs_interfaces::formsource_capnp::{change_sink, form_source};
use fs_interfaces::module::fake::{CREATE as FAKE_CREATE, OwnerOnlyModule, SET as FAKE_SET};
use fs_interfaces::module::{Actor, ModuleCore};
use fs_interfaces::owner::fake::person;
use fs_rpc::{Caller, Guarded, PerConnection, Ports};
use fs_types::Utc;
use guarded_support::{CREATE, Notes, Rules, SET};

const ALICE: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e01";
const BOB: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e02";

fn run<F: Future>(f: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, f)
}

/// The system id tells the person: `alice`, `bob`; anybody else is not known.
fn ports() -> Ports {
    Ports::new(
        |_: &Caller| Actor::Person,
        |c: &Caller| match c.id() {
            "alice" => Some(person(ALICE)),
            "bob" => Some(person(BOB)),
            _ => None,
        },
    )
}

fn connect<M: ModuleCore>(g: &mut Guarded<M>, who: &str) -> form_source::Client {
    FromClientHook::new(g.bootstrap(&Caller::new(who, None)).hook)
}

/// What a write answered.
#[derive(Debug, Default)]
struct Wrote {
    failed: bool,
    kind: String,
    status: u16,
    record: String,
    text: String,
    program: Option<String>,
    origin_person: Option<String>,
    activity_zero: bool,
    clash_paths: Vec<String>,
}

fn args(
    req: &mut capnp::capability::Request<
        form_source::invoke_if_params::Owned,
        form_source::invoke_if_results::Owned,
    >,
    list: &[(&str, &str)],
) {
    let mut out = req.get().init_args(list.len() as u32);
    for (i, (p, t)) in list.iter().enumerate() {
        let mut e = out.reborrow().get(i as u32);
        e.set_path(p);
        e.set_text(t);
    }
}

async fn invoke_if(
    c: &form_source::Client,
    cmd: &str,
    record: &str,
    list: &[(&str, &str)],
    expect: &str,
) -> Wrote {
    let mut req = c.invoke_if_request();
    req.get().set_command(cmd);
    req.get().set_record(record);
    req.get().set_expect(expect);
    args(&mut req, list);
    let r = req
        .send()
        .promise
        .await
        .expect("invokeIf is answered, a refusal is in the answer");
    let a = r.get().unwrap();
    let mut w = Wrote {
        failed: a.get_failed(),
        ..Wrote::default()
    };
    let p = a.get_problem().unwrap();
    w.kind = p.get_type().unwrap().to_string().unwrap();
    w.status = p.get_status();
    let res = a.get_result().unwrap();
    w.record = res.get_record().unwrap().to_string().unwrap();
    w.text = res.get_text().unwrap().to_string().unwrap();
    if res.has_change() {
        let ch = res.get_change().unwrap();
        let o = ch.get_origin().unwrap();
        w.program = Some(o.get_program().unwrap().to_string().unwrap());
        w.origin_person = Some(o.get_person().unwrap().to_string().unwrap());
        w.activity_zero = ch.get_activity().unwrap().get_lo() == 0;
    }
    if a.has_conflict() {
        let c = a.get_conflict().unwrap();
        w.activity_zero = c.get_by().unwrap().get_activity().unwrap().get_lo() == 0;
        w.clash_paths = c
            .get_fields()
            .unwrap()
            .iter()
            .map(|f| f.get_path().unwrap().to_string().unwrap())
            .collect();
    }
    w
}

/// Level 1 `invoke`.
async fn invoke(c: &form_source::Client, cmd: &str, record: &str, list: &[(&str, &str)]) -> Wrote {
    let mut req = c.invoke_request();
    req.get().set_command(cmd);
    req.get().set_record(record);
    let mut out = req.get().init_args(list.len() as u32);
    for (i, (p, t)) in list.iter().enumerate() {
        let mut e = out.reborrow().get(i as u32);
        e.set_path(p);
        e.set_text(t);
    }
    let r = req.send().promise.await.unwrap();
    let a = r.get().unwrap();
    Wrote {
        failed: a.get_failed(),
        kind: a
            .get_problem()
            .unwrap()
            .get_type()
            .unwrap()
            .to_string()
            .unwrap(),
        status: a.get_problem().unwrap().get_status(),
        record: a
            .get_result()
            .unwrap()
            .get_record()
            .unwrap()
            .to_string()
            .unwrap(),
        ..Wrote::default()
    }
}

async fn introduce(c: &form_source::Client, program: &str, instance: &str) {
    let mut req = c.introduce_request();
    req.get().set_program(program);
    req.get().set_instance(instance);
    req.get().set_device("dev-1");
    req.send().promise.await.unwrap();
}

fn notes() -> (Guarded<Notes>, Rules) {
    let rules = Rules::default();
    (Guarded::new(Notes::new(rules.clone()), ports()), rules)
}

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
    });
}

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

async fn page(
    c: &form_source::Client,
) -> capnp::capability::Response<form_source::page_results::Owned> {
    let mut req = c.page_request();
    req.get().set_form("notes@1.0.0");
    req.send().promise.await.unwrap()
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

async fn page_err(c: &form_source::Client) -> bool {
    let mut req = c.page_request();
    req.get().set_form("notes@1.0.0");
    req.send().promise.await.is_err()
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

struct Sink {
    seen: Rc<RefCell<Vec<String>>>,
    ended: Rc<Cell<bool>>,
}
impl change_sink::Server for Sink {
    async fn changed(
        self: Rc<Self>,
        params: change_sink::ChangedParams,
    ) -> Result<(), capnp::Error> {
        let object = params.get()?.get_change()?.get_object()?.to_string()?;
        self.seen.borrow_mut().push(object);
        Ok(())
    }
    async fn ended(
        self: Rc<Self>,
        _: change_sink::EndedParams,
        _: change_sink::EndedResults,
    ) -> Result<(), capnp::Error> {
        self.ended.set(true);
        Ok(())
    }
}

async fn wait_for(what: impl Fn() -> bool) {
    for _ in 0..200 {
        if what() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("did not happen in time");
}

async fn watch(
    c: &form_source::Client,
) -> (
    Rc<RefCell<Vec<String>>>,
    Rc<Cell<bool>>,
    capnp::capability::Response<form_source::watch_results::Owned>,
) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let ended = Rc::new(Cell::new(false));
    let mut req = c.watch_request();
    req.get().set_form("notes@1.0.0");
    req.get().set_sink(capnp_rpc::new_client(Sink {
        seen: seen.clone(),
        ended: ended.clone(),
    }));
    (seen, ended, req.send().promise.await.unwrap())
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
