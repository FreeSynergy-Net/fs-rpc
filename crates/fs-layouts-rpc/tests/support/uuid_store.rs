//! TEST ONLY: the memory store of fs-forms-layout names its rows `layout-000001`, `overlay-000002`, `log-000003`; the contract
//! carries `Common.Uuid`, and so does every real store. This wrapper maps the ids of ANY port to UUID text on the way out and back
//! on the way in, so the service and the client are tested with a store that behaves like a real one.
#![allow(dead_code)]

use fs_forms_layout::*;

use super::uuid_ids::*;

/// The id mapping; the second field makes `stamp` fail (a store that cannot say its stamp).
pub struct UuidStore<S>(pub S, pub std::sync::Arc<std::sync::atomic::AtomicBool>);

impl<S: LayoutRead> LayoutRead for UuidStore<S> {
    fn stamp(&self) -> Pending<'_, Stamp> {
        if self.1.load(std::sync::atomic::Ordering::SeqCst) {
            return Box::pin(std::future::ready(Err(
                Coded::new(LayoutCode::StoreBusy).with("file", "test")
            )));
        }
        self.0.stamp()
    }
    fn health(&self) -> Pending<'_, Health> {
        self.0.health()
    }
    fn list<'a>(
        &'a self,
        f: &'a LayoutFilter,
        after: Option<&'a LayoutId>,
        size: u32,
    ) -> Pending<'a, Page<Summary>> {
        let after = after.map(lid);
        Box::pin(async move {
            let p = self.0.list(f, after.as_ref(), size).await?;
            Ok(Page {
                items: p.items.into_iter().map(fsummary).collect(),
                next: p.next.as_ref().map(flid),
            })
        })
    }
    fn original<'a>(&'a self, id: &'a LayoutId) -> Pending<'a, Option<(Layout, Summary)>> {
        let id = lid(id);
        Box::pin(async move { Ok(self.0.original(&id).await?.map(|(l, s)| (l, fsummary(s)))) })
    }
    fn resolve<'a>(&'a self, key: &'a FrontendKey) -> Pending<'a, Option<Resolved>> {
        Box::pin(async move {
            Ok(self.0.resolve(key).await?.map(|mut r| {
                r.id = flid(&r.id);
                r.conflicts = r.conflicts.into_iter().map(fconflict).collect();
                for l in &mut r.layers {
                    l.overlay = foid(&l.overlay);
                }
                for (_, l) in &mut r.provenance {
                    *l = match l.clone() {
                        Layer::Instance(o) => Layer::Instance(foid(&o)),
                        Layer::Person(o) => Layer::Person(foid(&o)),
                        Layer::Original => Layer::Original,
                    };
                }
                r
            }))
        })
    }
    fn overlays<'a>(&'a self, layout: &'a LayoutId) -> Pending<'a, Vec<Overlay>> {
        let l = lid(layout);
        Box::pin(async move {
            Ok(self
                .0
                .overlays(&l)
                .await?
                .into_iter()
                .map(foverlay)
                .collect())
        })
    }
    fn overlay<'a>(&'a self, id: &'a OverlayId) -> Pending<'a, Option<Overlay>> {
        let id = oid(id);
        Box::pin(async move { Ok(self.0.overlay(&id).await?.map(foverlay)) })
    }
    fn conflicts<'a>(
        &'a self,
        layout: Option<&'a LayoutId>,
        open_only: bool,
    ) -> Pending<'a, Vec<Conflict>> {
        let l = layout.map(lid);
        Box::pin(async move {
            Ok(self
                .0
                .conflicts(l.as_ref(), open_only)
                .await?
                .into_iter()
                .map(fconflict)
                .collect())
        })
    }
    fn log<'a>(&'a self, after: Option<&'a ObjectId>, size: u32) -> Pending<'a, Vec<LogEntry>> {
        let after = after.map(|a| ObjectId(back(&a.0)));
        Box::pin(async move {
            let mut v = self.0.log(after.as_ref(), size).await?;
            for e in &mut v {
                e.id = ObjectId(fwd(&e.id.0));
                e.layout = e.layout.as_ref().map(flid);
                e.overlay = e.overlay.as_ref().map(foid);
            }
            Ok(v)
        })
    }
}

