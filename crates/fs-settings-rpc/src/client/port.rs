//! `impl SettingsStore for RpcSettingsStore`: each method one call of the contract.
//!
//! Pattern: Proxy. `update_index` is compare-and-set: read the index, let the closure change a copy, `replaceIndex(expected,
//! wanted)`; the service answers `index-changed` when somebody else changed it in between, then the next round (at most 3).

use std::collections::BTreeMap;
use std::sync::PoisonError;

use fs_settings::{
    Coded, Health, IndexFn, Instance, Place, ProfileCode, ProfileSet, Raw, Scope, SettingCode,
    SettingKey, SettingsStore, Snapshot, Stamp, keys,
};

use super::RpcSettingsStore;
use super::cache::Cache;
use super::worker::CallError;
use crate::wire::{
    place_to_wire, read_index, read_problems, read_snapshot, write_index, write_raw, write_scope,
};

/// The index and what went wrong reading it.
type IndexRead = (ProfileSet<u64>, Vec<Coded<SettingCode>>);

/// Rounds of the compare-and-set of `update_index` before `store-busy`.
pub const INDEX_ROUNDS: usize = 3;

impl RpcSettingsStore {
    fn cache(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Notes the outcome of a call: answered (and the stamp the service gave, if any) or not.
    fn note<T>(&self, r: &Result<T, Coded<SettingCode>>, stamp: impl FnOnce(&T) -> Option<u64>) {
        let mut c = self.cache();
        match r {
            Err(e) if e.code() == SettingCode::StoreUnreachable => c.lost(),
            Ok(t) => match stamp(t) {
                Some(s) => c.seen(s),
                None => c.seen_up(),
            },
            Err(_) => c.seen_up(),
        }
    }

    fn fresh_index(&self) -> Result<IndexRead, Coded<SettingCode>> {
        let r = self.worker.call(|l| {
            Box::pin(async move {
                let resp = l.store.index_request().send().promise.await?;
                let r = resp.get()?;
                Ok((
                    read_index(r.get_index()?)?,
                    read_problems(r.get_problems()?)?,
                ))
            })
        });
        self.note(&r, |_| None);
        r
    }

    fn write_call(
        &self,
        scope: &Scope,
        place: Place,
        profile: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        let (scope, key, raw) = (scope.clone(), key.as_str().to_owned(), raw.cloned());
        let r = self.worker.call(move |l| {
            Box::pin(async move {
                let mut req = l.store.write_request();
                {
                    let mut b = req.get();
                    write_scope(b.reborrow().init_scope(), &scope);
                    b.set_place(place_to_wire(place));
                    b.set_profile(profile);
                    b.set_key(key.as_str());
                    match &raw {
                        Some(r) => write_raw(b.reborrow().init_value(), r),
                        None => b.set_remove(true),
                    }
                }
                Ok::<u64, CallError>(req.send().promise.await?.get()?.get_stamp())
            })
        });
        self.note(&r, |s| Some(*s));
        r.map(|_| ())
    }
}

impl SettingsStore for RpcSettingsStore {
    fn stamp(&self) -> Stamp {
        let r = self.worker.call(|l| {
            Box::pin(async move {
                Ok::<u64, CallError>(
                    l.store
                        .stamp_request()
                        .send()
                        .promise
                        .await?
                        .get()?
                        .get_stamp(),
                )
            })
        });
        self.note(&r, |s| Some(*s));
        self.cache().stamp()
    }

    fn health(&self) -> Health {
        let r = self.worker.call(|l| {
            Box::pin(async move {
                let resp = l.store.health_request().send().promise.await?;
                match resp
                    .get()?
                    .get_health()?
                    .which()
                    .map_err(capnp::Error::from)?
                {
                    crate::settings_capnp::health::Ok(()) => Ok(Health::Ok),
                    crate::settings_capnp::health::Defaults(list) => {
                        Ok(Health::Defaults(read_problems(list?)?))
                    }
                }
            })
        });
        self.note(&r, |_| None);
        r.unwrap_or_else(|e| Health::Defaults(vec![e]))
    }

