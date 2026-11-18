//! The answers of the contract: summary, page, health, report, log entry. Absent is `""` (text), the all-zero id (an id) or a
//! flag next to the number (`hasRev`, `hasCount`, ...): 0 is a real counter, so a counter never stands for "absent".
//!
//! Pattern: Adapter (model <-> wire).

use fs_forms_layout::{
    ElemId, FrontendKey, Health, LogEntry, PackageRef, Reference, Report, Rev, Summary, Who,
};

use super::conflict::{read_conflict, write_conflict};
use super::ids::{layout_id, read_cursor, read_id, read_time, write_id, write_nil, write_time};
use super::text::{R, bad, n, opt, read_problems, txt, word, write_problems};
use super::words::Words;
use crate::layouts_capnp as w;

pub(crate) fn write_summary(mut e: w::summary::Builder<'_>, s: &Summary) -> R<()> {
    write_id(e.reborrow().init_id(), &s.id.0)?;
    e.set_extends(&s.key.extends);
    e.set_frontend(&s.key.frontend);
    e.set_origin(s.origin.word());
    if let Some(p) = &s.package {
        e.set_package(&p.package);
        e.set_package_version(&p.version);
    }
    e.set_owner(s.owner.as_ref().map_or("", |r| r.0.as_str()));
    e.set_rev(s.rev.0);
    write_time(e.reborrow().init_updated(), s.updated);
    if let Some(d) = s.deleted {
        write_time(e.reborrow().init_deleted(), d);
    }
    e.set_is_deleted(s.deleted.is_some());
    e.set_overlays(s.overlays);
    e.set_open_conflicts(s.open_conflicts);
    Ok(())
}

pub(crate) fn read_summary(e: w::summary::Reader<'_>) -> R<Summary> {
    let package = opt(e.get_package())?.map(|package| -> R<PackageRef> {
        Ok(PackageRef {
            package,
            version: txt(e.get_package_version())?,
        })
    });
    let deleted = if e.get_is_deleted() {
        Some(read_time(e.get_deleted().map_err(|_| bad("deleted"))?))
    } else {
        None
    };
    Ok(Summary {
        id: layout_id(e.get_id().map_err(|_| bad("id"))?),
        key: FrontendKey {
            extends: txt(e.get_extends())?,
            frontend: txt(e.get_frontend())?,
        },
        origin: word(e.get_origin())?,
        package: package.transpose()?,
        owner: opt(e.get_owner())?.map(Reference),
        rev: Rev(e.get_rev()),
        updated: read_time(e.get_updated().map_err(|_| bad("updated"))?),
        deleted,
        overlays: e.get_overlays(),
        open_conflicts: e.get_open_conflicts(),
    })
}

#[allow(clippy::match_wildcard_for_single_variants)] // `Health` is non-exhaustive
pub(crate) fn write_health(mut out: w::health::Builder<'_>, h: &Health) {
    match h {
        Health::Defaults(list) => write_problems(out.init_defaults(n(list)), list),
        _ => out.set_ok(()),
    }
}

pub(crate) fn read_health(r: w::health::Reader<'_>) -> R<Health> {
    Ok(match r.which().map_err(|_| bad("health"))? {
        w::health::Which::Ok(()) => Health::Ok,
        w::health::Which::Defaults(l) => {
            Health::Defaults(read_problems(l.map_err(|_| bad("defaults"))?)?)
        }
    })
}

pub(crate) fn write_report(mut out: w::report::Builder<'_>, r: &Report) -> R<()> {
    out.set_has_layout(r.layout.is_some());
    if let Some(l) = &r.layout {
        write_id(out.reborrow().init_layout(), &l.0)?;
    }
    out.set_has_rev(r.rev.is_some());
    out.set_rev(r.rev.map_or(0, |v| v.0));
    write_problems(out.reborrow().init_problems(n(&r.problems)), &r.problems);
    write_problems(out.reborrow().init_warnings(n(&r.warnings)), &r.warnings);
    let mut l = out.init_conflicts(n(&r.conflicts));
    for (i, c) in (0..).zip(&r.conflicts) {
        write_conflict(l.reborrow().get(i), c)?;
    }
    Ok(())
}

pub(crate) fn read_report(r: w::report::Reader<'_>) -> R<Report> {
    Ok(Report {
        layout: r
            .get_has_layout()
            .then(|| r.get_layout().map(layout_id))
            .transpose()
            .map_err(|_| bad("layout"))?,
        rev: r.get_has_rev().then(|| Rev(r.get_rev())),
        problems: read_problems(r.get_problems().map_err(|_| bad("problems"))?)?,
        warnings: read_problems(r.get_warnings().map_err(|_| bad("warnings"))?)?,
        conflicts: r
            .get_conflicts()
            .map_err(|_| bad("conflicts"))?
            .iter()
            .map(read_conflict)
            .collect::<R<_>>()?,
    })
}

pub(crate) fn write_log(mut e: w::log_entry::Builder<'_>, l: &LogEntry) -> R<()> {
    write_id(e.reborrow().init_id(), &l.id)?;
    write_time(e.reborrow().init_at(), l.at);
    e.set_by(&l.who.actor.0);
    e.set_author(&l.who.author.0);
    e.set_os_user(l.os_user.as_deref().unwrap_or(""));
    e.set_action(l.action.word());
    match &l.layout {
        Some(x) => write_id(e.reborrow().init_layout(), &x.0)?,
        None => write_nil(e.reborrow().init_layout()),
    }
    match &l.overlay {
        Some(x) => write_id(e.reborrow().init_overlay(), &x.0)?,
        None => write_nil(e.reborrow().init_overlay()),
    }
    e.set_elem(l.elem.as_ref().map_or("", ElemId::as_str));
    e.set_has_rev_before(l.rev_before.is_some());
    e.set_rev_before(l.rev_before.map_or(0, |v| v.0));
    e.set_has_rev_after(l.rev_after.is_some());
    e.set_rev_after(l.rev_after.map_or(0, |v| v.0));
    if let Some(p) = &l.package {
        e.set_package(&p.package);
        e.set_version(&p.version);
    }
    e.set_has_count(l.count.is_some());
    e.set_count(l.count.unwrap_or(0));
    e.set_reason(l.reason.as_deref().unwrap_or(""));
    Ok(())
}

pub(crate) fn read_log(e: w::log_entry::Reader<'_>) -> R<LogEntry> {
    let package = opt(e.get_package())?
        .map(|package| -> R<PackageRef> {
            Ok(PackageRef {
                package,
                version: txt(e.get_version())?,
            })
        })
        .transpose()?;
    Ok(LogEntry {
        id: read_id(e.get_id().map_err(|_| bad("id"))?),
        at: read_time(e.get_at().map_err(|_| bad("at"))?),
        who: Who {
            actor: Reference(txt(e.get_by())?),
            author: Reference(txt(e.get_author())?),
        },
        os_user: opt(e.get_os_user())?,
        action: word(e.get_action())?,
        layout: read_cursor(e.get_layout().map_err(|_| bad("layout"))?)
            .map(fs_forms_layout::LayoutId),
        overlay: read_cursor(e.get_overlay().map_err(|_| bad("overlay"))?)
            .map(fs_forms_layout::OverlayId),
        elem: super::text::elem(e.get_elem())?,
        rev_before: e.get_has_rev_before().then(|| Rev(e.get_rev_before())),
        rev_after: e.get_has_rev_after().then(|| Rev(e.get_rev_after())),
        package,
        count: e.get_has_count().then(|| e.get_count()),
        reason: opt(e.get_reason())?,
    })
}
