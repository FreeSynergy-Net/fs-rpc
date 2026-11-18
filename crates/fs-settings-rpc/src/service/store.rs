//! `Store`: the port for one program (or for all, `manage`). Scope check here; the port does the rest.
//!
//! Pattern: Adapter (contract -> port `SettingsStore`) + Guard (the scope check).

use crate::Coded;
use std::rc::Rc;

use fs_interfaces::common_capnp::subscription;
use fs_settings::{
    InstanceStore, ProfileCode, ProfileSet, Scope, SettingCode, SettingKey, SettingsStore,
    Snapshot, keys,
};

use super::listeners::Subscription;
use super::{Shared, blocking, err};
use crate::settings_capnp::store;
use crate::wire::{place_of_wire, read_index, read_raw, read_scope, write_index, write_problems};

/// What a `Store` capability may touch.
#[derive(Clone, Debug)]
pub(crate) enum Access {
    /// `open(program)`: `general` and `program(<this one>)`.
    Own(fs_settings::ProgramName),
    /// `manage()`: every scope.
    All,
}

impl Access {
    fn allows(&self, scope: &Scope) -> bool {
        match (self, scope) {
            (Self::All, _) | (_, Scope::General) => true,
            (Self::Own(me), Scope::Program(p)) => me == p,
        }
    }

    /// `write-failed{kind=not-own}`.
    fn refuse(&self, scope: &Scope) -> Result<(), Coded<SettingCode>> {
        if self.allows(scope) {
            Ok(())
        } else {
            Err(not_own())
        }
    }
}

fn not_own() -> Coded<SettingCode> {
    Coded::new(SettingCode::WriteFailed).with_text(keys::KIND, "not-own")
}

pub(crate) struct StoreImpl<S: ?Sized, I: ?Sized> {
    shared: Rc<Shared<S, I>>,
    access: Access,
}

