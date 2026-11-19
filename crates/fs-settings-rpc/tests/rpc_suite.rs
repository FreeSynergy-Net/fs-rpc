//! KB18: the suite `store` of fs-settings over RPC. 11 rules conform; `store-health-said` is skipped and listed (a client over a socket
//! cannot damage the storage under the service); the health the service says has its own test (`rpc_health_watch`).
#![cfg(feature = "conformance")]

mod support;

use std::sync::Arc;

use fs_conformance::{Need, Options, RuleId, run};
use fs_settings::suite::{StoreFixture, StoreSuite};
use fs_settings::{InstanceStore, MemoryStore, SettingsStore};
use fs_settings_rpc::SettingsService;
use support::{Combined, Server, dir, serve_on};
use tempfile::TempDir;

struct RpcFixture {
    dir: TempDir,
    n: usize,
    servers: Vec<Server>,
}

impl StoreFixture for RpcFixture {
    fn fresh(&mut self) -> Arc<dyn InstanceStore> {
        self.n += 1;
        let socket = self.dir.path().join(format!("s{}.sock", self.n));
        let store = MemoryStore::new();
        self.servers.push(serve_on(&socket, move || {
            let person: Arc<dyn SettingsStore> = Arc::new(store.clone());
            let mut svc = SettingsService::new_os_user_is_person(person, Some(Arc::new(store)));
            // The suite expects the actor it passes in `Stampd` (`https://example.org/people/suite`); over RPC the actor is the
            // connection's, so the fixture's connection IS that actor.
            svc.actor_of = Arc::new(|_| support::reference("https://example.org/people/suite"));
            svc
        }));
        Arc::new(Combined::open(&socket))
    }

    fn break_store(&mut self, _: &dyn InstanceStore) -> bool {
        false
    }
}

#[test]
fn kb18_store_suite_over_rpc() {
    // A plain thread without a runtime: the port is sync and blocks (KB17).
    let done = std::thread::spawn(|| {
        let mut fixture = RpcFixture {
            dir: dir(),
            n: 0,
            servers: Vec::new(),
        };
        let options = Options {
            partial: true,
            ..Options::default()
        };
        futures::executor::block_on(run(&StoreSuite, &mut fixture, &options))
    })
    .join()
    .unwrap();
    assert!(done.is_conformant(), "{:?}", done.failures);
    assert_eq!(done.checked, 11);
    assert_eq!(done.skipped.len(), 1);
    assert_eq!(done.skipped[0].rule, RuleId::new("store-health-said"));
    assert_eq!(done.skipped[0].need, Need::new("fixture.break-store"));
}
