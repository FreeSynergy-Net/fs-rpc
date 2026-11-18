//! The actor of a connection is the caller the SYSTEM names (fs-rpc `Caller`), asked for each connection anew: a capability per
//! connection, the claimed `by` is logged with that caller and never effective, a caller nobody knows is told so, a refused
//! connection is told to the host.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fs_forms_layout::*;
use fs_rpc::{Caller, CallerError, PerConnection};
use support::*;

type Ids = Arc<Mutex<Vec<String>>>;

/// A server whose mapping from caller to actor records the id of every caller it is asked about.
fn recording(socket: &std::path::Path, ids: &Ids) -> Server {
    let ids = ids.clone();
    serve_on(socket, move || {
        let mut svc = service(true);
        svc.actors = Rc::new(move |c: &Caller| {
            ids.lock().unwrap().push(c.id().to_owned());
            Some(Reference(PERSON.to_owned()))
        });
        svc
    })
}

#[test]
fn every_connection_gets_its_own_capability_built_from_its_own_caller() {
    let d = dir();
    let ids = Ids::default();
    let s = recording(&d.path().join("l.sock"), &ids);
    run(async {
        let (a, b, c) = (client(&s.socket), client(&s.socket), client(&s.socket));
        for x in [&a, &b, &c] {
            x.stamp().await.unwrap();
        }
        let seen = ids.lock().unwrap().clone();
        assert!(
            seen.len() >= 3,
            "the mapping was asked {} time(s) for 3 connections: {seen:?}",
            seen.len()
        );
        assert!(
            seen.iter().all(|id| !id.is_empty()),
            "a caller without an id: {seen:?}"
        );
    });
}

#[test]
fn a_caller_nobody_knows_is_told_so_on_every_call() {
    let d = dir();
    let s = serve_on(&d.path().join("l.sock"), || {
        let mut svc = service(false);
        svc.actors = Rc::new(|_| None);
        svc
    });
    run(async {
        let c = client(&s.socket);
        let e = c.stamp().await.unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "what")),
            ("unknown", Some("caller")),
            "an unknown caller got an answer"
        );
        let e = c.create(doc("board"), &who()).await.unwrap_err();
        assert_eq!((code(&e), param(&e, "what")), ("unknown", Some("caller")));
    });
}

#[test]
fn the_claimed_by_is_logged_with_the_caller_of_the_connection_and_never_effective() {
    let d = dir();
    let ids = Ids::default();
    let claims: Ids = Ids::default();
    let (i2, c2) = (ids.clone(), claims.clone());
    let s = serve_on(&d.path().join("l.sock"), move || {
        let mut svc = service(true);
        svc.actors = Rc::new(move |c: &Caller| {
            i2.lock().unwrap().push(c.id().to_owned());
            Some(Reference(PERSON.to_owned()))
        });
        svc.claimed_by = Some(Rc::new(move |c: &Caller, by: &str| {
            c2.lock().unwrap().push(format!("{}|{by}", c.id()));
        }));
        svc
    });
    run(async {
        let c = client(&s.socket);
        let id = c
            .install(doc("board"), &pkg("1.0"), Mode::Apply, &who())
            .await
            .unwrap()
            .layout
            .unwrap();
        let forged = Who {
            actor: Reference("https://example.org/people/forged".into()),
            author: who().author,
        };
        c.admin()
            .lock(&id, None, "forged claim", &forged)
            .await
            .unwrap();
        let caller = ids.lock().unwrap().first().cloned().expect("a caller");
        assert_eq!(
            claims.lock().unwrap().as_slice(),
            [format!("{caller}|https://example.org/people/forged")],
            "the claim is logged with the caller of the connection"
        );
        let log = c.log(None, 20).await.unwrap();
        let lock = log.iter().find(|e| e.action == LogAction::Lock).unwrap();
        assert_eq!(lock.who.actor.0, PERSON, "the claimed by became the actor");
    });
}

#[test]
fn a_connection_the_system_could_not_name_is_told_to_the_host() {
    let told = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let t2 = told.clone();
    let mut svc = service(false);
    svc.refused = Some(Rc::new(move |why: &CallerError| {
        t2.borrow_mut().push(why.code().to_owned());
    }));
    let hub = fs_layouts_rpc::Hub::new();
    let mut per = svc.per_connection(&hub);
    per.refused(&CallerError::Credentials(std::io::Error::other("no")));
    assert_eq!(told.borrow().as_slice(), ["caller-credentials"]);
}

#[test]
fn each_connection_is_built_from_its_own_caller_in_order() {
    let ids = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let i2 = ids.clone();
    let mut svc = service(false);
    svc.actors = Rc::new(move |c: &Caller| {
        i2.borrow_mut().push(c.id().to_owned());
        Some(Reference(PERSON.to_owned()))
    });
    let hub = fs_layouts_rpc::Hub::new();
    let mut per = svc.per_connection(&hub);
    for id in ["1000", "2000", "3000"] {
        let _ = per.bootstrap(&Caller::new(id, None));
    }
    assert_eq!(
        ids.borrow().as_slice(),
        ["1000", "2000", "3000"],
        "a connection was built from another caller or from an earlier capability"
    );
}
