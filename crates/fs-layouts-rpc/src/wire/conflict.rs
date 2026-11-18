//! Conflicts, element locks and the exchange shape `OverlayDoc` on the wire. An open conflict has no `resolution`; a closed one
//! says `resolved` (the model keeps only open or not).
//!
//! Pattern: Adapter (model <-> wire).

use fs_forms_layout::{
    Conflict, ElemId, ElemKind, ElemLock, ElemRef, Item, Op, OpKind, OverlayDoc, Reference, Who,
};

use super::ids::{overlay_id, read_time, write_id, write_time};
use super::overlay::{need_elem, read_kinds, read_value, write_ops, write_value};
use super::text::{R, bad, n, opt, txt, word};
use super::words::Words;
use crate::layouts_capnp as w;

pub(crate) fn write_conflict(mut e: w::conflict::Builder<'_>, c: &Conflict) -> R<()> {
    write_id(e.reborrow().init_overlay(), &c.overlay.0)?;
    e.set_seq(c.seq);
    e.set_base_version(&c.base_version);
    e.set_cause(c.cause.word());
    write_value(e.reborrow().init_found(), &c.found);
    write_time(e.reborrow().init_at(), c.at);
    e.set_resolution(if c.open { "" } else { "resolved" });
    Ok(())
}

pub(crate) fn read_conflict(e: w::conflict::Reader<'_>) -> R<Conflict> {
    Ok(Conflict {
        overlay: overlay_id(e.get_overlay().map_err(|_| bad("overlay"))?),
        seq: e.get_seq(),
        base_version: txt(e.get_base_version())?,
        cause: word(e.get_cause())?,
        found: read_value(e.get_found().map_err(|_| bad("found"))?)?,
        at: read_time(e.get_at().map_err(|_| bad("at"))?),
        open: txt(e.get_resolution())?.is_empty(),
    })
}

pub(crate) fn write_lock(mut e: w::elem_lock::Builder<'_>, l: &ElemLock) {
    if let Some(t) = &l.target {
        e.set_kind(t.kind.as_str());
        e.set_elem(t.elem.as_str());
    }
    e.set_reason(&l.reason);
    e.set_locked_by(&l.locked_by.0);
    write_time(e.init_locked_at(), l.locked_at);
}

pub(crate) fn read_lock(e: w::elem_lock::Reader<'_>) -> R<ElemLock> {
    let kind = txt(e.get_kind())?;
    let target = if kind.is_empty() {
        None
    } else {
        Some(ElemRef {
            kind: ElemKind::parse(&kind).ok_or_else(|| bad(&kind))?,
            elem: need_elem(e.get_elem())?,
        })
    };
    Ok(ElemLock {
        target,
        reason: txt(e.get_reason())?,
        locked_by: Reference(txt(e.get_locked_by())?),
        locked_at: read_time(e.get_locked_at().map_err(|_| bad("at"))?),
    })
}

/// The element id an op item is known by: the target, or the new element of an insert.
fn op_elem(k: &OpKind) -> ElemId {
    match k {
        OpKind::Set { target, .. } | OpKind::Remove { target } | OpKind::Move { target, .. } => {
            target.elem.clone()
        }
        OpKind::Insert { new_elem, .. } => new_elem.clone(),
    }
}

pub(crate) fn write_overlay_doc(
    mut e: w::overlay_doc::Builder<'_>,
    d: &OverlayDoc,
    who: &Who,
    at: fs_forms_layout::Utc,
) {
    e.set_format(d.format);
    e.set_package(&d.package);
    e.set_extends(&d.extends);
    e.set_frontend(&d.frontend);
    e.set_base_version(&d.base_version);
    e.set_lang(d.lang.as_deref().unwrap_or(""));
    e.set_title_key(d.title_key.as_deref().unwrap_or(""));
    let ops: Vec<Op> = (0..)
        .zip(&d.ops)
        .map(|(seq, i)| Op {
            seq,
            kind: i.v.clone(),
            who: who.clone(),
            at,
        })
        .collect();
    write_ops(e.init_ops(n(&ops)), &ops);
}

pub(crate) fn read_overlay_doc(e: w::overlay_doc::Reader<'_>) -> R<OverlayDoc> {
    let ops = read_kinds(e.get_ops().map_err(|_| bad("ops"))?)?
        .into_iter()
        .map(|v| Item {
            elem: Some(op_elem(&v)),
            line: 0,
            v,
        })
        .collect();
    Ok(OverlayDoc {
        format: e.get_format(),
        package: txt(e.get_package())?,
        extends: txt(e.get_extends())?,
        frontend: txt(e.get_frontend())?,
        base_version: txt(e.get_base_version())?,
        lang: opt(e.get_lang())?,
        title_key: opt(e.get_title_key())?,
        ops,
    })
}
