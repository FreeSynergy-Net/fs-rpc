//! `Resolved` (layer 3) on the wire. The wire carries more than the model (a full `Summary`); the model has no place for it, so
//! the service writes what it knows (id, key, rev; `origin` stays empty) and a reader takes only what the model has. The layers travel as
//! overlays without ops; provenance as (kind, elem, layer, overlay).
//!
//! Pattern: Adapter (model <-> wire).

use fs_forms_layout::{
    ElemKind, ElemRef, FrontendKey, Layer, LayerInfo, Layout, OverlayScope, Reference, Resolved,
    Rev, Stamp, Summary,
};

use super::answers::write_summary;
use super::conflict::{read_conflict, read_lock, write_conflict, write_lock};
use super::doc_in::read_doc;
use super::doc_out::write_doc;
use super::ids::{layout_id, read_cursor, write_id, write_nil};
use super::text::{R, bad, n, read_problems, txt, word, write_problems};
use super::words::Words;
use crate::layouts_capnp as w;

pub(crate) fn write_resolved(
    mut out: w::resolved::Builder<'_>,
    resolved: &Resolved,
    summary: Option<&Summary>,
) -> R<()> {
    match summary {
        Some(s) => write_summary(out.reborrow().init_summary(), s)?,
        None => write_summary_part(out.reborrow().init_summary(), resolved)?,
    }
    write_doc(out.reborrow().init_layout(), &resolved.layout.to_doc());
    out.set_stamp(resolved.stamp.0);
    write_layers(out.reborrow().init_layers(n(&resolved.layers)), resolved)?;
    let mut conflicts = out.reborrow().init_conflicts(n(&resolved.conflicts));
    for (i, conflict) in (0..).zip(&resolved.conflicts) {
        write_conflict(conflicts.reborrow().get(i), conflict)?;
    }
    let mut locks = out.reborrow().init_locks(n(&resolved.locks));
    for (i, lock) in (0..).zip(&resolved.locks) {
        write_lock(locks.reborrow().get(i), lock);
    }
    write_provenance(
        out.reborrow().init_provenance(n(&resolved.provenance)),
        &resolved.provenance,
    )?;
    out.reborrow()
        .set_effective_digest(&resolved.effective_digest[..]);
    write_problems(
        out.reborrow().init_problems(n(&resolved.problems)),
        &resolved.problems,
    );
    write_problems(out.init_warnings(n(&resolved.warnings)), &resolved.warnings);
    Ok(())
}

/// The summary carries what the model knows (id, key, rev); origin and package come from `list` and `original`.
fn write_summary_part(mut sum: w::summary::Builder<'_>, resolved: &Resolved) -> R<()> {
    write_id(sum.reborrow().init_id(), &resolved.id.0)?;
    sum.set_extends(&resolved.key.extends);
    sum.set_frontend(&resolved.key.frontend);
    // `origin` stays empty (= not given): the model's `Resolved` does not know it, and a made-up value would be a lie.
    sum.set_rev(resolved.rev.0);
    Ok(())
}

fn write_layers(
    mut list: capnp::struct_list::Builder<'_, w::overlay::Owned>,
    resolved: &Resolved,
) -> R<()> {
    for (i, layer) in (0..).zip(&resolved.layers) {
        let mut entry = list.reborrow().get(i);
        write_id(entry.reborrow().init_id(), &layer.overlay.0)?;
        write_id(entry.reborrow().init_layout(), &resolved.id.0)?;
        match &layer.scope {
            OverlayScope::Instance => entry.set_scope("instance"),
            OverlayScope::Person(person) => {
                entry.set_scope("person");
                entry.set_owner(&person.0);
            }
        }
        entry.set_state(layer.state.word());
        entry.set_rev(layer.rev.0);
    }
    Ok(())
}

