//! `LayoutAdmin` over the contract (`admin(by)`, then `lock`/`unlock`/`createInstanceOverlay`). `by` is only a claim (`Who.actor`
//! as the caller says it); the service ignores it and records it, the actor is the connection's. The person service answers `unsupported{what=admin}`. One round trip more per call (the capability is asked for
//! each time): the administrator's commands are rare, and a held capability would have to outlive a lost link.
//!
//! Pattern: Adapter (port -> contract), Proxy.

use std::rc::Rc;

use fs_forms_layout::{ElemRef, LayoutAdmin, LayoutId, OverlayId, Pending, Who};

use super::ctx::Ctx;
use crate::wire::ids::{read_id, write_id};

/// The administrator's port.
pub struct RpcLayoutAdmin {
    ctx: Rc<Ctx>,
}

impl RpcLayoutAdmin {
    pub(crate) fn new(ctx: Rc<Ctx>) -> Self {
        Self { ctx }
    }
}

macro_rules! locking {
    ($c:ident, $req:ident, $layout:ident, $elem:ident, $reason:ident) => {{
        write_id($req.get().init_layout(), &$layout.0)?;
        if let Some(e) = $elem {
            $req.get().set_kind(e.kind.as_str());
            $req.get().set_elem(e.elem.as_str());
        }
        $req.get().set_reason($reason);
    }};
}

impl LayoutAdmin for RpcLayoutAdmin {
    fn lock<'a>(
        &'a self,
        layout: &'a LayoutId,
        elem: Option<&'a ElemRef>,
        reason: &'a str,
        who: &'a Who,
    ) -> Pending<'a, bool> {
        Box::pin(self.ctx.call(async move |c| {
            let mut a = c.admin_request();
            a.get().set_by(&who.actor.0);
            let admin = a.send().promise.await?.get()?.get_admin()?;
            let mut req = admin.lock_request();
            locking!(c, req, layout, elem, reason);
            Ok(req.send().promise.await?.get()?.get_changed())
        }))
    }

    fn unlock<'a>(
        &'a self,
        layout: &'a LayoutId,
        elem: Option<&'a ElemRef>,
        reason: &'a str,
        who: &'a Who,
    ) -> Pending<'a, bool> {
        Box::pin(self.ctx.call(async move |c| {
            let mut a = c.admin_request();
            a.get().set_by(&who.actor.0);
            let admin = a.send().promise.await?.get()?.get_admin()?;
            let mut req = admin.unlock_request();
            locking!(c, req, layout, elem, reason);
            Ok(req.send().promise.await?.get()?.get_changed())
        }))
    }

    fn create_instance_overlay<'a>(
        &'a self,
        layout: &'a LayoutId,
        lang: Option<&'a str>,
        title_key: Option<&'a str>,
        who: &'a Who,
    ) -> Pending<'a, OverlayId> {
        Box::pin(self.ctx.call(async move |c| {
            let mut a = c.admin_request();
            a.get().set_by(&who.actor.0);
            let admin = a.send().promise.await?.get()?.get_admin()?;
            let mut req = admin.create_instance_overlay_request();
            write_id(req.get().init_layout(), &layout.0)?;
            req.get().set_lang(lang.unwrap_or(""));
            req.get().set_title_key(title_key.unwrap_or(""));
            Ok(OverlayId(read_id(
                req.send().promise.await?.get()?.get_id()?,
            )))
        }))
    }
}
