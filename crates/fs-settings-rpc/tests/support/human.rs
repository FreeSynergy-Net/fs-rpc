//! A world with a key that only a person may touch: one store, a service for a bot connection and one for a person.
#![allow(dead_code)]

use std::sync::Arc;

use fs_settings::{
    Catalog, Declaration, GroupId, InstanceStore, Kind, LockChange, MemoryStore, Place, Raw, Scope,
    SettingsStore, Snapshot, Value,
};
use fs_settings_rpc::{RpcInstanceAdmin, RpcSettingsStore, SettingsService};
use tempfile::TempDir;

use super::{Server, dir, key, nobody, serve_on, times};

pub const HIDDEN: &str = "fs.mcp.enabled";
pub const OPEN: &str = "fs.look.compact";
pub const NOPE: &str = "fs.look.nothing";

/// The declarations: the human-only switch and one open key.
pub fn catalog() -> Catalog {
    let decl = |k: &str, g: &str| {
        Declaration::new(
            key(k),
            Kind::Flag,
            Value::Flag(false),
            GroupId::new(g).unwrap(),
        )
    };
    Catalog::of(vec![decl(HIDDEN, "ai").human_only(), decl(OPEN, "look")]).unwrap()
}

type Svc = SettingsService<dyn SettingsStore, MemoryStore>;

/// A service over `s` with the catalog; the default fail-closed one (every connection a bot) or one where the own OS user is a person.
pub fn service(s: &MemoryStore, person: bool) -> Svc {
    let port: Arc<dyn SettingsStore> = Arc::new(s.clone());
    let inst = Some(Arc::new(s.clone()));
    let mut svc = if person {
        SettingsService::new_os_user_is_person(port, inst)
    } else {
        SettingsService::new(port, inst)
    };
    svc.catalog = catalog();
    svc
}

pub struct World {
    _tmp: TempDir,
    pub store: MemoryStore,
    pub bot: Server,
    pub person: Server,
}

/// Both keys set in the person's profile, as instance defaults and in the log; the hidden one is locked when `locked`.
pub fn world(locked: bool) -> World {
    let (tmp, store) = (dir(), MemoryStore::new());
    for k in [HIDDEN, OPEN] {
        let raw = Raw::Bool(true);
        store
            .write(&Scope::General, Place::Profile, 0, &key(k), Some(&raw))
            .unwrap();
        store
            .write_instance(&key(k), Some(&raw), &nobody())
            .unwrap();
    }
    if locked {
        let lock = LockChange::Lock {
            value: Raw::Bool(true),
            reason: "kept".to_owned(),
        };
        store.write_lock(&key(HIDDEN), &lock, &nobody()).unwrap();
    }
    let (b, p) = (store.clone(), store.clone());
    World {
        bot: serve_on(&tmp.path().join("b.sock"), move || service(&b, false)),
        person: serve_on(&tmp.path().join("p.sock"), move || service(&p, true)),
        store,
        _tmp: tmp,
    }
}

pub fn manage(s: &Server) -> RpcSettingsStore {
    RpcSettingsStore::manage(&s.socket, times())
}

pub fn admin(s: &Server) -> RpcInstanceAdmin {
    RpcInstanceAdmin::open(&s.socket, "claim", times())
}

pub fn keys_of(snap: &Snapshot) -> Vec<String> {
    snap.values.keys().map(|k| k.as_str().to_owned()).collect()
}

pub fn log_keys(a: &RpcInstanceAdmin) -> Vec<String> {
    a.instance_log(None, 50)
        .iter()
        .map(|e| e.key.as_str().to_owned())
        .collect()
}