impl<S: LayoutWrite> LayoutWrite for UuidStore<S> {
    fn create<'a>(&'a self, doc: LayoutDoc, who: &'a Who) -> Pending<'a, Report> {
        Box::pin(async move { Ok(freport(self.0.create(doc, who).await?)) })
    }
    fn import<'a>(&'a self, doc: LayoutDoc, mode: Mode, who: &'a Who) -> Pending<'a, Report> {
        Box::pin(async move { Ok(freport(self.0.import(doc, mode, who).await?)) })
    }
    fn save<'a>(
        &'a self,
        id: &'a LayoutId,
        expected: Rev,
        doc: LayoutDoc,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        let id = lid(id);
        Box::pin(async move { Ok(freport(self.0.save(&id, expected, doc, who).await?)) })
    }
    fn delete<'a>(&'a self, id: &'a LayoutId, expected: Rev, who: &'a Who) -> Pending<'a, ()> {
        let id = lid(id);
        Box::pin(async move { self.0.delete(&id, expected, who).await })
    }
}

impl<S: PackageLayouts> PackageLayouts for UuidStore<S> {
    fn install<'a>(
        &'a self,
        doc: LayoutDoc,
        package: &'a PackageRef,
        mode: Mode,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        Box::pin(async move { Ok(freport(self.0.install(doc, package, mode, who).await?)) })
    }
    fn uninstall<'a>(&'a self, package: &'a str, who: &'a Who) -> Pending<'a, Vec<Report>> {
        Box::pin(async move {
            Ok(self
                .0
                .uninstall(package, who)
                .await?
                .into_iter()
                .map(freport)
                .collect())
        })
    }
}

impl<S: OverlayWrite> OverlayWrite for UuidStore<S> {
    fn create_overlay<'a>(
        &'a self,
        layout: &'a LayoutId,
        lang: Option<&'a str>,
        title: Option<&'a str>,
        who: &'a Who,
    ) -> Pending<'a, OverlayId> {
        let l = lid(layout);
        Box::pin(async move { Ok(foid(&self.0.create_overlay(&l, lang, title, who).await?)) })
    }
    fn replace_ops<'a>(
        &'a self,
        id: &'a OverlayId,
        expected: Rev,
        ops: Vec<OpKind>,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        let id = oid(id);
        Box::pin(async move { Ok(freport(self.0.replace_ops(&id, expected, ops, who).await?)) })
    }
    fn set_state<'a>(
        &'a self,
        id: &'a OverlayId,
        expected: Rev,
        on: bool,
        who: &'a Who,
    ) -> Pending<'a, Rev> {
        let id = oid(id);
        Box::pin(async move { self.0.set_state(&id, expected, on, who).await })
    }
    fn delete_overlay<'a>(
        &'a self,
        id: &'a OverlayId,
        expected: Rev,
        who: &'a Who,
    ) -> Pending<'a, ()> {
        let id = oid(id);
        Box::pin(async move { self.0.delete_overlay(&id, expected, who).await })
    }
    fn import_overlay<'a>(
        &'a self,
        doc: OverlayDoc,
        mode: Mode,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        Box::pin(async move { Ok(freport(self.0.import_overlay(doc, mode, who).await?)) })
    }
    fn resolve_conflict<'a>(
        &'a self,
        id: &'a OverlayId,
        seq: u32,
        base: &'a str,
        how: Resolution,
        who: &'a Who,
    ) -> Pending<'a, Rev> {
        let id = oid(id);
        Box::pin(async move { self.0.resolve_conflict(&id, seq, base, how, who).await })
    }
}

impl<S: LayoutAdmin> LayoutAdmin for UuidStore<S> {
    fn lock<'a>(
        &'a self,
        layout: &'a LayoutId,
        elem: Option<&'a ElemRef>,
        reason: &'a str,
        who: &'a Who,
    ) -> Pending<'a, bool> {
        let l = lid(layout);
        Box::pin(async move { self.0.lock(&l, elem, reason, who).await })
    }
    fn unlock<'a>(
        &'a self,
        layout: &'a LayoutId,
        elem: Option<&'a ElemRef>,
        reason: &'a str,
        who: &'a Who,
    ) -> Pending<'a, bool> {
        let l = lid(layout);
        Box::pin(async move { self.0.unlock(&l, elem, reason, who).await })
    }
    fn create_instance_overlay<'a>(
        &'a self,
        layout: &'a LayoutId,
        lang: Option<&'a str>,
        title: Option<&'a str>,
        who: &'a Who,
    ) -> Pending<'a, OverlayId> {
        let l = lid(layout);
        Box::pin(async move {
            Ok(foid(
                &self.0.create_instance_overlay(&l, lang, title, who).await?,
            ))
        })
    }
}
