//! The writing methods for the person's own descriptions and the package originals: `create`, `import`, `save`, `delete`,
//! `install`, `uninstall`. Every write that went through (not a dry run, not a refusal) tells the watchers the new stamp.
//! A refusal is in the report (`problems`), never an RPC error: the caller reads what was refused and why.
//!
//! Pattern: Adapter (contract -> ports `LayoutWrite` and `PackageLayouts`).

use fs_forms_layout::{LayoutStore, Mode, PackageRef, Rev};

use super::Shared;
use crate::fail::H;
use crate::layouts_capnp::layouts as l;
use crate::wire::answers::write_report;
use crate::wire::doc_in::read_doc;
use crate::wire::ids::layout_id;
use crate::wire::text::{n, txt};

fn mode(dry: bool) -> Mode {
    if dry { Mode::DryRun } else { Mode::Apply }
}

pub(crate) async fn create<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::CreateParams,
    mut r: l::CreateResults,
) -> H {
    let p = p.get()?;
    let (doc, who) = (read_doc(p.get_doc()?)?, sh.who(&txt(p.get_author())?)?);
    let report = sh.svc.store.create(doc, &who).await?;
    write_report(r.get().init_report(), &report)?;
    if report.problems.is_empty() {
        sh.changed().await;
    }
    Ok(())
}

pub(crate) async fn import<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::ImportParams,
    mut r: l::ImportResults,
) -> H {
    let p = p.get()?;
    let (doc, who) = (read_doc(p.get_doc()?)?, sh.who(&txt(p.get_author())?)?);
    let report = sh
        .svc
        .store
        .import(doc, mode(p.get_dry_run()), &who)
        .await?;
    write_report(r.get().init_report(), &report)?;
    if !p.get_dry_run() && report.problems.is_empty() {
        sh.changed().await;
    }
    Ok(())
}

pub(crate) async fn save<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::SaveParams,
    mut r: l::SaveResults,
) -> H {
    let p = p.get()?;
    let (doc, who) = (read_doc(p.get_doc()?)?, sh.who(&txt(p.get_author())?)?);
    let report = sh
        .svc
        .store
        .save(
            &layout_id(p.get_id()?),
            Rev(p.get_expected_rev()),
            doc,
            &who,
        )
        .await?;
    write_report(r.get().init_report(), &report)?;
    if report.problems.is_empty() {
        sh.changed().await;
    }
    Ok(())
}

pub(crate) async fn delete<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::DeleteParams,
    _: l::DeleteResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    sh.svc
        .store
        .delete(&layout_id(p.get_id()?), Rev(p.get_expected_rev()), &who)
        .await?;
    sh.changed().await;
    Ok(())
}

pub(crate) async fn install<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::InstallParams,
    mut r: l::InstallResults,
) -> H {
    let p = p.get()?;
    let (doc, who) = (read_doc(p.get_doc()?)?, sh.who(&txt(p.get_author())?)?);
    let package = PackageRef {
        package: txt(p.get_package())?,
        version: txt(p.get_version())?,
    };
    let report = sh
        .svc
        .store
        .install(doc, &package, mode(p.get_dry_run()), &who)
        .await?;
    write_report(r.get().init_report(), &report)?;
    if !p.get_dry_run() && report.problems.is_empty() {
        sh.changed().await;
    }
    Ok(())
}

pub(crate) async fn uninstall<S: LayoutStore + ?Sized>(
    sh: &Shared<S>,
    p: l::UninstallParams,
    mut r: l::UninstallResults,
) -> H {
    let p = p.get()?;
    let who = sh.who(&txt(p.get_author())?)?;
    let reports = sh.svc.store.uninstall(&txt(p.get_package())?, &who).await?;
    let mut out = r.get().init_reports(n(&reports));
    for (i, x) in (0..).zip(&reports) {
        write_report(out.reborrow().get(i), x)?;
    }
    sh.changed().await;
    Ok(())
}
