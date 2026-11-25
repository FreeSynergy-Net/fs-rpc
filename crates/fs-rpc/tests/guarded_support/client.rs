//! Helpers of the tests of `Guarded`: persons, the clients (`FormSource` by the capability of a connection), answers, a sink.
//!
//! Pattern: Test double helpers - in-process: the capability of a connection is called directly, no socket.
#![allow(
    dead_code,
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::field_reassign_with_default,
    clippy::unused_async_trait_impl
)]

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use capnp::capability::FromClientHook;
use fs_interfaces::formsource_capnp::{change_sink, form_source};
use fs_interfaces::module::{Actor, ModuleCore};
use fs_interfaces::owner::fake::person;
use fs_rpc::{Caller, Guarded, PerConnection, Ports};

use super::{Notes, Rules};

pub const ALICE: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e01";
pub const BOB: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e02";

pub fn run<F: Future>(f: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, f)
}

/// The system id tells the person: `alice`, `bob`; anybody else is not known.
pub fn ports() -> Ports {
    Ports::new(
        |_: &Caller| Actor::Person,
        |c: &Caller| match c.id() {
            "alice" => Some(person(ALICE)),
            "bob" => Some(person(BOB)),
            _ => None,
        },
    )
}

pub fn connect<M: ModuleCore>(g: &mut Guarded<M>, who: &str) -> form_source::Client {
    FromClientHook::new(g.bootstrap(&Caller::new(who, None)).hook)
}

/// What a write answered.
#[derive(Debug, Default)]
pub struct Wrote {
    pub failed: bool,
    pub kind: String,
    pub status: u16,
    pub record: String,
    pub text: String,
    pub program: Option<String>,
    pub origin_person: Option<String>,
    pub activity_zero: bool,
    pub clash_paths: Vec<String>,
}

pub fn args(
    req: &mut capnp::capability::Request<
        form_source::invoke_if_params::Owned,
        form_source::invoke_if_results::Owned,
    >,
    list: &[(&str, &str)],
) {
    let mut out = req.get().init_args(list.len() as u32);
    for (i, (p, t)) in list.iter().enumerate() {
        let mut e = out.reborrow().get(i as u32);
        e.set_path(p);
        e.set_text(t);
    }
}

pub async fn invoke_if(
    c: &form_source::Client,
    cmd: &str,
    record: &str,
    list: &[(&str, &str)],
    expect: &str,
) -> Wrote {
    let mut req = c.invoke_if_request();
    req.get().set_command(cmd);
    req.get().set_record(record);
    req.get().set_expect(expect);
    args(&mut req, list);
    let r = req
        .send()
        .promise
        .await
        .expect("invokeIf is answered, a refusal is in the answer");
    let a = r.get().unwrap();
    let mut w = Wrote {
        failed: a.get_failed(),
        ..Wrote::default()
    };
    let p = a.get_problem().unwrap();
    w.kind = p.get_type().unwrap().to_string().unwrap();
    w.status = p.get_status();
    let res = a.get_result().unwrap();
    w.record = res.get_record().unwrap().to_string().unwrap();
    w.text = res.get_text().unwrap().to_string().unwrap();
    if res.has_change() {
        let ch = res.get_change().unwrap();
        let o = ch.get_origin().unwrap();
        w.program = Some(o.get_program().unwrap().to_string().unwrap());
        w.origin_person = Some(o.get_person().unwrap().to_string().unwrap());
        w.activity_zero = ch.get_activity().unwrap().get_lo() == 0;
    }
    if a.has_conflict() {
        let c = a.get_conflict().unwrap();
        w.activity_zero = c.get_by().unwrap().get_activity().unwrap().get_lo() == 0;
        w.clash_paths = c
            .get_fields()
            .unwrap()
            .iter()
            .map(|f| f.get_path().unwrap().to_string().unwrap())
            .collect();
    }
    w
}

/// Level 1 `invoke`.
pub async fn invoke(
    c: &form_source::Client,
    cmd: &str,
    record: &str,
    list: &[(&str, &str)],
) -> Wrote {
    let mut req = c.invoke_request();
    req.get().set_command(cmd);
    req.get().set_record(record);
    let mut out = req.get().init_args(list.len() as u32);
    for (i, (p, t)) in list.iter().enumerate() {
        let mut e = out.reborrow().get(i as u32);
        e.set_path(p);
        e.set_text(t);
    }
    let r = req.send().promise.await.unwrap();
    let a = r.get().unwrap();
    Wrote {
        failed: a.get_failed(),
        kind: a
            .get_problem()
            .unwrap()
            .get_type()
            .unwrap()
            .to_string()
            .unwrap(),
        status: a.get_problem().unwrap().get_status(),
        record: a
            .get_result()
            .unwrap()
            .get_record()
            .unwrap()
            .to_string()
            .unwrap(),
        ..Wrote::default()
    }
}

pub async fn introduce(c: &form_source::Client, program: &str, instance: &str) {
    let mut req = c.introduce_request();
    req.get().set_program(program);
    req.get().set_instance(instance);
    req.get().set_device("dev-1");
    req.send().promise.await.unwrap();
}

pub fn notes() -> (Guarded<Notes>, Rules) {
    let rules = Rules::default();
    (Guarded::new(Notes::new(rules.clone()), ports()), rules)
}
pub async fn page(
    c: &form_source::Client,
) -> capnp::capability::Response<form_source::page_results::Owned> {
    let mut req = c.page_request();
    req.get().set_form("notes@1.0.0");
    req.send().promise.await.unwrap()
}

pub async fn page_err(c: &form_source::Client) -> bool {
    let mut req = c.page_request();
    req.get().set_form("notes@1.0.0");
    req.send().promise.await.is_err()
}

pub struct Sink {
    seen: Rc<RefCell<Vec<String>>>,
    ended: Rc<Cell<bool>>,
}
impl change_sink::Server for Sink {
    async fn changed(
        self: Rc<Self>,
        params: change_sink::ChangedParams,
    ) -> Result<(), capnp::Error> {
        let object = params.get()?.get_change()?.get_object()?.to_string()?;
        self.seen.borrow_mut().push(object);
        Ok(())
    }
    async fn ended(
        self: Rc<Self>,
        _: change_sink::EndedParams,
        _: change_sink::EndedResults,
    ) -> Result<(), capnp::Error> {
        self.ended.set(true);
        Ok(())
    }
}

pub async fn wait_for(what: impl Fn() -> bool) {
    for _ in 0..200 {
        if what() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("did not happen in time");
}

pub async fn watch(
    c: &form_source::Client,
) -> (
    Rc<RefCell<Vec<String>>>,
    Rc<Cell<bool>>,
    capnp::capability::Response<form_source::watch_results::Owned>,
) {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let ended = Rc::new(Cell::new(false));
    let mut req = c.watch_request();
    req.get().set_form("notes@1.0.0");
    req.get().set_sink(capnp_rpc::new_client(Sink {
        seen: seen.clone(),
        ended: ended.clone(),
    }));
    (seen, ended, req.send().promise.await.unwrap())
}
