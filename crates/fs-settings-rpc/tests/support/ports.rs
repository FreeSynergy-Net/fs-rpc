//! Ports for the tests: one that always loses the compare-and-set race, one that notes the caller context.
#![allow(dead_code)]

use std::sync::Arc;

use super::{Coded, IndexFn};
use fs_settings::{
    Health, Instance, InstanceStore, MemoryStore, Place, ProfileSet, Raw, Scope, SettingCode,
    SettingKey, SettingsStore, Snapshot, Stamp,
};

/// A port whose `update_index` always finds somebody else was first, `fail_first` times (`usize::MAX`: always); counts the rounds.
pub struct Flaky {
    pub inner: MemoryStore,
    pub fail_first: usize,
    pub rounds: std::sync::atomic::AtomicUsize,
}

impl SettingsStore for Flaky {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        self.inner.health()
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
        let n = self
            .rounds
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n < self.fail_first {
            // Somebody else changed the index in between: the closure sees another one than the client expected.
            let mut other = ProfileSet::new(999);
            let _ = f(&mut other);
            return Ok(other);
        }
        self.inner.update_index(f)
    }
}

pub fn reference(text: &str) -> fs_types::Reference {
    <fs_types::Reference as fs_types::BasicValue>::parse(text).unwrap()
}

/// Who and when, as the port takes them (the service fills them; the client's is not sent).
pub fn nobody() -> fs_settings::Stampd {
    fs_settings::Stampd {
        by: reference("https://example.net/nobody"),
        at: fs_types::Utc::from_millis(0),
    }
}

/// What a spying instance port saw per write: the OS user of the caller (`current_os_user`), the time, the actor.
pub type Seen = (Option<String>, fs_types::Utc, String);

/// A memory store that notes the caller context of every instance write (the port itself has no place for the OS user).
pub struct Spy {
    pub inner: MemoryStore,
    pub seen: std::sync::Mutex<Vec<Seen>>,
}

impl Spy {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: MemoryStore::new(),
            seen: std::sync::Mutex::new(Vec::new()),
        })
    }
    fn note(&self, who: &fs_settings::Stampd) {
        let canonical = <fs_types::Reference as fs_types::BasicValue>::canonical(&who.by);
        self.seen
            .lock()
            .unwrap()
            .push((fs_settings_rpc::current_os_user(), who.at, canonical));
    }
}

impl SettingsStore for Spy {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        self.inner.health()
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

impl InstanceStore for Spy {
    fn write_instance(
        &self,
        k: &SettingKey,
        r: Option<&Raw>,
        who: &fs_settings::Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.note(who);
        self.inner.write_instance(k, r, who)
    }
    fn write_lock(
        &self,
        k: &SettingKey,
        c: &fs_settings::LockChange,
        who: &fs_settings::Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.note(who);
        self.inner.write_lock(k, c, who)
    }
    fn instance_log(
        &self,
        after: Option<fs_types::ObjectId>,
        size: u32,
    ) -> Vec<fs_settings::InstanceLogEntry> {
        self.inner.instance_log(after, size)
    }
}

/// A port whose `profile` BLOCKS (a database that takes its time); everything else is quick.
pub struct Slow {
    pub inner: MemoryStore,
    pub delay: std::time::Duration,
}

impl SettingsStore for Slow {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        self.inner.health()
    }
    fn instance(&self) -> Instance {
        self.inner.instance()
    }
    fn profile(&self, s: &Scope, p: Place, id: u64) -> Snapshot {
        std::thread::sleep(self.delay);
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
