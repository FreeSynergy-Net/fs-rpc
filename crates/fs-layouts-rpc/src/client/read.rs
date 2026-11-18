//! `LayoutRead` over the contract. A Problem of the service is its code, 1:1 (`from_problem`).
//!
//! Pattern: Adapter (port -> contract).

use fs_forms_layout::{
    Conflict, FrontendKey, Health, Layout, LayoutFilter, LayoutId, LayoutRead, LogEntry, ObjectId,
    Overlay, OverlayId, Page, Pending, Resolved, Stamp, Summary,
};

use super::RpcLayouts;
use crate::wire::answers::{read_health, read_log, read_summary};
use crate::wire::conflict::read_conflict;
use crate::wire::ids::{read_id, write_id, write_nil};
use crate::wire::overlay::read_overlay;
use crate::wire::resolved::{read_layout, read_resolved};
use crate::wire::words::Words;

impl LayoutRead for RpcLayouts {
    fn stamp(&self) -> Pending<'_, Stamp> {
        Box::pin(self.ctx.call(async move |c| {
            let r = c.stamp_request().send().promise.await?;
            Ok(Stamp(r.get()?.get_stamp()))
        }))
    }

    fn health(&self) -> Pending<'_, Health> {
        Box::pin(self.ctx.call(async move |c| {
            let r = c.health_request().send().promise.await?;
            Ok(read_health(r.get()?.get_health()?)?)
        }))
    }

    fn list<'a>(
        &'a self,
        filter: &'a LayoutFilter,
        after: Option<&'a LayoutId>,
        size: u32,
    ) -> Pending<'a, Page<Summary>> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.list_request();
            {
                let mut p = req.get();
                p.set_extends(filter.extends.as_deref().unwrap_or(""));
                p.set_origin(filter.origin.map_or("", Words::word));
                p.set_package(filter.package.as_deref().unwrap_or(""));
                p.set_with_deleted(filter.with_deleted);
                p.set_size(size);
                match after {
                    Some(a) => write_id(p.init_after(), &a.0)?,
                    None => write_nil(p.init_after()),
                }
            }
            let r = req.send().promise.await?;
            let r = r.get()?;
            let items = r
                .get_items()?
                .iter()
                .map(read_summary)
                .collect::<Result<Vec<_>, _>>()?;
            let next = r
                .get_has_next()
                .then(|| r.get_next().map(|n| LayoutId(read_id(n))))
                .transpose()?;
            Ok(Page { items, next })
        }))
    }

    fn original<'a>(&'a self, id: &'a LayoutId) -> Pending<'a, Option<(Layout, Summary)>> {
        let check = self.check.clone();
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.original_request();
            write_id(req.get().init_id(), &id.0)?;
            let r = req.send().promise.await?;
            let r = r.get()?;
            if !r.get_found() {
                return Ok(None);
            }
            Ok(Some((
                read_layout(r.get_layout()?, check.as_ref())?,
                read_summary(r.get_summary()?)?,
            )))
        }))
    }

    fn resolve<'a>(&'a self, key: &'a FrontendKey) -> Pending<'a, Option<Resolved>> {
        let check = self.check.clone();
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.resolve_request();
            req.get().set_extends(&key.extends);
            req.get().set_frontend(&key.frontend);
            let r = req.send().promise.await?;
            let r = r.get()?;
            if !r.get_found() {
                return Ok(None);
            }
            Ok(Some(read_resolved(r.get_resolved()?, check.as_ref())?))
        }))
    }

    fn overlays<'a>(&'a self, layout: &'a LayoutId) -> Pending<'a, Vec<Overlay>> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.overlays_request();
            write_id(req.get().init_layout(), &layout.0)?;
            let r = req.send().promise.await?;
            Ok(r.get()?
                .get_overlays()?
                .iter()
                .map(read_overlay)
                .collect::<Result<Vec<_>, _>>()?)
        }))
    }

    fn overlay<'a>(&'a self, id: &'a OverlayId) -> Pending<'a, Option<Overlay>> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.overlay_request();
            write_id(req.get().init_id(), &id.0)?;
            let r = req.send().promise.await?;
            let r = r.get()?;
            Ok(if r.get_found() {
                Some(read_overlay(r.get_overlay()?)?)
            } else {
                None
            })
        }))
    }

    fn conflicts<'a>(
        &'a self,
        layout: Option<&'a LayoutId>,
        open_only: bool,
    ) -> Pending<'a, Vec<Conflict>> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.conflicts_request();
            {
                let mut p = req.get();
                p.set_all(layout.is_none());
                p.set_open_only(open_only);
                match layout {
                    Some(l) => write_id(p.init_layout(), &l.0)?,
                    None => write_nil(p.init_layout()),
                }
            }
            let r = req.send().promise.await?;
            Ok(r.get()?
                .get_conflicts()?
                .iter()
                .map(read_conflict)
                .collect::<Result<Vec<_>, _>>()?)
        }))
    }

    fn log<'a>(&'a self, after: Option<&'a ObjectId>, size: u32) -> Pending<'a, Vec<LogEntry>> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.log_request();
            {
                let mut p = req.get();
                p.set_size(size);
                match after {
                    Some(a) => write_id(p.init_after(), a)?,
                    None => write_nil(p.init_after()),
                }
            }
            let r = req.send().promise.await?;
            Ok(r.get()?
                .get_entries()?
                .iter()
                .map(read_log)
                .collect::<Result<Vec<_>, _>>()?)
        }))
    }
}