fn write_provenance(
    mut list: capnp::struct_list::Builder<'_, w::provenance::Owned>,
    provenance: &[(ElemRef, Layer)],
) -> R<()> {
    for (i, (target, layer)) in (0..).zip(provenance) {
        let mut entry = list.reborrow().get(i);
        entry.set_kind(target.kind.as_str());
        entry.set_elem(target.elem.as_str());
        match layer {
            Layer::Original => {
                entry.set_layer("original");
                write_nil(entry.init_overlay());
            }
            Layer::Instance(o) => {
                entry.set_layer("instance");
                write_id(entry.init_overlay(), &o.0)?;
            }
            Layer::Person(o) => {
                entry.set_layer("person");
                write_id(entry.init_overlay(), &o.0)?;
            }
        }
    }
    Ok(())
}

/// Builds a checked `Layout` from the description the service sent. The client is no judge of limits (the service decided them,
/// rule 57), so the check is the caller's choice: see `RpcLayouts::with_check`.
pub(crate) type Check<'a> = &'a dyn Fn(fs_forms_layout::LayoutDoc) -> R<Layout>;

pub(crate) fn read_layout(r: w::layout_doc::Reader<'_>, check: Check<'_>) -> R<Layout> {
    check(read_doc(r)?)
}

pub(crate) fn read_resolved(r: w::resolved::Reader<'_>, check: Check<'_>) -> R<Resolved> {
    let s = r.get_summary().map_err(|_| bad("summary"))?;
    let id = layout_id(s.get_id().map_err(|_| bad("id"))?);
    let layers = r
        .get_layers()
        .map_err(|_| bad("layers"))?
        .iter()
        .map(|e| {
            let scope = txt(e.get_scope())?;
            let scope = match scope.as_str() {
                "instance" => OverlayScope::Instance,
                "person" => OverlayScope::Person(Reference(txt(e.get_owner())?)),
                other => return Err(bad(other)),
            };
            Ok(LayerInfo {
                overlay: super::ids::overlay_id(e.get_id().map_err(|_| bad("id"))?),
                scope,
                state: word(e.get_state())?,
                rev: Rev(e.get_rev()),
            })
        })
        .collect::<R<Vec<_>>>()?;
    let provenance = r
        .get_provenance()
        .map_err(|_| bad("provenance"))?
        .iter()
        .map(|e| {
            let kind = txt(e.get_kind())?;
            let t = ElemRef {
                kind: ElemKind::parse(&kind).ok_or_else(|| bad(&kind))?,
                elem: super::text::elem(e.get_elem())?.ok_or_else(|| bad("elem"))?,
            };
            let o = read_cursor(e.get_overlay().map_err(|_| bad("overlay"))?);
            let layer = match (txt(e.get_layer())?.as_str(), o) {
                ("original", _) => Layer::Original,
                ("instance", Some(o)) => Layer::Instance(fs_forms_layout::OverlayId(o)),
                ("person", Some(o)) => Layer::Person(fs_forms_layout::OverlayId(o)),
                (other, _) => return Err(bad(other)),
            };
            Ok((t, layer))
        })
        .collect::<R<Vec<_>>>()?;
    let digest: [u8; 32] = r
        .get_effective_digest()
        .map_err(|_| bad("digest"))?
        .try_into()
        .map_err(|_| bad("digest"))?;
    Ok(Resolved {
        id,
        key: FrontendKey {
            extends: txt(s.get_extends())?,
            frontend: txt(s.get_frontend())?,
        },
        rev: Rev(s.get_rev()),
        stamp: Stamp(r.get_stamp()),
        layout: read_layout(r.get_layout().map_err(|_| bad("layout"))?, check)?,
        layers,
        conflicts: r
            .get_conflicts()
            .map_err(|_| bad("conflicts"))?
            .iter()
            .map(read_conflict)
            .collect::<R<_>>()?,
        locks: r
            .get_locks()
            .map_err(|_| bad("locks"))?
            .iter()
            .map(read_lock)
            .collect::<R<_>>()?,
        provenance,
        effective_digest: digest,
        problems: read_problems(r.get_problems().map_err(|_| bad("problems"))?)?,
        warnings: read_problems(r.get_warnings().map_err(|_| bad("warnings"))?)?,
    })
}
