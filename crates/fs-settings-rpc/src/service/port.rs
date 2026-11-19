//! `Held`: an `Arc` of a (maybe unsized) port as a port again, so the module `fs-settings` (`Admin`) can hold the service's instance port.
//!
//! Pattern: Adapter (a delegate, nothing of its own).

use std::sync::Arc;

use fs_settings::{
    Coded, Health, IndexFn, Instance, InstanceLogEntry, InstanceStore, LockChange, Place,
    ProfileSet, Raw, Scope, SettingCode, SettingKey, SettingsStore, Snapshot, Stamp, Stampd,
};
use fs_types::ObjectId;

/// The port behind an `Arc`.
pub(crate) struct Held<T: ?Sized>(pub(crate) Arc<T>);

impl<T: SettingsStore + ?Sized> SettingsStore for Held<T> {
    fn stamp(&self) -> Stamp {
        self.0.stamp()
    }
    fn health(&self) -> Health {
        self.0.health()
    }
    fn instance(&self) -> Instance {
        self.0.instance()
    }
    fn profile(&self, scope: &Scope, place: Place, profile: u64) -> Snapshot {
        self.0.profile(scope, place, profile)
    }
    fn write(
        &self,
        scope: &Scope,
        place: Place,
        profile: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.0.write(scope, place, profile, key, raw)
    }
    fn clear(&self, scope: &Scope, profile: u64) -> Result<(), Coded<SettingCode>> {
        self.0.clear(scope, profile)
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.0.index()
    }
    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        self.0.update_index(f)
    }
}

impl<T: InstanceStore + ?Sized> InstanceStore for Held<T> {
    fn write_instance(
        &self,
        key: &SettingKey,
        raw: Option<&Raw>,
        who: &Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.0.write_instance(key, raw, who)
    }
    fn write_lock(
        &self,
        key: &SettingKey,
        change: &LockChange,
        who: &Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        self.0.write_lock(key, change, who)
    }
    fn instance_log(&self, after: Option<ObjectId>, size: u32) -> Vec<InstanceLogEntry> {
        self.0.instance_log(after, size)
    }
}
