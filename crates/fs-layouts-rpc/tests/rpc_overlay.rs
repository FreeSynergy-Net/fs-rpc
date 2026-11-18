//! `OverlayWrite` and the reading of overlays and conflicts over a real socket: an overlay changes the resolved layout and not the
//! original, a changed original gives a kept conflict, the store (not the caller) fills `expect`.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use fs_forms_layout::*;
use support::*;

#[test]
fn an_overlay_acts_in_resolve_and_the_original_stays() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let before = c.original(&id).await.unwrap().unwrap().0.digest();
        let o = c
            .create_overlay(&id, Some("de"), Some("mine-title"), &who())
            .await
            .unwrap();
        let r = c
            .replace_ops(&o, Rev(1), vec![set_text(Field::TextKey, "mine")], &who())
            .await
            .unwrap();
        assert!(r.problems.is_empty() && r.conflicts.is_empty(), "{r:?}");
        let res = c.resolve(&key("board")).await.unwrap().unwrap();
        assert_eq!(
            res.layout.doc().pages[0].v.blocks[0].v.text_key.as_deref(),
            Some("mine")
        );
        assert_eq!(res.layers.len(), 1);
        assert_eq!(res.layers[0].overlay, o);
        assert!(matches!(&res.provenance[0].1, Layer::Person(x) if *x == o));
        assert_eq!(c.original(&id).await.unwrap().unwrap().0.digest(), before);
        let ov = c.overlay(&o).await.unwrap().unwrap();
        assert_eq!(
            (ov.lang.as_deref(), ov.title_key.as_deref()),
            (Some("de"), Some("mine-title"))
        );
        assert_eq!(ov.base, pkg("1.0"));
        // D13: the caller put "wrong" into `expect`; the store set it from the original
        assert!(matches!(
            &ov.ops[0].kind,
            OpKind::Set {
                expect: Value::None,
                ..
            }
        ));
        assert_eq!(c.overlays(&id).await.unwrap().len(), 1);
        assert!(
            c.overlay(&OverlayId(ObjectId(uuid_text(99))))
                .await
                .unwrap()
                .is_none()
        );
    });
}

#[test]
fn a_changed_original_gives_a_conflict_and_it_resolves() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let o = c.create_overlay(&id, None, None, &who()).await.unwrap();
        c.replace_ops(&o, Rev(1), vec![set_text(Field::TextKey, "mine")], &who())
            .await
            .unwrap();
        let mut v2 = doc("board");
        v2.pages[0].v.blocks[0].v.text_key = Some("theirs".into());
        let r = c
            .install(v2, &pkg("2.0"), Mode::Apply, &who())
            .await
            .unwrap();
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(
            (r.conflicts[0].overlay.clone(), r.conflicts[0].cause),
            (o.clone(), Cause::TargetChanged)
        );
        // the probe store leaves `found` empty; the empty value must survive the wire as empty
        assert_eq!(r.conflicts[0].found, Value::None);
        let open = c.conflicts(Some(&id), true).await.unwrap();
        assert_eq!(open.len(), 1);
        assert!(open[0].open);
        let everywhere = c.conflicts(None, true).await.unwrap();
        assert_eq!(everywhere.len(), 1);
        let ov = c.overlay(&o).await.unwrap().unwrap();
        c.resolve_conflict(&o, 0, "2.0", Resolution::Keep, &who())
            .await
            .unwrap();
        assert!(ov.rev.0 >= 1);
        let res = c.resolve(&key("board")).await.unwrap().unwrap();
        assert_eq!(
            res.layout.doc().pages[0].v.blocks[0].v.text_key.as_deref(),
            Some("mine")
        );
    });
}

#[test]
fn state_delete_and_refusals() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let own = c.create(doc("own"), &who()).await.unwrap().layout.unwrap();
        let e = c
            .create_overlay(&own, None, None, &who())
            .await
            .unwrap_err();
        assert_eq!(code(&e), "not-overlayable");
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let o = c.create_overlay(&id, None, None, &who()).await.unwrap();
        let rev = c.set_state(&o, Rev(1), false, &who()).await.unwrap();
        assert_eq!(
            c.overlay(&o).await.unwrap().unwrap().state,
            OverlayState::Off
        );
        let e = c
            .delete_overlay(&o, Rev(rev.0 + 5), &who())
            .await
            .unwrap_err();
        assert_eq!(code(&e), "rev-changed");
        c.delete_overlay(&o, rev, &who()).await.unwrap();
        assert!(c.overlay(&o).await.unwrap().is_none());
    });
}

#[test]
fn an_overlay_document_imports_with_a_dry_run() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        c.install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap();
        let ops = vec![Item {
            elem: Some(elem("block:board/main")),
            line: 0,
            v: set_text(Field::TextKey, "mine"),
        }];
        let od = OverlayDoc {
            format: 1,
            package: "net.example.pkg".into(),
            extends: "net.example.Tasks".into(),
            frontend: "board".into(),
            base_version: "1.0".into(),
            lang: None,
            title_key: None,
            ops,
        };
        let dry = c
            .import_overlay(od.clone(), Mode::DryRun, &who())
            .await
            .unwrap();
        assert!(dry.problems.is_empty(), "{dry:?}");
        let id = c
            .list(&LayoutFilter::default(), None, 5)
            .await
            .unwrap()
            .items[0]
            .id
            .clone();
        assert!(c.overlays(&id).await.unwrap().is_empty(), "overlay present");
        let done = c.import_overlay(od, Mode::Apply, &who()).await.unwrap();
        assert!(done.problems.is_empty(), "{done:?}");
        assert_eq!(c.overlays(&id).await.unwrap().len(), 1);
    });
}
