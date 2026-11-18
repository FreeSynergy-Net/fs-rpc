//! The places of the watchers are limited twice, both as SETTINGS the host passes in: in all (the hub of one store) and per caller
//! (`Caller::id`), so a bot cannot take the places of the person. Reaching a limit is a visible error with a code (`too-many`, what,
//! max, by), never silence. The callers are fabricated here and the capabilities used in this process: no socket can say another
//! user.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use std::rc::Rc;

use capnp::capability::Client;
use fs_forms_layout::*;
use fs_layouts_rpc::layouts_capnp::layouts;
use fs_layouts_rpc::{Hub, ServiceLimits};
use fs_rpc::{Caller, PerConnection};
use support::*;

struct Noop;

#[allow(clippy::unused_async_trait_impl)]
impl layouts::listener::Server for Noop {
    async fn changed(
        self: Rc<Self>,
        _: layouts::listener::ChangedParams,
    ) -> Result<(), capnp::Error> {
        Ok(())
    }
}

fn limited(all: u32, per_actor: u32) -> fs_layouts_rpc::Layouts<dyn LayoutStore> {
    let mut svc = service(false);
    svc.limits = ServiceLimits {
        page_max: 10,
        watchers_max: all,
        watchers_per_actor_max: per_actor,
        by: LimitSource::Person,
    };
    svc.per_connection(&Hub::new())
}

fn connect_as(per: &mut fs_layouts_rpc::Layouts<dyn LayoutStore>, id: &str) -> layouts::Client {
    let boot: Client = per.bootstrap(&Caller::new(id, None));
    <layouts::Client as capnp::capability::FromClientHook>::new(boot.hook)
}

/// Watches once; the subscription is returned so that it stays held.
async fn watch(
    c: &layouts::Client,
) -> Result<capnp::capability::Response<layouts::watch_results::Owned>, Coded<LayoutCode>> {
    let mut req = c.watch_request();
    req.get().set_listener(capnp_rpc::new_client(Noop));
    match req.send().promise.await {
        Ok(r) => Ok(r),
        Err(e) => {
            let p =
                fs_rpc::Problem::from_error(&e).expect("a problem with a code, not a bare error");
            Err(fs_layouts_rpc::from_problem(&p).expect("a layout code"))
        }
    }
}

#[test]
fn one_caller_cannot_take_more_places_than_its_own_limit_and_another_caller_is_not_hurt() {
    run(async {
        let mut per = limited(10, 2);
        let (a1, a2, b) = (
            connect_as(&mut per, "1000"),
            connect_as(&mut per, "1000"),
            connect_as(&mut per, "2000"),
        );
        // two connections of the SAME caller share its two places
        let _w1 = watch(&a1).await.unwrap();
        let _w2 = watch(&a2).await.unwrap();
        let e = watch(&a1).await.map(drop).unwrap_err();
        assert_eq!(
            (
                code(&e),
                param(&e, "what"),
                param(&e, "max"),
                param(&e, "by")
            ),
            (
                "too-many",
                Some("watchers-of-caller"),
                Some("2"),
                Some("person")
            )
        );
        // another caller still has its own places
        let _w3 = watch(&b)
            .await
            .expect("another caller was refused for the first");
    });
}

#[test]
fn the_hub_limit_counts_all_callers_together_and_names_itself() {
    run(async {
        let mut per = limited(2, 5);
        let (a, b, c) = (
            connect_as(&mut per, "1000"),
            connect_as(&mut per, "2000"),
            connect_as(&mut per, "3000"),
        );
        let _w1 = watch(&a).await.unwrap();
        let _w2 = watch(&b).await.unwrap();
        let e = watch(&c).await.map(drop).unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what"), param(&e, "max")),
            ("too-many", Some("watchers"), Some("2"))
        );
    });
}

#[test]
fn a_caller_nobody_knows_gets_no_places_at_all() {
    run(async {
        let mut svc = service(false);
        svc.actors = Rc::new(|c: &Caller| (c.id() == "1000").then(|| Reference(PERSON.to_owned())));
        let mut per = svc.per_connection(&Hub::new());
        let stranger = connect_as(&mut per, "9999");
        let e = watch(&stranger).await.map(drop).unwrap_err();
        assert_eq!((code(&e), param(&e, "what")), ("unknown", Some("caller")));
    });
}
