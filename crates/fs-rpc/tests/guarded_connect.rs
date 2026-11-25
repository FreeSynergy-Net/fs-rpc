//! `Guarded::connect`, `with_form_source`, `LocalPerson`, `OsUserIsPerson` (Stufe 4, B2a).
//!
//! Pattern: Test (in-process: the capability of a connection is called directly, no socket; one `LocalSet` per test).
#![allow(clippy::field_reassign_with_default)]

mod guarded_support;

use std::cell::Cell;
use std::rc::Rc;

use capnp::capability::FromClientHook;
use fs_interfaces::formsource_capnp::{change_sink, form_source};
use fs_interfaces::owner::OwnerProblem;
use fs_interfaces::owner::fake::{FixedOwner, NoOwner, person};
use fs_rpc::{
    ActorOfConnection, Caller, LocalPerson, OsUserIsPerson, PersonOfConnection, id_usable,
    with_form_source,
};
use guarded_support::CREATE;
use guarded_support::client::*;

/// `connect` is what `bootstrap` hands out: the same context (the person of the connection, a stranger refused).
#[test]
fn connect_is_bootstrap_the_same_context() {
    run(async {
        let (mut g, _) = notes();
        let by_boot = connect(&mut g, "alice");
        let by_connect = g.connect(&Caller::new("alice", None));
        for client in [&by_boot, &by_connect] {
            let w = invoke(client, CREATE, "", &[("text", "hi")]).await;
            assert!(!w.failed, "{w:?}");
        }
        let cxs = g.core().applied.borrow().clone();
        assert_eq!(cxs.len(), 2);
        assert_eq!(cxs[0].person, cxs[1].person);
        assert_eq!(cxs[1].person, person(ALICE));
        // No known person: refused on both paths.
        let nobody = g.connect(&Caller::new("nobody", None));
        assert_eq!(
            invoke(&nobody, CREATE, "", &[("text", "x")]).await.status,
            403
        );
        let nobody = connect(&mut g, "nobody");
        assert_eq!(
            invoke(&nobody, CREATE, "", &[("text", "x")]).await.status,
            403
        );
        assert_eq!(g.core().applied.borrow().len(), 2);
    });
}

struct Flag(Rc<Cell<u32>>);
impl change_sink::Server for Flag {
    async fn changed(self: Rc<Self>, _: change_sink::ChangedParams) -> Result<(), capnp::Error> {
        Ok(())
    }
    async fn ended(
        self: Rc<Self>,
        _: change_sink::EndedParams,
        _: change_sink::EndedResults,
    ) -> Result<(), capnp::Error> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}

/// `with_form_source` sends the calls of `FormSource` to the form capability and every other call to the program's own.
#[test]
fn with_form_source_routes_both_contracts() {
    run(async {
        let (g, _) = notes();
        let hits = Rc::new(Cell::new(0));
        let other: change_sink::Client = capnp_rpc::new_client(Flag(hits.clone()));
        let both = with_form_source(other.client, g.connect(&Caller::new("alice", None)));
        // The one capability, cast to each contract.
        let as_form: form_source::Client = FromClientHook::new(both.hook.add_ref());
        let as_other: change_sink::Client = FromClientHook::new(both.hook.add_ref());
        let w = invoke(&as_form, CREATE, "", &[("text", "hi")]).await;
        assert!(!w.failed, "{w:?}");
        assert_eq!(hits.get(), 0, "a FormSource call must not reach the other");
        as_other.ended_request().send().promise.await.unwrap();
        assert_eq!(hits.get(), 1);
        assert_eq!(
            g.core().applied.borrow().len(),
            1,
            "an other call must not reach the form"
        );
    });
}

/// `LocalPerson`: the owner is read once; every caller gets her; no owner is no start.
#[test]
fn local_person_gives_every_caller_the_owner_and_no_me_is_no_start() {
    let owner = person(ALICE);
    let p = LocalPerson::new(&FixedOwner(owner.clone())).expect("owner");
    assert_eq!(p.person(&Caller::new("1000", None)), Some(owner.clone()));
    assert_eq!(
        p.person(&Caller::new("anyone", Some("x".to_owned()))),
        Some(owner)
    );
    let no_me = OwnerProblem::NoMe {
        hint: "create me".into(),
    };
    let err = LocalPerson::new(&NoOwner(no_me.clone())).expect_err("no start");
    assert_eq!(err, no_me);
}

/// `OsUserIsPerson`: a usable id is a person, anything else a bot (fail closed); `id_usable` is the one rule.
#[test]
fn os_user_is_person_only_for_a_usable_id() {
    use fs_interfaces::module::Actor;
    let port = OsUserIsPerson;
    assert_eq!(port.actor(&Caller::new("1000", None)), Actor::Person);
    assert_eq!(
        port.actor(&Caller::new("S-1-5-21-1.2_3", None)),
        Actor::Person
    );
    assert_eq!(port.actor(&Caller::new("", None)), Actor::Bot);
    assert_eq!(port.actor(&Caller::new("a b", None)), Actor::Bot);
    assert!(id_usable("alice-1") && !id_usable("") && !id_usable("a/b"));
}
