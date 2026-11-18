//! The typed absent values and the fields the contract appended, and what resolve says: counters, presence flags, range text,
//! provider warnings, origin, empty versus not given.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use std::rc::Rc;

use fs_forms_layout::*;
use fs_layouts_rpc::layouts_capnp::layouts;
use support::*;

#[test]
fn the_wire_fields_the_contract_appended_carry_what_the_store_said() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let local = mem(false, None);
        let mut bad = doc("zz");
        bad.format = 9;
        // reports: layout and rev presence follow the store, for a write, a dry run and a refusal
        for (d, mode) in [
            (doc("a"), Mode::Apply),
            (doc("b"), Mode::DryRun),
            (bad, Mode::DryRun),
        ] {
            let want = local.import(d.clone(), mode, &who()).await.unwrap();
            let got = c.import(d, mode, &who()).await.unwrap();
            assert_eq!(
                (want.rev, want.layout.is_some()),
                (got.rev, got.layout.is_some())
            );
            assert_eq!(want.problems.len(), got.problems.len());
        }
        // the log: counters and presence flags, entry by entry, for the same work done on both
        c.install(doc("p"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap();
        local
            .install(doc("p"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap();
        c.uninstall("net.example.pkg", &who()).await.unwrap();
        local.uninstall("net.example.pkg", &who()).await.unwrap();
        let shape = |l: Vec<LogEntry>| {
            l.into_iter()
                .map(|e| {
                    (
                        e.action,
                        e.rev_before,
                        e.rev_after,
                        e.count,
                        e.reason,
                        e.package,
                        e.elem,
                    )
                })
                .collect::<Vec<_>>()
        };
        let (got, want) = (
            shape(c.log(None, 100).await.unwrap()),
            shape(local.log(None, 100).await.unwrap()),
        );
        assert!(!want.is_empty(), "the local log is empty");
        assert_eq!(got, want);
    });
}

#[test]
fn a_save_logs_both_counters_and_a_list_counts_overlays_not_conflicts() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let own = c.create(doc("own"), &who()).await.unwrap().layout.unwrap();
        c.save(&own, Rev(1), doc("own"), &who()).await.unwrap();
        let log = c.log(None, 10).await.unwrap();
        let save = log.iter().find(|e| e.action == LogAction::Save).unwrap();
        assert_eq!(
            (save.rev_before, save.rev_after),
            (Some(Rev(1)), Some(Rev(2)))
        );
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        c.create_overlay(&id, None, None, &who()).await.unwrap();
        c.create_overlay(&id, None, None, &who()).await.unwrap();
        let page = c
            .list(
                &LayoutFilter {
                    package: Some("net.example.pkg".into()),
                    ..Default::default()
                },
                None,
                5,
            )
            .await
            .unwrap();
        assert_eq!(
            (page.items[0].overlays, page.items[0].open_conflicts),
            (2, 0)
        );
    });
}

#[test]
fn the_range_text_is_read_and_the_fields_are_the_fallback() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let conn = raw(&s.socket).await;
        let dry = |text: &str, cap: &str| {
            let mut req = conn.client.import_request();
            let mut p = req.get();
            p.set_dry_run(true);
            let mut doc = p.init_doc();
            doc.set_format(2);
            doc.set_extends("net.example.Tasks");
            doc.set_frontend("raw");
            let mut r = doc.init_requires(1).get(0);
            r.set_elem("requires:t");
            let mut range = r.init_range();
            range.set_text(text);
            range.set_capability(cap);
            range.set_min_minor(2);
            range.set_below_minor(3);
            req.send().promise
        };
        let report = |r: capnp::capability::Response<layouts::import_results::Owned>| {
            r.get()
                .unwrap()
                .get_report()
                .unwrap()
                .get_problems()
                .unwrap()
                .iter()
                .map(|p| fs_layouts_rpc::from_problem(&fs_rpc::Problem::read(p).unwrap()).unwrap())
                .collect::<Vec<_>>()
        };
        let garbage = report(dry("tasks >=zz", "").await.unwrap());
        let range = garbage
            .iter()
            .find(|c| code(c) == "range-invalid")
            .expect("the text was not read");
        assert_eq!(param(range, "text"), Some("tasks >=zz"));
        let fields = report(dry("", "tasks").await.unwrap());
        assert!(
            fields.iter().all(|c| code(c) != "range-invalid"),
            "fields only must build `tasks >=0.2 <0.3`: {fields:?}"
        );
    });
}

