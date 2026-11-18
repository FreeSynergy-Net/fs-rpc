//! The reading methods: `stamp`, `health`, `list`, `original`, `resolve`, `conflicts`, `log`. Absent or not visible is
//! `found = false` (no existence leak, ENTWURF 3.1); a page larger than the service takes is `page-size{size,max,by}`.
//!
//! Pattern: Adapter (contract -> port `LayoutRead`).

use fs_forms_layout::{FrontendKey, LayoutFilter, LayoutId, LayoutStore};

use super::Shared;
use crate::fail::H;
use crate::layouts_capnp::layouts as l;
use crate::wire::answers::{write_health, write_log, write_summary};
use crate::wire::conflict::write_conflict;
use crate::wire::doc_out::write_doc;
use crate::wire::ids::{layout_id, read_cursor, write_id, write_nil};
use crate::wire::resolved::write_resolved;
use crate::wire::text::{n, opt, txt};

pub(crate) async fn stamp<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    _: l::StampParams,
    mut r: l::StampResults,
) -> H {
    r.get().set_stamp(sh.svc.store.stamp().await?.0);
    Ok(())
}

pub(crate) async fn health<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    _: l::HealthParams,
    mut r: l::HealthResults,
) -> H {
    write_health(r.get().init_health(), &sh.svc.store.health().await?);
    Ok(())
}

pub(crate) async fn list<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::ListParams,
    mut r: l::ListResults,
) -> H {
    let p = p.get()?;
    sh.svc.check_page(p.get_size())?;
    let filter = LayoutFilter {
        extends: opt(p.get_extends())?,
        origin: opt(p.get_origin())?.map(|o| word_origin(&o)).transpose()?,
        package: opt(p.get_package())?,
        with_deleted: p.get_with_deleted(),
    };
    let after = read_cursor(p.get_after()?).map(LayoutId);
    let page = sh
        .svc
        .store
        .list(&filter, after.as_ref(), p.get_size())
        .await?;
    let mut out = r.get();
    let mut items = out.reborrow().init_items(n(&page.items));
    for (i, s) in (0..).zip(&page.items) {
        write_summary(items.reborrow().get(i), s)?;
    }
    out.set_has_next(page.next.is_some());
    match &page.next {
        Some(x) => write_id(out.init_next(), &x.0)?,
        None => write_nil(out.init_next()),
    }
    Ok(())
}

fn word_origin(
    w: &str,
) -> Result<fs_forms_layout::Origin, fs_forms_layout::Coded<fs_forms_layout::LayoutCode>> {
    use crate::wire::words::Words;
    fs_forms_layout::Origin::from_word(w).ok_or_else(|| crate::wire::text::bad(w))
}

pub(crate) async fn original<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::OriginalParams,
    mut r: l::OriginalResults,
) -> H {
    let id = layout_id(p.get()?.get_id()?);
    let found = sh.svc.store.original(&id).await?;
    let mut out = r.get();
    out.set_found(found.is_some());
    if let Some((layout, summary)) = found {
        write_doc(out.reborrow().init_layout(), &layout.to_doc());
        write_summary(out.init_summary(), &summary)?;
    }
    Ok(())
}

pub(crate) async fn resolve<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::ResolveParams,
    mut r: l::ResolveResults,
) -> H {
    let p = p.get()?;
    let key = FrontendKey {
        extends: txt(p.get_extends())?,
        frontend: txt(p.get_frontend())?,
    };
    let found = sh.svc.store.resolve(&key).await?;
    let mut out = r.get();
    out.set_found(found.is_some());
    if let Some(x) = found {
        // the real summary (origin, package, owner, counters) is the store's; `Resolved` itself does not carry it
        let summary = sh.svc.store.original(&x.id).await?.map(|(_, s)| s);
        write_resolved(out.init_resolved(), &x, summary.as_ref())?;
    }
    Ok(())
}

pub(crate) async fn conflicts<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::ConflictsParams,
    mut r: l::ConflictsResults,
) -> H {
    let p = p.get()?;
    let layout = if p.get_all() {
        None
    } else {
        Some(layout_id(p.get_layout()?))
    };
    let list = sh
        .svc
        .store
        .conflicts(layout.as_ref(), p.get_open_only())
        .await?;
    let mut out = r.get().init_conflicts(n(&list));
    for (i, c) in (0..).zip(&list) {
        write_conflict(out.reborrow().get(i), c)?;
    }
    Ok(())
}

pub(crate) async fn log<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::LogParams,
    mut r: l::LogResults,
) -> H {
    let p = p.get()?;
    sh.svc.check_page(p.get_size())?;
    let after = read_cursor(p.get_after()?);
    let list = sh.svc.store.log(after.as_ref(), p.get_size()).await?;
    let mut out = r.get().init_entries(n(&list));
    for (i, e) in (0..).zip(&list) {
        write_log(out.reborrow().get(i), e)?;
    }
    Ok(())
}