impl<S: ?Sized, I: ?Sized> StoreImpl<S, I> {
    pub(crate) fn new(shared: Rc<Shared<S, I>>, access: Access) -> Self {
        Self { shared, access }
    }
}

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
impl<S, I> store::Server for StoreImpl<S, I>
where
    S: SettingsStore + ?Sized + 'static,
    I: InstanceStore + ?Sized + 'static,
{
    async fn stamp(
        self: Rc<Self>,
        _: store::StampParams,
        mut results: store::StampResults,
    ) -> Result<(), capnp::Error> {
        let port = self.shared.svc.person.clone();
        results
            .get()
            .set_stamp(blocking(move || port.stamp().0).await?);
        Ok(())
    }

    #[allow(clippy::match_wildcard_for_single_variants)] // `Health` is non-exhaustive
    async fn health(
        self: Rc<Self>,
        _: store::HealthParams,
        mut results: store::HealthResults,
    ) -> Result<(), capnp::Error> {
        let port = self.shared.svc.person.clone();
        let health = blocking(move || port.health()).await?;
        let mut out = results.get().init_health();
        match health {
            fs_settings::Health::Defaults(list) => {
                write_problems(
                    out.init_defaults(u32::try_from(list.len()).unwrap_or(0)),
                    &list,
                );
            }
            _ => out.set_ok(()),
        }
        Ok(())
    }

    async fn profile(
        self: Rc<Self>,
        params: store::ProfileParams,
        mut results: store::ProfileResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let scope = read_scope(p.get_scope()?).map_err(|c| err(&c))?;
        let (place, profile) = (place_of_wire(p.get_place()?), p.get_profile());
        let snapshot = if let Err(c) = self.access.refuse(&scope) {
            Snapshot {
                problems: vec![c],
                ..Snapshot::default()
            }
        } else {
            let port = self.shared.svc.person.clone();
            blocking(move || port.profile(&scope, place, profile)).await?
        };
        crate::wire::write_snapshot(results.get().init_snapshot(), &snapshot);
        Ok(())
    }

    async fn write(
        self: Rc<Self>,
        params: store::WriteParams,
        mut results: store::WriteResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let scope = read_scope(p.get_scope()?).map_err(|c| err(&c))?;
        self.access.refuse(&scope).map_err(|c| err(&c))?;
        let (place, profile) = (place_of_wire(p.get_place()?), p.get_profile());
        let key = SettingKey::parse(p.get_key()?.to_str()?).map_err(|c| err(&c))?;
        let raw = if p.get_remove() {
            None
        } else {
            Some(read_raw(p.get_value()?).map_err(|c| err(&c))?)
        };
        let port = self.shared.svc.person.clone();
        let stamp = blocking(move || {
            port.write(&scope, place, profile, &key, raw.as_ref())?;
            Ok::<u64, Coded<SettingCode>>(port.stamp().0)
        })
        .await?
        .map_err(|c| err(&c))?;
        self.shared.listeners.tell(stamp);
        results.get().set_stamp(stamp);
        Ok(())
    }

    async fn clear(
        self: Rc<Self>,
        params: store::ClearParams,
        mut results: store::ClearResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let scope = read_scope(p.get_scope()?).map_err(|c| err(&c))?;
        self.access.refuse(&scope).map_err(|c| err(&c))?;
        let profile = p.get_profile();
        let port = self.shared.svc.person.clone();
        let stamp = blocking(move || {
            port.clear(&scope, profile)?;
            Ok::<u64, Coded<SettingCode>>(port.stamp().0)
        })
        .await?
        .map_err(|c| err(&c))?;
        self.shared.listeners.tell(stamp);
        results.get().set_stamp(stamp);
        Ok(())
    }

    async fn index(
        self: Rc<Self>,
        _: store::IndexParams,
        mut results: store::IndexResults,
    ) -> Result<(), capnp::Error> {
        let port = self.shared.svc.person.clone();
        let (set, problems) = blocking(move || port.index()).await?;
        let mut out = results.get();
        write_index(out.reborrow().init_index(), &set);
        write_problems(
            out.init_problems(u32::try_from(problems.len()).unwrap_or(0)),
            &problems,
        );
        Ok(())
    }

    async fn replace_index(
        self: Rc<Self>,
        params: store::ReplaceIndexParams,
        mut results: store::ReplaceIndexResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let expected = read_index(p.get_expected()?).map_err(|c| err(&c))?;
        let wanted = read_index(p.get_wanted()?).map_err(|c| err(&c))?;
        let port = self.shared.svc.person.clone();
        let (done, stamp) = blocking(move || replace(port.as_ref(), &expected, &wanted))
            .await?
            .map_err(|c| err(&c))?;
        self.shared.listeners.tell(stamp);
        write_index(results.get().init_index(), &done);
        Ok(())
    }

    async fn watch(
        self: Rc<Self>,
        params: store::WatchParams,
        mut results: store::WatchResults,
    ) -> Result<(), capnp::Error> {
        let listener = params.get()?.get_listener()?;
        let id = self.shared.next_id();
        self.shared.listeners.add(id, listener);
        let shared = self.shared.clone();
        let sub = Subscription {
            id,
            remove: move |i| shared.listeners.remove(i),
        };
        results
            .get()
            .set_subscription(capnp_rpc::new_client::<subscription::Client, _>(sub));
        Ok(())
    }
}

impl<F: Fn(u64) + 'static> subscription::Server for Subscription<F> {}

/// Compare-and-set of the index in ONE port transaction: `index-changed` when the stored index is not `expected`; a step that is no
/// legal successor (`fs_settings::index_step_ok`) is `profile{cause=id-invalid}`. Returns the new index and the new stamp.
fn replace<S: SettingsStore + ?Sized>(
    port: &S,
    expected: &ProfileSet<u64>,
    wanted: &ProfileSet<u64>,
) -> Result<(ProfileSet<u64>, u64), Coded<SettingCode>> {
    let mut refused: Option<Coded<SettingCode>> = None;
    let placeholder = |name: &str| Coded::new(ProfileCode::Unknown).with_text("name", name);
    let r = port.update_index(&mut |cur| {
        if cur != expected {
            refused = Some(Coded::new(SettingCode::IndexChanged));
            return Err(placeholder(""));
        }
        if let Err(c) = fs_settings::index_step_ok(cur, wanted) {
            refused = Some(c);
            return Err(placeholder(""));
        }
        *cur = wanted.clone();
        Ok(())
    });
    match (refused, r) {
        (Some(c), _) | (None, Err(c)) => Err(c),
        (None, Ok(done)) => Ok((done, port.stamp().0)),
    }
}
