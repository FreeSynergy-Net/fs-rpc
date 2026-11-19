//! `Admin`: the port `InstanceStore` for ONE actor. The actor is the identity of the CONNECTION (never the `by` a client claims);
//! time and OS user are the service's.
//!
//! Pattern: Adapter (contract -> port `InstanceStore`).

use crate::Coded;
use std::rc::Rc;
use std::sync::Arc;

use fs_settings::{
    Actor, InstanceStore, LockChange, SettingCode, SettingKey, SettingsStore, Stampd, keys,
};
use fs_types::Reference;

use super::{Shared, blocking, err};
use crate::settings_capnp::admin;
use crate::wire::{read_id, read_raw, write_log_entry};

/// The most log entries one answer holds (a client reads on with `after`).
pub const MAX_LOG: u32 = 200;

pub(crate) struct AdminImpl<S: ?Sized, I: ?Sized> {
    shared: Rc<Shared<S, I>>,
    instance: Arc<I>,
    actor: Reference,
    os_user: Option<String>,
    /// Person or bot of the connection: every key it names goes through the guard.
    kind: Actor,
}

impl<S: ?Sized, I: ?Sized> AdminImpl<S, I> {
    pub(crate) fn new(
        shared: Rc<Shared<S, I>>,
        instance: Arc<I>,
        who: (Reference, Option<String>),
        kind: Actor,
    ) -> Self {
        Self {
            shared,
            instance,
            actor: who.0,
            os_user: who.1,
            kind,
        }
    }

    /// The key a call names, refused like an undeclared one when the connection cannot see it.
    fn key(&self, text: &str) -> Result<SettingKey, capnp::Error> {
        let key = SettingKey::parse(text).map_err(|c| err(&c))?;
        self.shared
            .guard
            .refuse(self.kind, &key)
            .map_err(|c| err(&c))?;
        Ok(key)
    }

    /// Who and when: the actor of the connection, the service's clock and the OS user the service saw.
    fn who(&self) -> Stampd {
        Stampd {
            by: self.actor.clone(),
            at: (self.shared.svc.now)(),
        }
    }
}

fn reason_missing(key: &SettingKey) -> capnp::Error {
    err(&Coded::new(SettingCode::ReasonMissing).with_text(keys::KEY, key.as_str()))
}

impl<S, I> AdminImpl<S, I>
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    /// Runs one instance write on the blocking pool and tells the watchers when something changed.
    async fn write(
        &self,
        f: impl FnOnce(&I, &Stampd) -> Result<bool, Coded<SettingCode>> + Send + 'static,
    ) -> Result<bool, capnp::Error> {
        let (port, who) = (self.instance.clone(), self.who());
        let os_user = self.os_user.clone();
        let changed = blocking(move || crate::caller::scoped(os_user, || f(port.as_ref(), &who)))
            .await?
            .map_err(|c| err(&c))?;
        if changed {
            let port = self.shared.svc.person.clone();
            self.shared
                .listeners
                .tell(blocking(move || port.stamp().0).await?);
        }
        Ok(changed)
    }
}

impl<S, I> admin::Server for AdminImpl<S, I>
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    async fn set(
        self: Rc<Self>,
        params: admin::SetParams,
        mut results: admin::SetResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let key = self.key(p.get_key()?.to_str()?)?;
        let raw = read_raw(p.get_value()?).map_err(|c| err(&c))?;
        let changed = self
            .write(move |port, who| port.write_instance(&key, Some(&raw), who))
            .await?;
        results.get().set_changed(changed);
        Ok(())
    }

    async fn clear(
        self: Rc<Self>,
        params: admin::ClearParams,
        mut results: admin::ClearResults,
    ) -> Result<(), capnp::Error> {
        let key = self.key(params.get()?.get_key()?.to_str()?)?;
        let changed = self
            .write(move |port, who| port.write_instance(&key, None, who))
            .await?;
        results.get().set_changed(changed);
        Ok(())
    }

    async fn lock(
        self: Rc<Self>,
        params: admin::LockParams,
        mut results: admin::LockResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let key = self.key(p.get_key()?.to_str()?)?;
        let value = read_raw(p.get_value()?).map_err(|c| err(&c))?;
        let reason = p.get_reason()?.to_str()?.to_owned();
        if reason.trim().is_empty() {
            return Err(reason_missing(&key));
        }
        let change = LockChange::Lock { value, reason };
        let changed = self
            .write(move |port, who| port.write_lock(&key, &change, who))
            .await?;
        results.get().set_changed(changed);
        Ok(())
    }

    async fn unlock(
        self: Rc<Self>,
        params: admin::UnlockParams,
        mut results: admin::UnlockResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let key = self.key(p.get_key()?.to_str()?)?;
        let reason = p.get_reason()?.to_str()?.to_owned();
        if reason.trim().is_empty() {
            return Err(reason_missing(&key));
        }
        let change = LockChange::Unlock { reason };
        let changed = self
            .write(move |port, who| port.write_lock(&key, &change, who))
            .await?;
        results.get().set_changed(changed);
        Ok(())
    }

    async fn log(
        self: Rc<Self>,
        params: admin::LogParams,
        mut results: admin::LogResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let after = read_id(p.get_after()?);
        let size = p.get_size().clamp(1, MAX_LOG);
        let (port, guard, kind) = (self.instance.clone(), self.shared.guard.clone(), self.kind);
        let page = blocking(move || guard.log(kind, &port, after, size)).await?;
        // The wire has no `cut`: a page that stopped at the read limit with nothing to show is an error, never an empty page that
        // looks like the end of the log (the client turns repeats of it into `history-stuck`).
        if page.lines.is_empty() && page.cut {
            return Err(crate::problem::history_cut());
        }
        let entries = page.lines;
        let mut list = results
            .get()
            .init_entries(u32::try_from(entries.len()).unwrap_or(0));
        for (i, e) in (0..).zip(&entries) {
            write_log_entry(list.reborrow().get(i), e);
        }
        Ok(())
    }
}
