//! Views, orders and filter trees, both directions. A filter that is absent is `hasFilter = false` (never an empty node);
//! `value`, `low` and `high` carry a flag each (an empty text is a value); `searchKind` and `grouping` are `""` = not given.
//!
//! Pattern: Adapter (model <-> wire), recursive for the filter tree.

use fs_forms_layout::{FilterDoc, FilterOp, Item, OrderDoc, Reference, ViewDoc};

use super::doc_out::elem_text;
use super::text::{R, elem, n, opt, txt, word};
use super::words::Words;
use crate::layouts_capnp as w;

pub(crate) fn write_view(mut e: w::view::Builder<'_>, v: &Item<ViewDoc>) {
    e.set_elem(elem_text(v));
    e.set_view(&v.v.view);
    e.set_owner(v.v.owner.as_ref().map_or("", |r| r.0.as_str()));
    e.set_grouping(v.v.grouping.as_deref().unwrap_or(""));
    e.set_search_kind(v.v.search_kind.as_deref().unwrap_or(""));
    let mut l = e.reborrow().init_orders(n(&v.v.orders));
    for (i, o) in (0..).zip(&v.v.orders) {
        let mut x = l.reborrow().get(i);
        x.set_elem(elem_text(o));
        x.set_by(o.v.by.word());
        x.set_name(&o.v.name);
        x.set_descending(o.v.descending);
    }
    e.set_has_filter(v.v.filter.is_some());
    if let Some(f) = &v.v.filter {
        write_filter(e.init_filter(), f);
    }
}

fn write_filter(mut e: w::filter_node::Builder<'_>, f: &Item<FilterDoc>) {
    e.set_elem(elem_text(f));
    match &f.v {
        FilterDoc::Op { op, children } => {
            e.set_op(op.word());
            let mut l = e.init_children(n(children));
            for (i, c) in (0..).zip(children) {
                write_filter(l.reborrow().get(i), c);
            }
        }
        FilterDoc::Is {
            cond,
            path,
            value,
            low,
            high,
            values,
        } => {
            e.set_op(FilterOp::Is.word());
            e.set_cond(cond.word());
            e.set_path(path);
            e.set_has_value(value.is_some());
            e.set_value(value.as_deref().unwrap_or(""));
            e.set_has_low(low.is_some());
            e.set_low(low.as_deref().unwrap_or(""));
            e.set_has_high(high.is_some());
            e.set_high(high.as_deref().unwrap_or(""));
            let mut l = e.init_values(n(values));
            for (i, s) in (0..).zip(values) {
                let mut x = l.reborrow().get(i);
                x.set_elem(elem_text(s));
                x.set_scheme(&s.v);
            }
        }
    }
}

/// A text that is given only when its flag says so: an empty text with the flag is a real empty text.
fn given(flag: bool, t: capnp::Result<capnp::text::Reader<'_>>) -> R<Option<String>> {
    flag.then(|| txt(t)).transpose()
}

pub(crate) fn read_view(e: w::view::Reader<'_>) -> R<ViewDoc> {
    let orders = e
        .get_orders()
        .map_err(|_| super::text::bad("orders"))?
        .iter()
        .map(|o| {
            Ok(Item {
                elem: elem(o.get_elem())?,
                line: 0,
                v: OrderDoc {
                    by: word(o.get_by())?,
                    name: txt(o.get_name())?,
                    descending: o.get_descending(),
                },
            })
        })
        .collect::<R<Vec<_>>>()?;
    let filter = if e.get_has_filter() {
        let f = e.get_filter().map_err(|_| super::text::bad("filter"))?;
        Some(read_filter(f)?)
    } else {
        None
    };
    Ok(ViewDoc {
        view: txt(e.get_view())?,
        owner: opt(e.get_owner())?.map(Reference),
        grouping: opt(e.get_grouping())?,
        orders,
        filter,
        search_kind: opt(e.get_search_kind())?,
    })
}

fn read_filter(f: w::filter_node::Reader<'_>) -> R<Item<FilterDoc>> {
    let op: FilterOp = word(f.get_op())?;
    let v = if op == FilterOp::Is {
        let values = f
            .get_values()
            .map_err(|_| super::text::bad("values"))?
            .iter()
            .map(|s| {
                Ok(Item {
                    elem: elem(s.get_elem())?,
                    line: 0,
                    v: txt(s.get_scheme())?,
                })
            })
            .collect::<R<Vec<_>>>()?;
        FilterDoc::Is {
            cond: word(f.get_cond())?,
            path: txt(f.get_path())?,
            value: given(f.get_has_value(), f.get_value())?,
            low: given(f.get_has_low(), f.get_low())?,
            high: given(f.get_has_high(), f.get_high())?,
            values,
        }
    } else {
        let children = f
            .get_children()
            .map_err(|_| super::text::bad("children"))?
            .iter()
            .map(read_filter)
            .collect::<R<Vec<_>>>()?;
        FilterDoc::Op { op, children }
    };
    Ok(Item {
        elem: elem(f.get_elem())?,
        line: 0,
        v,
    })
}
