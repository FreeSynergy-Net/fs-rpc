//! The ports `LayoutRead`, `LayoutWrite` and `PackageLayouts` over a real socket: what the client says is what the store does, and what
//! the store refuses arrives as the same code with the same parameters.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use fs_forms_layout::*;
use support::*;

#[test]
fn create_list_original_resolve_roundtrip() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let r = c.create(rich("board"), &who()).await.unwrap();
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        let id = r.layout.clone().unwrap();
        assert_eq!(r.rev, Some(Rev(1)));
        let page = c.list(&LayoutFilter::default(), None, 10).await.unwrap();
        assert_eq!((page.items.len(), page.next.clone()), (1, None));
        assert_eq!(page.items[0].id, id);
        assert_eq!(page.items[0].origin, Origin::Person);
        assert_eq!(page.items[0].owner, Some(who().author));
        // the whole tree survives the wire: every family (K-wire)
        let (layout, summary) = c.original(&id).await.unwrap().unwrap();
        assert_eq!(layout.to_doc(), rich("board"));
        assert_eq!(summary.key, key("board"));
        let res = c.resolve(&key("board")).await.unwrap().unwrap();
        assert_eq!(
            (res.id, res.rev, res.layout.to_doc()),
            (id, Rev(1), rich("board"))
        );
        assert!(c.stamp().await.unwrap().0 > 0, "stamp");
        assert!(matches!(c.health().await.unwrap(), Health::Ok));
    });
}

#[test]
fn absent_is_none_never_an_error() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        assert!(c.resolve(&key("nope")).await.unwrap().is_none());
        let gone = LayoutId(ObjectId(support::uuid_text(1)));
        assert!(c.original(&gone).await.unwrap().is_none());
    });
}

#[test]
fn a_refusal_is_a_code_with_parameters_and_nothing_changed() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        c.create(doc("board"), &who()).await.unwrap();
        let again = refusal(c.create(doc("board"), &who()).await);
        assert_eq!(code(&again), "exists");
        assert_eq!(param(&again, "frontend"), Some("board"));
        assert_eq!(
            c.list(&LayoutFilter::default(), None, 10)
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        // a broken description: format 9 is newer than the service knows
        let mut bad = doc("other");
        bad.format = 9;
        let e = refusal(c.create(bad, &who()).await);
        assert_eq!(code(&e), "format-newer");
        assert_eq!(
            (param(&e, "found"), param(&e, "known")),
            (Some("9"), Some("2"))
        );
    });
}

#[test]
fn save_is_compare_and_set_and_delete_is_a_tombstone() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let id = c
            .create(doc("board"), &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let mut v2 = doc("board");
        v2.title_key = "other".into();
        let lost = c.save(&id, Rev(7), v2.clone(), &who()).await;
        let e = refusal(lost);
        assert_eq!(code(&e), "rev-changed");
        assert_eq!(
            (param(&e, "expected"), param(&e, "found")),
            (Some("7"), Some("1"))
        );
        let r = c.save(&id, Rev(1), v2, &who()).await.unwrap();
        assert_eq!(r.rev, Some(Rev(2)));
        c.delete(&id, Rev(2), &who()).await.unwrap();
        assert!(c.original(&id).await.unwrap().is_none());
        let all = c
            .list(
                &LayoutFilter {
                    with_deleted: true,
                    ..LayoutFilter::default()
                },
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(all.items.len(), 1);
        assert!(all.items[0].deleted.is_some());
        let err = c.delete(&id, Rev(3), &who()).await.unwrap_err();
        assert_eq!(code(&err), "unknown");
    });
}

#[test]
fn install_dry_run_writes_nothing_and_a_package_original_is_not_saved() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let dry = c
            .install(doc("board"), &pkg("1.0"), Mode::DryRun, &who())
            .await
            .unwrap();
        assert!(dry.problems.is_empty(), "{dry:?}");
        assert!(
            c.list(&LayoutFilter::default(), None, 10)
                .await
                .unwrap()
                .items
                .is_empty(),
            "expected nothing"
        );
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let e = refusal(c.save(&id, Rev(1), doc("board"), &who()).await);
        assert_eq!(code(&e), "package-owned");
        let page = c
            .list(
                &LayoutFilter {
                    package: Some("net.example.pkg".into()),
                    ..LayoutFilter::default()
                },
                None,
                10,
            )
            .await
            .unwrap();
        assert_eq!(page.items[0].package, Some(pkg("1.0")));
        let gone = c.uninstall("net.example.pkg", &who()).await.unwrap();
        assert_eq!(gone.len(), 1);
        assert!(c.original(&id).await.unwrap().is_none());
    });
}

#[test]
fn the_log_reads_back_with_its_cursor() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let id = c
            .create(doc("board"), &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        c.delete(&id, Rev(1), &who()).await.unwrap();
        let all = c.log(None, 10).await.unwrap();
        assert_eq!(
            all.iter().map(|e| e.action).collect::<Vec<_>>(),
            [LogAction::Create, LogAction::Delete]
        );
        assert_eq!(all[0].layout, Some(id));
        assert_eq!((all[0].rev_before, all[0].rev_after), (None, Some(Rev(1))));
        let rest = c.log(Some(&all[0].id), 10).await.unwrap();
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].id, all[1].id);
    });
}

/// The standard menus travel as `kind = "standard"` with `standard = "program" | "help"` (what a reader that is not Rust takes),
/// never as one invented word: the wire is read raw here, not through our own reader.
#[test]
fn the_standard_menu_is_kind_standard_with_its_word_on_the_wire() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let id = c
            .create(rich("board"), &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let conn = raw(&s.socket).await;
        let mut req = conn.client.original_request();
        put_uuid(req.get().init_id(), &id.0.0);
        let res = req.send().promise.await.unwrap();
        let menu = res.get().unwrap().get_layout().unwrap().get_menu().unwrap();
        let mut seen = Vec::new();
        for m in menu {
            let elem = m.get_elem().unwrap().to_string().unwrap();
            if elem == "menu:help" || elem == "menu:program" {
                seen.push((
                    elem,
                    m.get_kind().unwrap().to_string().unwrap(),
                    m.get_standard().unwrap().to_string().unwrap(),
                ));
            }
        }
        seen.sort();
        let t = |a: &str, b: &str, c: &str| (a.to_owned(), b.to_owned(), c.to_owned());
        assert_eq!(
            seen,
            [
                t("menu:help", "standard", "help"),
                t("menu:program", "standard", "program")
            ]
        );
    });
}
