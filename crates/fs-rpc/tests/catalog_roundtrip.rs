//! Acceptance test of ADR 0093 (S4): the type catalog of `fs-types` crosses the wire through the `fs-interfaces` topic and the
//! `fs-rpc` server, and what comes back is what went in. Three libraries meet only here: the schema and values of `fs-types`,
//! the `People` contract of `fs-interfaces`, the `CatalogServer` and the transport of `fs-rpc`.
//!
//! Two ways of the same bytes: an in-memory duplex (runs on every system, Windows included) and a real Unix socket (Unix only:
//! the Windows transport is a named pipe, which has its own tests in `rpc_socket_any.rs`; the duplex case covers the wire there).
//! Besides the catalog the test sends an `Edit` carrying values of `fs-types` (`Utc`, `LocalDateTime`, `Zone`, `EmailAddress`,
//! `PhoneNumber`) to `Persons.change` and compares what the server received with what was sent.

use std::cell::RefCell;
use std::rc::Rc;

use capnp_rpc::rpc_twoparty_capnp::Side;
use capnp_rpc::{RpcSystem, new_client, twoparty};
use fs_interfaces::people_capnp::{people, persons};
use fs_rpc::catalog::CatalogServer;
use fs_types::values::ContactValue;
use fs_types::{
    BasicType, Edit, EmailAddress, FieldDescription, LocalDateTime, PhoneNumber, TypeDescription,
    TypeRegistry, Utc, Zone,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// The canonical texts of the example values: what a client sends in an `Edit`.
struct Values {
    utc: String,
    local: String,
    zone: String,
    email: String,
    phone: String,
}

fn values() -> Values {
    let utc = Utc::parse("2026-11-16T12:30:45.123+01:00").unwrap();
    let local = LocalDateTime::parse("2026-11-16T13:30:45").unwrap();
    let zone = Zone::iana("Europe/Berlin").unwrap();
    Values {
        utc: utc.to_rfc3339().unwrap(),
        local: local.canonical(),
        zone: zone.id().to_owned(),
        email: EmailAddress::parse("Anna@Bücher.example")
            .unwrap()
            .ascii()
            .to_owned(),
        phone: PhoneNumber::parse("+49 (30) 1234-567")
            .unwrap()
            .as_str()
            .to_owned(),
    }
}

fn registry() -> TypeRegistry {
    let mut reg = TypeRegistry::new();
    reg.add(
        TypeDescription::new(
            "Appointment",
            "fs-calendars",
            "RFC 8984 Event",
            "appointment",
        )
        .field(
            FieldDescription::new("start", BasicType::DateTime)
                .required()
                .label("appointment.start"),
        )
        .field(
            FieldDescription::new("zone", BasicType::Choice)
                .choices(&["utc", "berlin", "new-york"])
                .label("appointment.zone"),
        )
        .field(
            FieldDescription::new("uid", BasicType::Text)
                .read_only()
                .label("appointment.uid"),
        ),
    )
    .unwrap();
    reg.add(
        TypeDescription::new("Person", "fs-people", "RFC 9553 Card", "person")
            .field(
                FieldDescription::new("emails", BasicType::Email)
                    .multiple()
                    .label("person.emails"),
            )
            .field(
                FieldDescription::new("phones", BasicType::Phone)
                    .multiple()
                    .label("person.phones"),
            )
            .field(FieldDescription::new("born", BasicType::Date).label("person.born"))
            .field(
                FieldDescription::new("knows", BasicType::Reference)
                    .target("Person")
                    .multiple()
                    .label("person.knows"),
            ),
    )
    .unwrap();
    reg
}

fn edit(v: &Values) -> Edit {
    Edit::new()
        .set(&["emails"], BasicType::Email, &[&v.email])
        .set(&["phones"], BasicType::Phone, &[&v.phone])
        .set(&["lastSeen"], BasicType::DateTime, &[&v.utc])
        .set(&["localStart"], BasicType::Text, &[&v.local])
        .set(&["zone"], BasicType::Choice, &[&v.zone])
        .clear(&["nickname"], BasicType::Text)
}

/// A `People` server: `types` is the `CatalogServer`, `persons().change` records the edit it receives.
struct PeopleServer {
    registry: TypeRegistry,
    seen: Rc<RefCell<Vec<Edit>>>,
}

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async; nothing to await here
impl people::Server for PeopleServer {
    async fn persons(
        self: Rc<Self>,
        _: people::PersonsParams,
        mut r: people::PersonsResults,
    ) -> Result<(), capnp::Error> {
        r.get()
            .set_persons(new_client(ChangeRecorder(self.seen.clone())));
        Ok(())
    }

    async fn types(
        self: Rc<Self>,
        _: people::TypesParams,
        mut r: people::TypesResults,
    ) -> Result<(), capnp::Error> {
        r.get()
            .set_catalog(new_client(CatalogServer::new(self.registry.clone())));
        Ok(())
    }
}

struct ChangeRecorder(Rc<RefCell<Vec<Edit>>>);

#[allow(clippy::unused_async_trait_impl)] // as above
impl persons::Server for ChangeRecorder {
    async fn change(
        self: Rc<Self>,
        p: persons::ChangeParams,
        _: persons::ChangeResults,
    ) -> Result<(), capnp::Error> {
        self.0.borrow_mut().push(Edit::read(p.get()?.get_edit()?)?);
        Ok(())
    }
}

/// The client half of the check: fetch the catalog, send the edit, compare both with what the server was given.
async fn check(
    client: people::Client,
    sent: &TypeRegistry,
    seen: &Rc<RefCell<Vec<Edit>>>,
    v: &Values,
) {
    let reply = client.types_request().send().promise.await.unwrap();
    let catalog = reply.get().unwrap().get_catalog().unwrap();
    let described = catalog.describe_request().send().promise.await.unwrap();
    let back = TypeRegistry::read(described.get().unwrap().get_types().unwrap()).unwrap();
    assert_eq!(back.len(), sent.len(), "number of types");
    let (got, want): (Vec<_>, Vec<_>) = (back.iter().collect(), sent.iter().collect());
    assert_eq!(got, want, "every type, every field, every choice");
    assert_eq!(
        back.get("Appointment")
            .unwrap()
            .find_field("zone")
            .unwrap()
            .choices,
        ["utc", "berlin", "new-york"]
    );

    let persons = client.persons_request().send().pipeline.get_persons();
    let mut call = persons.change_request();
    call.get().init_id().set_lo(7);
    edit(v).write(call.get().init_edit());
    call.send().promise.await.unwrap();
    assert_eq!(
        *seen.borrow(),
        vec![edit(v)],
        "the edit as the server received it"
    );
    let first = &seen.borrow()[0];
    assert_eq!(
        Utc::parse(&first.fields[2].values[0]).unwrap(),
        Utc::parse(&v.utc).unwrap()
    );
    assert_eq!(
        LocalDateTime::parse(&first.fields[3].values[0])
            .unwrap()
            .canonical(),
        v.local
    );
    assert_eq!(Zone::iana(&first.fields[4].values[0]).unwrap().id(), v.zone);
    assert!(first.fields[5].values.is_empty(), "cleared stays cleared");
}

async fn local<F: Future<Output = ()>>(test: F) {
    tokio::task::LocalSet::new().run_until(test).await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_catalog_and_the_values_cross_an_in_memory_pipe() {
    local(async {
        let v = values();
        let reg = registry();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (end_a, end_b) = tokio::io::duplex(1 << 20);
        let net = |s: tokio::io::DuplexStream, side| {
            let (rd, wr) = tokio::io::split(s);
            Box::new(twoparty::VatNetwork::new(
                rd.compat(),
                wr.compat_write(),
                side,
                capnp::message::ReaderOptions::default(),
            ))
        };
        let server: people::Client = new_client(PeopleServer {
            registry: reg.clone(),
            seen: seen.clone(),
        });
        tokio::task::spawn_local(RpcSystem::new(
            net(end_a, Side::Server),
            Some(server.client),
        ));
        let mut rpc = RpcSystem::new(net(end_b, Side::Client), None);
        let client: people::Client = rpc.bootstrap(Side::Server);
        tokio::task::spawn_local(rpc);
        check(client, &reg, &seen, &v).await;
    })
    .await;
}

/// A folder under the test target (never `/tmp`, which is memory) that goes away with the test, also when it fails.
#[cfg(unix)]
struct Scratch(std::path::PathBuf);

#[cfg(unix)]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn the_catalog_and_the_values_cross_a_unix_socket() {
    local(async {
        let dir = Scratch(
            std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
                .join(format!("catalog-roundtrip-{}", std::process::id())),
        );
        let path = dir.0.join("catalog.sock");
        let v = values();
        let reg = registry();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let bound = fs_rpc::bind_socket(&path).await.unwrap();
        let server: people::Client = new_client(PeopleServer {
            registry: reg.clone(),
            seen: seen.clone(),
        });
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let serving = tokio::task::spawn_local(fs_rpc::serve(bound, server.client, async {
            let _ = stopped.await;
        }));
        let conn = fs_rpc::connect::<people::Client>(&path).await.unwrap();
        check(conn.client.clone(), &reg, &seen, &v).await;
        drop(conn);
        let _ = stop.send(());
        serving.await.unwrap().unwrap();
    })
    .await;
}
