//! Overlays on the wire: ops, values, the overlay itself, conflicts, locks and the exchange shape `OverlayDoc`.
//! The service sets time and author/actor of an op when it stores it; here they are only carried.
//!
//! Pattern: Adapter (model <-> wire).

use fs_forms_layout::{
    ElemId, ElemKind, ElemRef, Op, OpKind, Overlay, OverlayScope, PackageRef, Reference, Rev,
    Value, Who,
};

use super::ids::{layout_id, overlay_id, read_time, write_id, write_time};
use super::text::{R, bad, elem, n, opt, txt, word};
use super::words::Words;
use crate::layouts_capnp as w;

pub(crate) fn write_value(mut out: w::value::Builder<'_>, v: &Value) {
    match v {
        Value::None => out.set_none(()),
        Value::Text(t) => out.set_text(t),
        Value::Int(i) => out.set_int(*i),
        Value::Bool(b) => out.set_bool(*b),
    }
}

pub(crate) fn read_value(r: w::value::Reader<'_>) -> R<Value> {
    Ok(match r.which().map_err(|_| bad("value"))? {
        w::value::Which::None(()) => Value::None,
        w::value::Which::Text(t) => Value::Text(txt(t)?),
        w::value::Which::Int(i) => Value::Int(i),
        w::value::Which::Bool(b) => Value::Bool(b),
    })
}

pub(crate) fn write_op(mut e: w::op::Builder<'_>, o: &Op) {
    e.set_seq(o.seq);
    e.set_author(&o.who.author.0);
    e.set_actor(&o.who.actor.0);
    write_time(e.reborrow().init_at(), o.at);
    match &o.kind {
        OpKind::Set {
            target,
            field,
            value,
            expect,
        } => {
            e.set_op("set");
            e.set_target_kind(target.kind.as_str());
            e.set_target_elem(target.elem.as_str());
            e.set_field(field.word());
            write_value(e.reborrow().init_value(), value);
            write_value(e.init_expect(), expect);
        }
        OpKind::Remove { target } => {
            e.set_op("remove");
            e.set_target_kind(target.kind.as_str());
            e.set_target_elem(target.elem.as_str());
        }
        OpKind::Insert {
            kind,
            new_elem,
            anchor,
            position,
        } => {
            e.set_op("insert");
            e.set_target_kind(kind.as_str());
            e.set_new_elem(new_elem.as_str());
            e.set_anchor_elem(anchor.as_str());
            e.set_position(position.word());
        }
        OpKind::Move {
            target,
            anchor,
            position,
        } => {
            e.set_op("move");
            e.set_target_kind(target.kind.as_str());
            e.set_target_elem(target.elem.as_str());
            e.set_anchor_elem(anchor.as_str());
            e.set_position(position.word());
        }
    }
}

pub(crate) fn need_elem(t: capnp::Result<capnp::text::Reader<'_>>) -> R<ElemId> {
    elem(t)?.ok_or_else(|| bad("elem"))
}

fn target(e: &w::op::Reader<'_>) -> R<ElemRef> {
    let kind = txt(e.get_target_kind())?;
    Ok(ElemRef {
        kind: ElemKind::parse(&kind).ok_or_else(|| bad(&kind))?,
        elem: need_elem(e.get_target_elem())?,
    })
}

pub(crate) fn read_kind(e: &w::op::Reader<'_>) -> R<OpKind> {
    let op = txt(e.get_op())?;
    Ok(match op.as_str() {
        "set" => OpKind::Set {
            target: target(e)?,
            field: word(e.get_field())?,
            value: read_value(e.get_value().map_err(|_| bad("value"))?)?,
            expect: read_value(e.get_expect().map_err(|_| bad("expect"))?)?,
        },
        "remove" => OpKind::Remove { target: target(e)? },
        "insert" => {
            let kind = txt(e.get_target_kind())?;
            OpKind::Insert {
                kind: ElemKind::parse(&kind).ok_or_else(|| bad(&kind))?,
                new_elem: need_elem(e.get_new_elem())?,
                anchor: need_elem(e.get_anchor_elem())?,
                position: word(e.get_position())?,
            }
        }
        "move" => OpKind::Move {
            target: target(e)?,
            anchor: need_elem(e.get_anchor_elem())?,
            position: word(e.get_position())?,
        },
        other => return Err(bad(other)),
    })
}

pub(crate) fn read_op(e: w::op::Reader<'_>) -> R<Op> {
    Ok(Op {
        seq: e.get_seq(),
        kind: read_kind(&e)?,
        who: Who {
            actor: Reference(txt(e.get_actor())?),
            author: Reference(txt(e.get_author())?),
        },
        at: read_time(e.get_at().map_err(|_| bad("at"))?),
    })
}

pub(crate) fn write_ops(out: capnp::struct_list::Builder<'_, w::op::Owned>, ops: &[Op]) {
    let mut out = out;
    for (i, o) in (0..).zip(ops) {
        write_op(out.reborrow().get(i), o);
    }
}

pub(crate) fn read_ops(l: capnp::struct_list::Reader<'_, w::op::Owned>) -> R<Vec<Op>> {
    l.iter().map(read_op).collect()
}

/// The kinds of one op list without who and time (what the editor sends): the service stamps who and time itself.
pub(crate) fn read_kinds(l: capnp::struct_list::Reader<'_, w::op::Owned>) -> R<Vec<OpKind>> {
    l.iter().map(|e| read_kind(&e)).collect()
}

pub(crate) fn write_overlay(mut e: w::overlay::Builder<'_>, o: &Overlay, with_ops: bool) -> R<()> {
    write_id(e.reborrow().init_id(), &o.id.0)?;
    write_id(e.reborrow().init_layout(), &o.layout.0)?;
    match &o.scope {
        OverlayScope::Instance => e.set_scope("instance"),
        OverlayScope::Person(r) => {
            e.set_scope("person");
            e.set_owner(&r.0);
        }
    }
    e.set_title_key(o.title_key.as_deref().unwrap_or(""));
    e.set_lang(o.lang.as_deref().unwrap_or(""));
    e.set_author(&o.who.author.0);
    e.set_actor(&o.who.actor.0);
    e.set_base_package(&o.base.package);
    e.set_base_version(&o.base.version);
    e.set_state(o.state.word());
    e.set_rev(o.rev.0);
    if with_ops {
        write_ops(e.init_ops(n(&o.ops)), &o.ops);
    }
    Ok(())
}

pub(crate) fn read_overlay(e: w::overlay::Reader<'_>) -> R<Overlay> {
    let scope = txt(e.get_scope())?;
    let scope = match scope.as_str() {
        "instance" => OverlayScope::Instance,
        "person" => OverlayScope::Person(Reference(txt(e.get_owner())?)),
        other => return Err(bad(other)),
    };
    Ok(Overlay {
        id: overlay_id(e.get_id().map_err(|_| bad("id"))?),
        layout: layout_id(e.get_layout().map_err(|_| bad("layout"))?),
        scope,
        title_key: opt(e.get_title_key())?,
        lang: opt(e.get_lang())?,
        who: Who {
            actor: Reference(txt(e.get_actor())?),
            author: Reference(txt(e.get_author())?),
        },
        base: PackageRef {
            package: txt(e.get_base_package())?,
            version: txt(e.get_base_version())?,
        },
        state: word(e.get_state())?,
        rev: Rev(e.get_rev()),
        ops: read_ops(e.get_ops().map_err(|_| bad("ops"))?)?,
    })
}
