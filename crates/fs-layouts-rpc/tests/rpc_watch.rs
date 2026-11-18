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
