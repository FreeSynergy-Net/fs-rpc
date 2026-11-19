//! `Guard`: the ONE place where the service asks what an actor may see: every way a bot can reach (profile read, write, clear, health,
//! index problems, the instance layer, the log) asks here, and here the answer comes from `fs_settings::Surface` ([`Surface::sees`], [`Surface::history`]),
//! never from a rule of this crate. A person is served as before (no filter); every other actor is restricted like a bot, also one added later.
//!
//! A restricted actor sees a key only when the catalog declares it and does not declare it `human_only`; a refused write answers like a key
//! nobody declared (`not-declared`), a read leaves the key out, a problem that names a hidden key is dropped (and the `line` of any problem,
//! as `Surface::problems` does). A service without a catalog therefore shows a bot nothing (fail closed). `watch` tells the stamp only.
//!
//! Pattern: Guard + Proxy (a filtered view over the port; the rule itself is the module's).

use std::sync::Arc;

use fs_settings::{
    Actor, Admin, Catalog, Coded, Instance, InstanceLogEntry, InstanceStore, NoLockRights, Place,
    ProgramName, Scope, SettingCode, SettingKey, SettingsStore, Snapshot, Surface, keys,
    system_clock,
};
use fs_types::coded::Param;
use fs_types::{BasicValue, ObjectId, Reference};

use super::port::Held;

/// The page of the log one answer holds, and whether the read stopped at its limit (`HistoryPage::cut`).
pub(crate) struct LogPage {
    pub(crate) lines: Vec<InstanceLogEntry>,
    pub(crate) cut: bool,
}

pub(crate) struct Guard {
    /// Only the carrier of the catalog for `Surface` (over an empty store: no port is read for it).
    settings: fs_settings::Settings,
}

impl Guard {
    pub(crate) fn new(catalog: Catalog) -> Self {
        let program = ProgramName::new("settings").expect("a valid program name");
        let store: Arc<dyn SettingsStore> = Arc::new(fs_settings::MemoryStore::new());
        Self {
            settings: fs_settings::Settings::open(
                program,
                catalog,
                store,
                Arc::new(fs_settings::NoSystem),
            ),
        }
    }

    fn surface(&self, actor: Actor) -> Surface<'_> {
        Surface::new(&self.settings, actor)
    }

    fn person(actor: Actor) -> bool {
        matches!(actor, Actor::Person)
    }

    /// Whether `key` exists for `actor`.
    pub(crate) fn visible(&self, actor: Actor, key: &SettingKey) -> bool {
        Self::person(actor) || self.surface(actor).sees(key)
    }

    /// A write, clear or lock of `key` is refused like a key nobody declared.
    pub(crate) fn refuse(&self, actor: Actor, key: &SettingKey) -> Result<(), Coded<SettingCode>> {
        if self.visible(actor, key) {
            Ok(())
        } else {
            Err(Coded::new(SettingCode::NotDeclared).with_text(keys::KEY, key.as_str()))
        }
    }

    /// `snapshot` as `actor` sees it.
    pub(crate) fn snapshot(&self, actor: Actor, mut snapshot: Snapshot) -> Snapshot {
        if !Self::person(actor) {
            snapshot.values.retain(|k, _| self.visible(actor, k));
            snapshot.problems = self.problems(actor, snapshot.problems);
        }
        snapshot
    }

    /// The instance layer as `actor` sees it.
    pub(crate) fn instance(&self, actor: Actor, mut held: Instance) -> Instance {
        if !Self::person(actor) {
            held.locks.retain(|k, _| self.visible(actor, k));
            held.values = self.snapshot(actor, held.values);
        }
        held
    }

    /// The problems without those that name a key `actor` cannot see, and without any line number.
    pub(crate) fn problems(
        &self,
        actor: Actor,
        list: Vec<Coded<SettingCode>>,
    ) -> Vec<Coded<SettingCode>> {
        if Self::person(actor) {
            return list;
        }
        let hidden = |p: &Coded<SettingCode>| {
            p.params().any(|(k, v)| {
                k == keys::KEY
                    && matches!(v, Param::Text(t) if SettingKey::parse(t).is_ok_and(|key| {
                        self.settings.catalog().get(&key).is_some() && !self.visible(actor, &key)
                    }))
            })
        };
        list.into_iter()
            .filter(|p| !hidden(p))
            .map(|p| {
                p.params()
                    .filter(|(k, _)| *k != keys::LINE)
                    .fold(Coded::new(p.code()), |c, (k, v)| c.with_param(k, v.clone()))
            })
            .collect()
    }

    /// `clear` of a whole profile, for a restricted actor: only the keys it can see are removed.
    pub(crate) fn clear<S: SettingsStore + ?Sized>(
        &self,
        actor: Actor,
        port: &S,
        scope: &Scope,
        profile: u64,
    ) -> Result<(), Coded<SettingCode>> {
        if Self::person(actor) {
            return port.clear(scope, profile);
        }
        for place in [Place::Profile, Place::Device] {
            for key in port.profile(scope, place, profile).values.keys() {
                if self.visible(actor, key) {
                    port.write(scope, place, profile, key, None)?;
                }
            }
        }
        Ok(())
    }

    /// The page of the instance log after `after`: for a restricted actor the filtered, bounded read of the module.
    pub(crate) fn log<I: InstanceStore + ?Sized + 'static>(
        &self,
        actor: Actor,
        instance: &Arc<I>,
        after: Option<ObjectId>,
        size: u32,
    ) -> LogPage {
        if Self::person(actor) {
            return LogPage {
                lines: instance.instance_log(after, size),
                cut: false,
            };
        }
        let admin = Admin {
            store: Arc::new(Held(instance.clone())),
            rights: Arc::new(NoLockRights),
            by: Reference::parse("urn:fs:os-user:guard").expect("a valid IRI"),
            catalog: self.settings.catalog().clone(),
            clock: system_clock,
        };
        let page = self.surface(actor).history(&admin, after, size);
        LogPage {
            lines: page.lines,
            cut: page.cut,
        }
    }
}