#[test]
fn a_provider_warning_reaches_the_host_and_a_blocking_problem_stays_a_problem() {
    struct Fake(Result<Option<bool>, ()>);
    impl ProviderCatalogs for Fake {
        fn field_has_schemes<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a str,
        ) -> Pending<'a, Option<bool>> {
            let r = self
                .0
                .map_err(|()| Coded::new(LayoutCode::Unreachable).with("socket", "tasks"));
            Box::pin(std::future::ready(r))
        }
    }
    let source = || {
        let mut d = doc("board");
        let src = SourceDoc {
            range: "tasks >=0.2 <0.3".into(),
            data_type: "Task".into(),
            scope: None,
        };
        d.sources = vec![item("source:1", src)];
        d.pages[0].v.blocks[0].v.source = Some(1);
        d
    };
    for (answer, problems, warnings) in [(Err(()), 0, 1), (Ok(Some(true)), 1, 0)] {
        let d = dir();
        let s = serve_on(&d.path().join("l.sock"), move || {
            service_with(false, Some(Rc::new(Fake(answer))))
        });
        run(async {
            let c = client(&s.socket);
            c.install(source(), &pkg("1.0"), Mode::Apply, &who())
                .await
                .unwrap();
            let r = c.resolve(&key("board")).await.unwrap().unwrap();
            assert_eq!((r.problems.len(), r.warnings.len()), (problems, warnings));
            if warnings == 1 {
                assert_eq!(
                    (r.warnings[0].code, r.warnings[0].param("capability")),
                    (LayoutCode::ProviderUnchecked, Some("tasks"))
                );
            }
        });
    }
}

#[test]
fn resolve_carries_the_real_summary() {
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
        c.create_overlay(&id, None, None, &who()).await.unwrap();
        let listed = c
            .list(&LayoutFilter::default(), None, 5)
            .await
            .unwrap()
            .items
            .remove(0);
        let conn = raw(&s.socket).await;
        let mut req = conn.client.resolve_request();
        req.get().set_extends("net.example.Tasks");
        req.get().set_frontend("board");
        let r = req.send().promise.await.unwrap();
        let sum = r
            .get()
            .unwrap()
            .get_resolved()
            .unwrap()
            .get_summary()
            .unwrap();
        assert_eq!(sum.get_origin().unwrap().to_str().unwrap(), "package");
        assert_eq!(
            sum.get_package().unwrap().to_str().unwrap(),
            "net.example.pkg"
        );
        assert_eq!(
            (sum.get_overlays(), sum.get_open_conflicts()),
            (listed.overlays, listed.open_conflicts)
        );
        assert_eq!(listed.overlays, 1);
        assert_eq!(listed.origin, Origin::Package);
    });
}
#[test]
fn the_filter_keeps_the_difference_between_empty_and_not_given() {
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
        let doc = c.original(&id).await.unwrap().unwrap().0.to_doc();
        let FilterDoc::Op { children, .. } = &doc.views[0].v.filter.as_ref().unwrap().v else {
            panic!("op")
        };
        let FilterDoc::Is {
            value, low, high, ..
        } = &children[1].v
        else {
            panic!("is")
        };
        assert_eq!(
            (value.as_deref(), low.as_deref(), high.as_deref()),
            (Some(""), Some(""), None)
        );
    });
}
