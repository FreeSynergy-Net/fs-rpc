//! The administrator's client: [`RpcInstanceAdmin`] is the port `InstanceStore` over `SettingsHub.admin(by)`.
//!
//! Pattern: Adapter. Time and OS user are set by the service, never by the caller: the `Stampd` of the port is not sent (only
//! `by`, chosen at connection). Reading goes through `manage()`; writing the person's values is refused here (`read-only{which=instance}`).

use std::path::Path;

use fs_settings::{
    Coded, Health, IndexFn, Instance, InstanceLogEntry, InstanceStore, LockChange, Place,
    ProfileSet, Raw, Scope, SettingCode, SettingKey, SettingsStore, Snapshot, Stamp, Stampd, keys,
};
use fs_types::ObjectId;

use super::RpcSettingsStore;
use super::worker::{CallError, ClientTimes, Role};
use crate::wire::{read_log_entry, write_id, write_raw};

/// The admin client. `by` is the actor IRI (mandatory); the service writes it into the log.
pub struct RpcInstanceAdmin {
    inner: RpcSettingsStore,
}

impl RpcInstanceAdmin {
    /// Connects for the actor `by`. Like [`RpcSettingsStore::open`] it does not fail without a service.
    #[must_use]
    pub fn open(socket: &Path, by: &str, times: ClientTimes) -> Self {
        Self {
            inner: RpcSettingsStore::with_role(socket, Role::Admin(by.to_owned()), times),
        }
    }

    fn read_only() -> Coded<SettingCode> {
        Coded::new(SettingCode::ReadOnly).with_text(keys::WHICH, "instance")
    }
}

impl SettingsStore for RpcInstanceAdmin {
    fn stamp(&self) -> Stamp {
        self.inner.stamp()
    }
    fn health(&self) -> Health {
        self.inner.health()
    }
    fn instance(&self) -> Instance {
        self.inner.instance()
    }
    fn profile(&self, scope: &Scope, place: Place, profile: u64) -> Snapshot {
        self.inner.profile(scope, place, profile)
    }
    fn write(
        &self,
        _: &Scope,
        _: Place,
        _: u64,
        _: &SettingKey,
        _: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        Err(Self::read_only())
    }
    fn clear(&self, _: &Scope, _: u64) -> Result<(), Coded<SettingCode>> {
        Err(Self::read_only())
    }
    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        self.inner.index()
    }
    fn update_index(&self, _: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        Err(Self::read_only())
    }
}

impl InstanceStore for RpcInstanceAdmin {
    fn write_instance(
        &self,
        key: &SettingKey,
        raw: Option<&Raw>,
        _: &Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        let (key, raw) = (key.as_str().to_owned(), raw.cloned());
        self.inner.worker.call(move |l| {
            Box::pin(async move {
                let admin = l
                    .admin
                    .ok_or_else(|| capnp::Error::failed("no admin".into()))?;
                let changed = if let Some(r) = raw {
                    let mut req = admin.set_request();
                    req.get().set_key(key.as_str());
                    write_raw(req.get().init_value(), &r);
                    req.send().promise.await?.get()?.get_changed()
                } else {
                    let mut req = admin.clear_request();
                    req.get().set_key(key.as_str());
                    req.send().promise.await?.get()?.get_changed()
                };
                Ok::<bool, CallError>(changed)
            })
        })
    }

    fn write_lock(
        &self,
        key: &SettingKey,
        change: &LockChange,
        _: &Stampd,
    ) -> Result<bool, Coded<SettingCode>> {
        let (key, change) = (key.as_str().to_owned(), change.clone());
        self.inner.worker.call(move |l| {
            Box::pin(async move {
                let admin = l
                    .admin
                    .ok_or_else(|| capnp::Error::failed("no admin".into()))?;
                Ok::<bool, CallError>(match change {
                    LockChange::Lock { value, reason } => {
                        let mut req = admin.lock_request();
                        req.get().set_key(key.as_str());
                        write_raw(req.get().init_value(), &value);
                        req.get().set_reason(reason.as_str());
                        req.send().promise.await?.get()?.get_changed()
                    }
                    LockChange::Unlock { reason } => {
                        let mut req = admin.unlock_request();
                        req.get().set_key(key.as_str());
                        req.get().set_reason(reason.as_str());
                        req.send().promise.await?.get()?.get_changed()
                    }
                })
            })
        })
    }

    fn instance_log(&self, after: Option<ObjectId>, size: u32) -> Vec<InstanceLogEntry> {
        self.inner
            .worker
            .call(move |l| {
                Box::pin(async move {
                    let admin = l
                        .admin
                        .ok_or_else(|| capnp::Error::failed("no admin".into()))?;
                    let mut req = admin.log_request();
                    write_id(req.get().init_after(), after);
                    req.get().set_size(size);
                    let resp = req.send().promise.await?;
                    let mut out = Vec::new();
                    for e in resp.get()?.get_entries()? {
                        out.push(read_log_entry(e)?);
                    }
                    Ok::<Vec<InstanceLogEntry>, CallError>(out)
                })
            })
            .unwrap_or_default()
    }
}
