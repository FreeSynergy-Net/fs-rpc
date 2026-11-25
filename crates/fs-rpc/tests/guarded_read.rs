//! `Guarded<M>`, the read right: `page` and `blob` ask the rules for `form.read` BEFORE the module answers (Stufe 4, C1).
//!
//! Pattern: Test (in-process, one `LocalSet` per test). Mutant: without the question in `screen` / `read_blob` the
//! refused record is in the page and the blob is served - both tests turn red.
#![allow(clippy::field_reassign_with_default)]

mod guarded_support;

use fs_rpc::{Problem, READ_COMMAND};
use guarded_support::client::*;
use guarded_support::{CREATE, Rules};

async fn two_notes(
    rules: &Rules,
    g: &mut fs_rpc::Guarded<guarded_support::Notes>,
) -> fs_interfaces::formsource_capnp::form_source::Client {
    let alice = connect(g, "alice");
    invoke(&alice, CREATE, "", &[("text", "one"), ("secret", "s1")]).await;
    invoke(&alice, CREATE, "", &[("text", "two"), ("secret", "s2")]).await;
    rules.asked.lock().unwrap().clear();
    alice
}

async fn ids(c: &fs_interfaces::formsource_capnp::form_source::Client) -> Vec<String> {
    let r = page(c).await;
    let p = r.get().unwrap().get_page().unwrap();
    p.get_records()
        .unwrap()
        .iter()
        .map(|r| r.get_id().unwrap().to_string().unwrap())
        .collect()
}

async fn blob_status(
    c: &fs_interfaces::formsource_capnp::form_source::Client,
    record: &str,
) -> Option<u16> {
    let mut req = c.blob_request();
    req.get().set_record(record);
    req.get().set_field("secret");
    match req.send().promise.await {
        Ok(_) => None,
        Err(e) => Some(Problem::from_error(&e).expect("a problem").status),
    }
}

/// `page`: every record is asked `form.read` BEFORE the answer; a refused record is not in it.
#[test]
fn page_strikes_the_record_the_rules_refuse() {
    run(async {
        let (mut g, rules) = notes();
        let alice = two_notes(&rules, &mut g).await;
        assert_eq!(ids(&alice).await, ["note-1", "note-2"], "allowed");
        let asked: Vec<_> = rules
            .asked
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.command == READ_COMMAND)
            .map(|a| a.record.clone())
            .collect();
        assert_eq!(asked, [Some("note-1".into()), Some("note-2".into())]);

        rules.unreadable.lock().unwrap().push("note-1".into());
        assert_eq!(
            ids(&alice).await,
            ["note-2"],
            "refused: the record is not there"
        );
    });
}

/// `blob`: the rules are asked first; a refusal is 403 and the module is not read.
#[test]
fn blob_is_refused_with_403_when_the_rules_refuse_the_read() {
    run(async {
        let (mut g, rules) = notes();
        let alice = two_notes(&rules, &mut g).await;
        assert_eq!(blob_status(&alice, "note-1").await, None, "allowed");
        rules.unreadable.lock().unwrap().push("note-1".into());
        assert_eq!(blob_status(&alice, "note-1").await, Some(403), "refused");
        assert_eq!(blob_status(&alice, "note-2").await, None, "another record");
    });
}

async fn page_of(
    c: &fs_interfaces::formsource_capnp::form_source::Client,
    cursor: &str,
) -> Result<(Vec<String>, String, bool, bool), capnp::Error> {
    let mut req = c.page_request();
    req.get().set_form("notes@1.0.0");
    req.get().init_cursor().set_opaque(cursor.as_bytes());
    let r = req.send().promise.await?;
    let p = r.get()?.get_page()?;
    let next = String::from_utf8(r.get()?.get_next()?.get_opaque()?.to_vec()).unwrap();
    let ids = p
        .get_records()?
        .iter()
        .map(|r| r.get_id().unwrap().to_string().unwrap())
        .collect();
    Ok((ids, next, p.get_has_total(), p.get_has_estimate()))
}

/// M1: a page whose records are ALL struck never leaves with a `next` (formsource.capnp: an empty page has an empty `next`); the wrapper
/// reads on until a page that is not empty, or the end.
#[test]
fn a_page_that_is_empty_after_striking_carries_no_next() {
    run(async {
        let (mut g, rules) = notes();
        let alice = two_notes(&rules, &mut g).await;
        invoke(&alice, CREATE, "", &[("text", "three"), ("secret", "s3")]).await;
        g.core().spy.page_size.set(1);
        rules
            .unreadable
            .lock()
            .unwrap()
            .extend(["note-1".into(), "note-2".into()]);
        let (ids, next, ..) = page_of(&alice, "").await.expect("page");
        assert_eq!(ids, ["note-3"], "read on past the two struck pages");
        assert_eq!(next, "", "last page");
        // The rest is struck too: the end is an empty page with an empty `next`.
        rules.unreadable.lock().unwrap().push("note-3".into());
        let (ids, next, ..) = page_of(&alice, "").await.expect("page");
        assert!(ids.is_empty() && next.is_empty(), "{ids:?} {next:?}");
    });
}

/// M1: past the fixed limit the answer is the refusal `read-again` 503, never an empty page with a cursor.
#[test]
fn too_many_struck_pages_are_an_honest_refusal() {
    run(async {
        let (mut g, rules) = notes();
        let alice = connect(&mut g, "alice");
        for _ in 0..70 {
            invoke(&alice, CREATE, "", &[("text", "x"), ("secret", "s")]).await;
        }
        g.core().spy.page_size.set(1);
        rules
            .unreadable
            .lock()
            .unwrap()
            .extend((1..=69).map(|n| format!("note-{n}")));
        let e = page_of(&alice, "").await.expect_err("refused");
        assert_eq!(Problem::from_error(&e).expect("a problem").status, 503);
    });
}

/// M2: what the read right struck out is not counted: no `total`, no `estimate`. Without striking the count of the module stands.
#[test]
fn striking_a_record_removes_the_count() {
    run(async {
        let (mut g, rules) = notes();
        let alice = two_notes(&rules, &mut g).await;
        let (_, _, has_total, _) = page_of(&alice, "").await.expect("page");
        assert!(has_total, "nothing struck: the module's total stays");
        rules.unreadable.lock().unwrap().push("note-1".into());
        let (ids, _, has_total, has_estimate) = page_of(&alice, "").await.expect("page");
        assert_eq!(ids, ["note-2"]);
        assert!(!has_total && !has_estimate, "struck: no count");
    });
}
