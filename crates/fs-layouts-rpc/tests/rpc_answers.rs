//! What the port cannot do answers `unsupported` with its name; a foreign service is told apart from ours.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use std::rc::Rc;

use fs_forms_layout::*;
use fs_layouts_rpc::layouts_capnp::layouts;
use support::*;

#[test]
fn every_method_the_port_cannot_do_answers_unsupported_with_its_name() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let conn = raw(&s.socket).await;
        let k = &conn.client;
        let mut got = Vec::new();
        got.push((
            "installFile",
            k.install_file_request().send().promise.await.map(drop),
        ));
        got.push((
            "importFile",
            k.import_file_request().send().promise.await.map(drop),
        ));
        got.push((
            "exportFile",
            k.export_file_request().send().promise.await.map(drop),
        ));
        got.push((
            "importOverlayFile",
            k.import_overlay_file_request()
                .send()
                .promise
                .await
                .map(drop),
        ));
        got.push((
            "exportOverlayFile",
            k.export_overlay_file_request()
                .send()
                .promise
                .await
                .map(drop),
        ));
        got.push(("repair", k.repair_request().send().promise.await.map(drop)));
        got.push(("bind", k.bind_request().send().promise.await.map(drop)));
        for (name, r) in got {
            let p = fs_rpc::Problem::from_error(&r.unwrap_err()).expect("a problem");
            let c = fs_layouts_rpc::from_problem(&p).expect("a layout code");
            assert_eq!((code(&c), param(&c, "what")), ("unsupported", Some(name)));
        }
    });
}

struct Foreign;

#[allow(clippy::unused_async_trait_impl)]
impl layouts::Server for Foreign {
    async fn stamp(
        self: Rc<Self>,
        _: layouts::StampParams,
        _: layouts::StampResults,
    ) -> Result<(), capnp::Error> {
        Err(fs_rpc::unsupported(&fs_interfaces::Unsupported::new(
            "layouts.other",
            "no",
        )))
    }
    async fn health(
        self: Rc<Self>,
        _: layouts::HealthParams,
        _: layouts::HealthResults,
    ) -> Result<(), capnp::Error> {
        Err(fs_rpc::invalid_request("not ours"))
    }
    async fn list(
        self: Rc<Self>,
        _: layouts::ListParams,
        _: layouts::ListResults,
    ) -> Result<(), capnp::Error> {
        Err(fs_rpc::failed("a bare error"))
    }
}

#[test]
fn a_foreign_service_is_told_apart_and_a_bare_error_keeps_the_link() {
    let d = dir();
    let socket = d.path().join("f.sock");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let (ready, up) = std::sync::mpsc::channel();
    let path = socket.clone();
    let t = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&rt, async move {
            let bound = fs_rpc::bind_socket(&path).await.unwrap();
            ready.send(()).unwrap();
            let boot = capnp_rpc::new_client::<layouts::Client, _>(Foreign).client;
            fs_rpc::serve(bound, boot, async move {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
    });
    up.recv().unwrap();
    run(async {
        let c = client(&socket);
        let e = c.stamp().await.unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what")),
            ("unsupported", Some("layouts.other"))
        );
        let e = c.health().await.map(drop).unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "found")),
            ("format-invalid", Some("problem-type"))
        );
        let e = c.list(&LayoutFilter::default(), None, 1).await.unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "found")),
            ("format-invalid", Some("rpc-error"))
        );
        // the link was kept: the next answer is the service's, not `unreachable`
        assert_eq!(code(&c.stamp().await.unwrap_err()), "unsupported");
    });
    drop(stop);
    t.join().unwrap();
}
