//! More ports for the tests: one in emergency mode, one that is "a store" for the suite (person + administrator over one service).
#![allow(dead_code)]

use super::{Coded, IndexFn};
use fs_settings::{
    Health, Instance, InstanceStore, MemoryStore, Place, ProfileSet, Raw, Scope, SettingCode,
    SettingKey, SettingsStore, Snapshot, Stamp,
};

/// A port that serves only the built-in defaults and says why (a damaged database in emergency mode).
pub struct Degraded {
    pub inner: MemoryStore,
}

pub fn degraded_codes() -> Vec<Coded<SettingCode>> {
    vec![
        Coded::new(SettingCode::StoreBroken)
            .with_text("file", "settings.db")
            .with_text("cause", "header"),
        Coded::new(SettingCode::StoreNewer)
            .with_text("file", "settings.db")
            .with_number("found", 9)
            .with_number("known", 5),
    ]
}

impl SettingsStore for Degraded {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        Health::Defaults(degraded_codes())
    }
    fn instance(&self) -> Instance {
        self.inner.instance()
    }
    fn profile(&self, s: &Scope, p: Place, id: u64) -> Snapshot {
        self.inner.profile(s, p, id)
    }
    fn write(
        &self,
        s: &Scope,
        p: Place,
        id: u64,
        k: &SettingKey,
        r: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.inner.write(s, p, id, k, r)
    }
    fn clear(&self, s: &Scope, id: u64) -> Result<(), Coded<SettingCode>> {
        self.inner.clear(s, id)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.inner.index()
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.inner.update_index(f)
    }
}

/// The person's store (all scopes) and the administrator over ONE service: what the suite `store` calls "a store".
pub struct Combined {
    person: fs_settings_rpc::RpcSettingsStore,
    admin: fs_settings_rpc::RpcInstanceAdmin,
}

impl Combined {
    pub fn open(socket: &std::path::Path) -> Self {
        let times = super::times();
        Self {
            person: fs_settings_rpc::RpcSettingsStore::manage(socket, times),
            admin: fs_settings_rpc::RpcInstanceAdmin::open(
                socket,
                "https://example.net/admin",
                times,
            ),
        }
    }
}

impl Combined {
    /// The administrator wrote through another handle: as the shell does, ask for the stamp before reading (a changed stamp drops the
    /// client's cache).
    fn refresh(&self) {
        let _ = self.person.stamp();
    }
}

impl SettingsStore for Combined {
    fn stamp(&self) -> Stamp {
        self.person.stamp()
    }
    fn health(&self) -> Health {
        self.person.health()
    }
    fn instance(&self) -> Instance {
        self.refresh();
        self.person.instance()
    }
    fn profile(&self, s: &Scope, p: Place, id: u64) -> Snapshot {
        self.refresh();
        self.person.profile(s, p, id)
    }
    fn write(
        &self,
        s: &Scope,
        p: Place,
        id: u64,
        k: &SettingKey,
        r: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.person.write(s, p, id, k, r)
    }
    fn clear(&self, s: &Scope, id: u64) -> Result<(), Coded<SettingCode>> {
        self.person.clear(s, id)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.refresh();
        self.person.index()
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.person.update_index(f)
    }
}

impl InstanceStore for Combined {
    fn write_instance(
        &self,
        k: &SettingKey,
        r: Option<&Raw>,
        who: &fs_settings::Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.admin.write_instance(k, r, who)
    }
    fn write_lock(
        &self,
        k: &SettingKey,
        c: &fs_settings::LockChange,
        who: &fs_settings::Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.admin.write_lock(k, c, who)
    }
    fn instance_log(
        &self,
        after: Option<fs_types::ObjectId>,
        size: u32,
    ) -> Vec<fs_settings::InstanceLogEntry> {
        self.admin.instance_log(after, size)
    }
}
