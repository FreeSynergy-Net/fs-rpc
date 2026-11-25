//! `Guarded<M>`: the shape of a refusal (rules 14 and 15 of the suite `module_v1`), one test per correction.
//!
//! Pattern: Test (in-process: the capability of a connection is called directly, no socket; one `LocalSet` per test).
#![allow(clippy::many_single_char_names, clippy::field_reassign_with_default)]

mod guarded_support;

use guarded_support::client::*;
use guarded_support::{CREATE, SET};

/// A cursor that is no text is `form-source-bad-arguments` (400), not the generic `invalid-request` 422 of the transport.
#[test]
fn a_garbage_cursor_is_bad_arguments_not_invalid_request() {
    run(async {
        let (mut g, _) = notes();
        let alice = connect(&mut g, "alice");
        let mut req = alice.page_request();
        req.get().set_form("notes@1.0.0");
        req.get().init_cursor().set_opaque(&[0xff, 0xfe, 0xfd]);
        let text = req.send().promise.await.err().expect("refused").to_string();
        assert!(text.contains("form-source-bad-arguments"), "{text}");
        assert!(text.contains("400"), "{text}");
        assert!(!text.contains("freesynergy.net/problems"), "{text}");
        // `changes` reads the cursor the same way.
        let mut req = alice.changes_request();
        req.get().init_cursor().set_opaque(&[0xa5; 64]);
        let text = req.send().promise.await.err().expect("refused").to_string();
        assert!(text.contains("form-source-bad-arguments"), "{text}");
    });
}

/// The title of a refusal is a text for people, never the kind; the type still names the kind.
#[test]
fn the_title_of_a_refusal_is_text_for_people_not_the_kind() {
    run(async {
        let (mut g, _) = notes();
        let (alice, bob) = (connect(&mut g, "alice"), connect(&mut g, "bob"));
        invoke(&alice, CREATE, "", &[("text", "x")]).await;
        let mut req = bob.invoke_request();
        req.get().set_command(SET);
        req.get().set_record("note-1");
        let r = req.send().promise.await.unwrap();
        let a = r.get().unwrap();
        assert!(a.get_failed());
        let p = a.get_problem().unwrap();
        assert!(
            p.get_type()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with("form-source-forbidden")
        );
        let title = p.get_title().unwrap().to_str().unwrap();
        assert_eq!(title, "Not allowed");
        assert_ne!(title, "forbidden");
    });
}
