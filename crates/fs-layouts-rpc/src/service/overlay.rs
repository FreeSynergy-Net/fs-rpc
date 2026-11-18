//! The overlay methods: `overlays`, `overlay`, `createOverlay`, `replaceOps`, `setOverlayState`, `deleteOverlay`,
//! `importOverlay`, `resolveConflict`. The editor sends the whole op list; who and time of an op are the service's and the
//! store's, never read from the wire (D13: the store fills `expect`).
//!
//! Pattern: Adapter (contract -> port `OverlayWrite`).

use fs_forms_layout::{ElemId, LayoutStore, Mode, Resolution, Rev};

use super::Shared;
use crate::fail::H;
use crate::layouts_capnp::layouts as l;
use crate::wire::answers::write_report;
use crate::wire::conflict::read_overlay_doc;
use crate::wire::ids::{layout_id, overlay_id, write_id};
use crate::wire::overlay::{read_kinds, write_overlay};
use crate::wire::text::{bad, n, opt, txt};

pub(crate) async fn overlays<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::OverlaysParams,
    mut r: l::OverlaysResults,
) -> H {
    let list = sh
        .svc
        .store
        .overlays(&layout_id(p.get()?.get_layout()?))
        .await?;
    let mut out = r.get().init_overlays(n(&list));
    for (i, o) in (0..).zip(&list) {
        write_overlay(out.reborrow().get(i), o, true)?;
    }
    Ok(())
}

pub(crate) async fn overlay<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::OverlayParams,
    mut r: l::OverlayResults,
) -> H {
    let found = sh
        .svc
        .store
        .overlay(&overlay_id(p.get()?.get_id()?))
        .await?;
    let mut out = r.get();
    out.set_found(found.is_some());
    if let Some(o) = found {
        write_overlay(out.init_overlay(), &o, true)?;
    }
    Ok(())
}

pub(crate) async fn create_overlay<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::CreateOverlayParams,
    mut r: l::CreateOverlayResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    let (lang, title) = (opt(p.get_lang())?, opt(p.get_title_key())?);
    let id = sh
        .svc
        .store
        .create_overlay(
            &layout_id(p.get_layout()?),
            lang.as_deref(),
            title.as_deref(),
            &who,
        )
        .await?;
    write_id(r.get().init_id(), &id.0)?;
    sh.changed().await;
    Ok(())
}

pub(crate) async fn replace_ops<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::ReplaceOpsParams,
    mut r: l::ReplaceOpsResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    let ops = read_kinds(p.get_ops()?)?;
    let report = sh
        .svc
        .store
        .replace_ops(
            &overlay_id(p.get_id()?),
            Rev(p.get_expected_rev()),
            ops,
            &who,
        )
        .await?;
    write_report(r.get().init_report(), &report)?;
    if report.problems.is_empty() {
        sh.changed().await;
    }
    Ok(())
}

pub(crate) async fn set_overlay_state<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::SetOverlayStateParams,
    mut r: l::SetOverlayStateResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    let rev = sh
        .svc
        .store
        .set_state(
            &overlay_id(p.get_id()?),
            Rev(p.get_expected_rev()),
            p.get_on(),
            &who,
        )
        .await?;
    r.get().set_rev(rev.0);
    sh.changed().await;
    Ok(())
}

pub(crate) async fn delete_overlay<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::DeleteOverlayParams,
    _: l::DeleteOverlayResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    sh.svc
        .store
        .delete_overlay(&overlay_id(p.get_id()?), Rev(p.get_expected_rev()), &who)
        .await?;
    sh.changed().await;
    Ok(())
}

pub(crate) async fn import_overlay<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::ImportOverlayParams,
    mut r: l::ImportOverlayResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    let doc = read_overlay_doc(p.get_doc()?)?;
    let mode = if p.get_dry_run() {
        Mode::DryRun
    } else {
        Mode::Apply
    };
    let report = sh.svc.store.import_overlay(doc, mode, &who).await?;
    write_report(r.get().init_report(), &report)?;
    if !p.get_dry_run() && report.problems.is_empty() {
        sh.changed().await;
    }
    Ok(())
}

pub(crate) async fn resolve_conflict<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::ResolveConflictParams,
    mut r: l::ResolveConflictResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    let how = match txt(p.get_how())?.as_str() {
        "keep" => Resolution::Keep,
        "drop" => Resolution::Drop,
        "retarget" => Resolution::Retarget(ElemId::parse(&txt(p.get_retarget())?)?),
        other => return Err(bad(other).into()),
    };
    let rev = sh
        .svc
        .store
        .resolve_conflict(
            &overlay_id(p.get_overlay()?),
            p.get_seq(),
            &txt(p.get_base_version())?,
            how,
            &who,
        )
        .await?;
    r.get().set_rev(rev.0);
    sh.changed().await;
    Ok(())
}