    fn instance(&self) -> Instance {
        if let Some(i) = self.cache().instance.clone() {
            return i;
        }
        let r = self.worker.call(|l| {
            Box::pin(async move {
                let resp = l.hub.instance_request().send().promise.await?;
                let r = resp.get()?;
                let values = read_snapshot(r.get_values()?)?;
                let mut locks = BTreeMap::new();
                for e in r.get_locks()? {
                    let key = SettingKey::parse(e.get_key()?.to_str()?)?;
                    locks.insert(key, crate::wire::read_lock(e.get_lock()?)?);
                }
                Ok::<Instance, CallError>(Instance { values, locks })
            })
        });
        self.note(&r, |_| None);
        let i = r.unwrap_or_default();
        let mut c = self.cache();
        if c.reachable() {
            c.instance = Some(i.clone());
        }
        i
    }

    fn profile(&self, scope: &Scope, place: Place, profile: u64) -> Snapshot {
        let id = (scope.clone(), place == Place::Device, profile);
        if let Some(s) = self.cache().snapshots.get(&id) {
            return s.clone();
        }
        let scope = scope.clone();
        let r = self.worker.call(move |l| {
            Box::pin(async move {
                let mut req = l.store.profile_request();
                {
                    let mut b = req.get();
                    write_scope(b.reborrow().init_scope(), &scope);
                    b.set_place(place_to_wire(place));
                    b.set_profile(profile);
                }
                let resp = req.send().promise.await?;
                Ok::<Snapshot, CallError>(read_snapshot(resp.get()?.get_snapshot()?)?)
            })
        });
        self.note(&r, |_| None);
        match r {
            Ok(s) => {
                self.cache().snapshots.insert(id, s.clone());
                s
            }
            Err(_) => Snapshot::default(),
        }
    }

    fn write(
        &self,
        scope: &Scope,
        place: Place,
        profile: u64,
        key: &SettingKey,
        raw: Option<&Raw>,
    ) -> Result<(), Coded<SettingCode>> {
        self.write_call(scope, place, profile, key, raw)
    }

    fn clear(&self, scope: &Scope, profile: u64) -> Result<(), Coded<SettingCode>> {
        let scope = scope.clone();
        let r = self.worker.call(move |l| {
            Box::pin(async move {
                let mut req = l.store.clear_request();
                {
                    let mut b = req.get();
                    write_scope(b.reborrow().init_scope(), &scope);
                    b.set_profile(profile);
                }
                Ok::<u64, CallError>(req.send().promise.await?.get()?.get_stamp())
            })
        });
        self.note(&r, |s| Some(*s));
        r.map(|_| ())
    }

    fn index(&self) -> (ProfileSet<u64>, Vec<Coded<SettingCode>>) {
        if let Some(i) = self.cache().index.clone() {
            return i;
        }
        match self.fresh_index() {
            Ok(i) => {
                self.cache().index = Some(i.clone());
                i
            }
            Err(e) => (ProfileSet::default(), vec![e]),
        }
    }

    fn update_index(&self, f: &mut IndexFn<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
        for _ in 0..INDEX_ROUNDS {
            let (current, _) = self.fresh_index()?;
            let mut wanted = current.clone();
            f(&mut wanted).map_err(|p| profile_problem(&p))?;
            let r = self.worker.call(move |l| {
                Box::pin(async move {
                    let mut req = l.store.replace_index_request();
                    {
                        let mut b = req.get();
                        write_index(b.reborrow().init_expected(), &current);
                        write_index(b.init_wanted(), &wanted);
                    }
                    let resp = req.send().promise.await?;
                    Ok::<ProfileSet<u64>, CallError>(read_index(resp.get()?.get_index()?)?)
                })
            });
            self.note(&r, |_| None);
            match r {
                Ok(set) => {
                    self.cache().index = None;
                    return Ok(set);
                }
                Err(e) if e.code() == SettingCode::IndexChanged => {}
                Err(e) => return Err(e),
            }
        }
        Err(Coded::new(SettingCode::StoreBusy)
            .with_text(keys::SOCKET, self.socket.display().to_string()))
    }
}

/// A profile code of the closure as the setting code `profile{cause, name}` (the same shape the stores answer).
pub(crate) fn profile_problem(p: &Coded<ProfileCode>) -> Coded<SettingCode> {
    use fs_settings::coded_stub::{ErrorCode, Param};
    let mut c = Coded::new(SettingCode::Profile).with_text(keys::CAUSE, p.code().as_str());
    for (k, v) in p.params() {
        if let Param::Text(t) = v {
            c = c.with_text(k, t.clone());
        }
    }
    c
}
