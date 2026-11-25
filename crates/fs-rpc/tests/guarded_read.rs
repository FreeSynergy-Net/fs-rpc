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
