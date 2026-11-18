//! `watch @23`: the host hears about a change without polling; letting go of the subscription frees the slot (Abo-Freigabe); the
//! number of watchers is a setting and its refusal names the limit.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use fs_forms_layout::*;
use fs_layouts_rpc::ServiceLimits;
use support::*;

async fn until(mut done: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    done()
}

#[test]
fn a_write_tells_the_watcher_the_new_stamp_and_a_dry_run_does_not() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let heard = Rc::new(RefCell::new(Vec::<u64>::new()));
        let h = heard.clone();
        let watching = c
            .watch(
                Rc::new(move |st: Stamp| h.borrow_mut().push(st.0)),
                Rc::new(|| {}),
            )
            .await
            .unwrap();
        c.import(doc("dry"), Mode::DryRun, &who()).await.unwrap();
        c.create(doc("board"), &who()).await.unwrap();
        let stamp = c.stamp().await.unwrap().0;
        assert!(
            until(|| !heard.borrow().is_empty()).await,
            "no change heard"
        );
        assert_eq!(
            heard.borrow().as_slice(),
            [stamp],
            "exactly the write, with its stamp"
        );
        drop(watching);
    });
}

#[test]
fn dropping_the_subscription_frees_the_watcher() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let c = client(&s.socket);
        let w = c.watch(Rc::new(|_| {}), Rc::new(|| {})).await.unwrap();
        assert!(until(|| s.watchers.count() == 1).await);
        drop(w);
        assert!(
            until(|| s.watchers.count() == 0).await,
            "the service still keeps a dropped watcher"
        );
    });
}

#[test]
fn the_watcher_limit_is_a_setting_with_its_source() {
    let d = dir();
    let s = serve_on(&d.path().join("l.sock"), || {
        let mut svc = service(false);
        svc.limits = ServiceLimits {
            page_max: 10,
            watchers_max: 1,
            watchers_per_actor_max: 8,
            by: LimitSource::Person,
        };
        svc
    });
    run(async {
        let c = client(&s.socket);
        let _first = c.watch(Rc::new(|_| {}), Rc::new(|| {})).await.unwrap();
        let e = c
            .watch(Rc::new(|_| {}), Rc::new(|| {}))
            .await
            .map(drop)
            .unwrap_err();
        assert_eq!(
            (
                code(&e),
                param(&e, "what"),
                param(&e, "max"),
                param(&e, "by")
            ),
            ("too-many", Some("watchers"), Some("1"), Some("person"))
        );
    });
}

#[test]
fn a_refused_watch_never_reports_an_end() {
    let d = dir();
    let s = serve_on(&d.path().join("l.sock"), || {
        let mut svc = service(false);
        svc.limits.watchers_max = 1;
        svc
    });
    run(async {
        let c = client(&s.socket);
        let _first = c.watch(Rc::new(|_| {}), Rc::new(|| {})).await.unwrap();
        let ended = Rc::new(std::cell::Cell::new(false));
        let e2 = ended.clone();
        let e = c
            .watch(Rc::new(|_| {}), Rc::new(move || e2.set(true)))
            .await
            .err()
            .expect("refused");
        assert_eq!(code(&e), "too-many");
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!ended.get(), "a refused watch reported an end");
    });
}

/// A listener that answers `changed` with an error (a protocol breach): the service drops it at once, while its subscription is
/// still held by the other side.
struct Broken;

#[allow(clippy::unused_async_trait_impl)]
impl fs_layouts_rpc::layouts_capnp::layouts::listener::Server for Broken {
    async fn changed(
        self: Rc<Self>,
        _: fs_layouts_rpc::layouts_capnp::layouts::listener::ChangedParams,
    ) -> Result<(), capnp::Error> {
        Err(capnp::Error::failed("broken listener".into()))
    }
}

#[test]
fn a_dead_listener_is_removed_while_its_subscription_is_still_held() {
    let d = dir();
    let s = person(&d.path().join("l.sock"));
    run(async {
        let conn = raw(&s.socket).await;
        let mut req = conn.client.watch_request();
        req.get().set_listener(capnp_rpc::new_client(Broken));
        let _held = req.send().promise.await.unwrap();
        assert!(until(|| s.watchers.count() == 1).await);
        let c = client(&s.socket);
        for n in 0..6 {
            c.create(doc(&format!("f{n}")), &who()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        assert!(
            until(|| s.watchers.count() == 0).await,
            "a dead listener is still kept"
        );
    });
}

#[test]
fn a_store_that_cannot_say_its_stamp_is_counted() {
    let d = dir();
    let fail = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let f2 = fail.clone();
    let s = serve_on(&d.path().join("l.sock"), move || {
        service_full(false, None, f2, None)
    });
    run(async {
        let c = client(&s.socket);
        c.create(doc("a"), &who()).await.unwrap();
        assert_eq!(s.watchers.missed(), 0);
        fail.store(true, std::sync::atomic::Ordering::SeqCst);
        c.create(doc("b"), &who()).await.unwrap();
        assert_eq!(s.watchers.missed(), 1, "the failed stamp was not counted");
    });
}

/// The hub of a store is shared by all its sockets: a write through socket A is heard by a watcher on socket B and the watchers of both sockets are counted together.
#[test]
fn a_write_through_one_socket_is_heard_on_the_other_socket() {
    let d = dir();
    let (a, b) = (d.path().join("a.sock"), d.path().join("b.sock"));
    let s = serve_actors(vec![
        (a.clone(), None),
        (b.clone(), Some("https://example.org/bots/seat".into())),
    ]);
    run(async {
        let (ca, cb) = (client(&a), client(&b));
        let (on_a, on_b) = (Rc::new(RefCell::new(0_u32)), Rc::new(RefCell::new(0_u32)));
        let (ha, hb) = (on_a.clone(), on_b.clone());
        let _wa = ca
            .watch(Rc::new(move |_| *ha.borrow_mut() += 1), Rc::new(|| {}))
            .await
            .unwrap();
        let _wb = cb
            .watch(Rc::new(move |_| *hb.borrow_mut() += 1), Rc::new(|| {}))
            .await
            .unwrap();
        assert!(
            until(|| s.watchers.count() == 2).await,
            "one hub counts both"
        );
        ca.create(doc("board"), &who()).await.unwrap();
        assert!(
            until(|| *on_b.borrow() == 1).await,
            "the watcher on socket B did not hear the write through socket A"
        );
        assert!(until(|| *on_a.borrow() == 1).await, "the watcher on A");
    });
}

/// The limit counts the watchers of the HUB, not of one socket: with one place, the first watch on socket A takes it and a watch on
/// socket B is refused with the limit named.
#[test]
fn the_watcher_limit_counts_the_whole_hub_not_one_socket() {
    let d = dir();
    let (a, b) = (d.path().join("a.sock"), d.path().join("b.sock"));
    let s = serve_actors_with(
        vec![
            (a.clone(), None),
            (b.clone(), Some("https://example.org/bots/seat".into())),
        ],
        Some(1),
    );
    run(async {
        let (ca, cb) = (client(&a), client(&b));
        let _first = ca.watch(Rc::new(|_| {}), Rc::new(|| {})).await.unwrap();
        assert!(until(|| s.watchers.count() == 1).await);
        let e = cb
            .watch(Rc::new(|_| {}), Rc::new(|| {}))
            .await
            .map(drop)
            .unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what"), param(&e, "max")),
            ("too-many", Some("watchers"), Some("1")),
            "the limit was counted per socket"
        );
    });
}
