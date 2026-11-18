//! The service itself: whose work it is (author), limits as settings with their source, the provider rule for what it cannot do,
//! the administrator's commands only on the instance service.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use fs_forms_layout::*;
use fs_layouts_rpc::{LayoutService, ServiceLimits};
use support::*;

fn other() -> Who {
    let r = Reference("https://example.org/people/other".to_owned());
    Who {
        actor: r.clone(),
        author: r,
    }
}

#[test]
fn another_persons_work_is_refused_never_replaced() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let e = c.create(doc("board"), &other()).await.unwrap_err();
        assert_eq!((code(&e), param(&e, "what")), ("unknown", Some("author")));
        assert!(
            c.list(&LayoutFilter::default(), None, 5)
                .await
                .unwrap()
                .items
                .is_empty(),
            "expected nothing"
        );
        // "" on the wire is the service's own person: made through the raw contract
        let c2 = client(&s.socket);
        let anon = Who {
            actor: who().actor,
            author: Reference(String::new()),
        };
        assert!(
            c2.create(doc("board"), &anon)
                .await
                .unwrap()
                .problems
                .is_empty(),
            "expected nothing"
        );
    });
}

#[test]
fn a_page_over_the_limit_names_the_limit_and_where_it_came_from() {
    let d = dir();
    let s = serve_on(&d.path().join("l.sock"), || {
        let mut svc: LayoutService<dyn LayoutStore> = service(false);
        svc.limits = ServiceLimits {
            page_max: 3,
            watchers_max: 1,
            watchers_per_actor_max: 8,
            by: LimitSource::Ceiling,
        };
        svc
    });
    run(async {
        let c = client(&s.socket);
        let e = c.list(&LayoutFilter::default(), None, 4).await.unwrap_err();
        assert_eq!(code(&e), "page-size");
        assert_eq!(
            (param(&e, "size"), param(&e, "max"), param(&e, "by")),
            (Some("4"), Some("3"), Some("ceiling"))
        );
        assert!(c.list(&LayoutFilter::default(), None, 3).await.is_ok());
        let e = c.log(None, 9).await.unwrap_err();
        assert_eq!(code(&e), "page-size");
    });
}

#[test]
fn what_the_port_cannot_do_answers_unsupported_and_is_in_the_report() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let e = c
            .admin()
            .lock(&LayoutId(ObjectId(uuid_text(1))), None, "why", &who())
            .await
            .unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what")),
            ("unsupported", Some("admin"))
        );
        let raw = fs_rpc::connect::<fs_layouts_rpc::layouts_capnp::layouts::Client>(&s.socket)
            .await
            .unwrap();
        for (name, request) in [
            (
                "repair",
                raw.client.repair_request().send().promise.await.map(drop),
            ),
            (
                "bind",
                raw.client.bind_request().send().promise.await.map(drop),
            ),
        ] {
            let p = fs_rpc::Problem::from_error(&request.unwrap_err())
                .expect("a problem, not a bare error");
            let c = fs_layouts_rpc::from_problem(&p).expect("a layout code");
            assert_eq!((code(&c), param(&c, "what")), ("unsupported", Some(name)));
        }
        let r = raw
            .client
            .support_report_request()
            .send()
            .promise
            .await
            .unwrap();
        let caps =
            fs_interfaces::Capabilities::read(r.get().unwrap().get_report().unwrap()).unwrap();
        assert!(
            caps.is_supported("layouts.read")
                && !caps.is_supported("layouts.admin")
                && !caps.is_supported("layouts.files")
        );
        assert!(caps.why_not("layouts.files").is_some());
    });
}

#[test]
fn the_instance_service_locks_and_a_reason_is_mandatory() {
    let d = dir();
    let s = instance(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let admin = c.admin();
        let block = ElemRef {
            kind: ElemKind::Block,
            elem: elem("block:board/main"),
        };
        let e = admin
            .lock(&id, Some(&block), "   ", &who())
            .await
            .unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what")),
            ("reason-missing", Some("lock"))
        );
        assert!(
            admin
                .lock(&id, Some(&block), "frozen for the audit", &who())
                .await
                .unwrap()
        );
        let res = c.resolve(&key("board")).await.unwrap().unwrap();
        assert_eq!(res.locks.len(), 1);
        assert_eq!(
            (res.locks[0].reason.as_str(), res.locks[0].target.clone()),
            ("frozen for the audit", Some(block.clone()))
        );
        assert!(
            admin
                .unlock(&id, Some(&block), "audit done", &who())
                .await
                .unwrap()
        );
        let o = admin
            .create_instance_overlay(&id, None, None, &who())
            .await
            .unwrap();
        let ov = c.overlay(&o).await.unwrap().unwrap();
        assert_eq!(ov.scope, OverlayScope::Instance);
        let e = admin.lock(&id, None, "x", &who()).await;
        assert!(e.is_ok());
    });
}
