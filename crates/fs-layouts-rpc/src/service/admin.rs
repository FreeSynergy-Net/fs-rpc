//! `admin(by)` and the capability `LayoutAdmin`: lock, unlock, instance overlay. Only the instance service has them; the person
//! service answers `unsupported{what=admin}` (v1). The reason of a lock is mandatory (the port checks it: `reason-missing`).
//!
//! Pattern: Adapter (contract -> port `LayoutAdmin`) + Guard (`by` is mandatory).

use std::rc::Rc;

use fs_forms_layout::{
    Coded, ElemId, ElemKind, ElemRef, LayoutAdmin, LayoutCode, LayoutStore, Reference, Who,
};

use super::{Shared, listeners::Listeners};
use crate::fail::H;
use crate::layouts_capnp::{layout_admin as a, layouts as l};
use crate::wire::ids::{layout_id, write_id};
use crate::wire::text::{bad, opt, txt};

pub(crate) async fn admin<S: LayoutStore + ?Sized + 'static>(
    sh: &Rc<Shared<S>>,
    p: l::AdminParams,
    mut r: l::AdminResults,
) -> H {
    // K78(c): administration is for the person herself. A connection whose actor is not the person (an AI seat acting on her
    // behalf) is refused, whatever `by` says.
    let actor = sh.actor.clone();
    if actor != sh.svc.person {
        return Err(Coded::new(LayoutCode::Unsupported)
            .with("what", "on-behalf")
            .into());
    }
    let Some(port) = sh.svc.admin.clone() else {
        return Err(Coded::new(LayoutCode::Unsupported)
            .with("what", "admin")
            .into());
    };
    // `by` is what the client CLAIMS. It is never the actor (the actor is the connection's identity, K78): it only goes to the
    // service's record of ignored claims, where the program that hosts the service writes it to its log.
    let by = txt(p.get()?.get_by())?;
    if let Some(note) = &sh.svc.claimed_by {
        note(&sh.caller, &by);
    }
    let imp = AdminImpl {
        port,
        listeners: sh.listeners.clone(),
        store: sh.clone().into_stamp(),
        actor,
        person: sh.svc.person.clone(),
    };
    r.get().set_admin(capnp_rpc::new_client(imp));
    Ok(())
}

/// What the admin capability needs to tell the watchers after a change: the stamp of the store.
pub(crate) trait StampSource {
    fn stamp(&self) -> fs_forms_layout::Pending<'_, fs_forms_layout::Stamp>;
}

impl<S: LayoutStore + ?Sized> StampSource for Shared<S> {
    fn stamp(&self) -> fs_forms_layout::Pending<'_, fs_forms_layout::Stamp> {
        self.svc.store.stamp()
    }
}

impl<S: LayoutStore + ?Sized + 'static> Shared<S> {
    fn into_stamp(self: Rc<Self>) -> Rc<dyn StampSource> {
        self
    }
}

struct AdminImpl {
    port: Rc<dyn LayoutAdmin>,
    listeners: Rc<Listeners>,
    store: Rc<dyn StampSource>,
    actor: Reference,
    person: Reference,
}

impl AdminImpl {
    fn who(&self) -> Who {
        Who {
            actor: self.actor.clone(),
            author: self.person.clone(),
        }
    }

    async fn changed(&self) {
        self.listeners.heard(&self.store.stamp().await);
    }
}

fn target(kind: &str, elem: &str) -> Result<Option<ElemRef>, Coded<LayoutCode>> {
    if kind.is_empty() {
        return Ok(None);
    }
    let kind = ElemKind::parse(kind).ok_or_else(|| bad(kind))?;
    Ok(Some(ElemRef {
        kind,
        elem: ElemId::parse(elem)?,
    }))
}

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
impl a::Server for AdminImpl {
    async fn lock(
        self: Rc<Self>,
        p: a::LockParams,
        mut r: a::LockResults,
    ) -> Result<(), capnp::Error> {
        let run = async {
            let p = p.get()?;
            let t = target(&txt(p.get_kind())?, &txt(p.get_elem())?)?;
            let changed = self
                .port
                .lock(
                    &layout_id(p.get_layout()?),
                    t.as_ref(),
                    &txt(p.get_reason())?,
                    &self.who(),
                )
                .await?;
            r.get().set_changed(changed);
            if changed {
                self.changed().await;
            }
            H::Ok(())
        };
        run.await.map_err(Into::into)
    }

    async fn unlock(
        self: Rc<Self>,
        p: a::UnlockParams,
        mut r: a::UnlockResults,
    ) -> Result<(), capnp::Error> {
        let run = async {
            let p = p.get()?;
            let t = target(&txt(p.get_kind())?, &txt(p.get_elem())?)?;
            let changed = self
                .port
                .unlock(
                    &layout_id(p.get_layout()?),
                    t.as_ref(),
                    &txt(p.get_reason())?,
                    &self.who(),
                )
                .await?;
            r.get().set_changed(changed);
            if changed {
                self.changed().await;
            }
            H::Ok(())
        };
        run.await.map_err(Into::into)
    }

    async fn create_instance_overlay(
        self: Rc<Self>,
        p: a::CreateInstanceOverlayParams,
        mut r: a::CreateInstanceOverlayResults,
    ) -> Result<(), capnp::Error> {
        let run = async {
            let p = p.get()?;
            let (lang, title) = (opt(p.get_lang())?, opt(p.get_title_key())?);
            let id = self
                .port
                .create_instance_overlay(
                    &layout_id(p.get_layout()?),
                    lang.as_deref(),
                    title.as_deref(),
                    &self.who(),
                )
                .await?;
            write_id(r.get().init_id(), &id.0)?;
            self.changed().await;
            H::Ok(())
        };
        run.await.map_err(Into::into)
    }
}
