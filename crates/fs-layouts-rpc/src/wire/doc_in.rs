//! The description from the wire: layer 1 in. Names stay unchecked (the store checks them against the catalog); `""` is "not
//! given"; the line is 0 (= wire). A word that is in no closed list is `format-invalid{found=<word>}`, never replaced.
//!
//! Pattern: Adapter (wire -> model), one small function per struct.

use fs_forms_layout::{
    ActionDoc, ActionTarget, BindFrom, BindingDoc, BlockDoc, FrontendKey, Item, LayoutDoc, MenuDoc,
    MessageDoc, PageDoc, Reference, RequiresDoc, SourceDoc,
};

use super::filter::read_view;
use super::text::{R, bad, elem, opt, txt, word};
use super::words::menu_from_words;
use crate::layouts_capnp as w;

/// The range as written: the text when given, else built from the fields (a reader that is no Rust program sends fields only).
fn read_range(r: w::range::Reader<'_>) -> R<String> {
    if let Some(t) = opt(r.get_text())? {
        return Ok(t);
    }
    let mut out = txt(r.get_capability())?;
    if (r.get_min_major(), r.get_min_minor()) != (0, 0) {
        out = format!("{out} >={}.{}", r.get_min_major(), r.get_min_minor());
    }
    if (r.get_below_major(), r.get_below_minor()) != (0, 0) {
        out = format!("{out} <{}.{}", r.get_below_major(), r.get_below_minor());
    }
    Ok(out)
}

/// A wire read that failed: `format-invalid{found=<what>}`.
fn lost(
    what: &'static str,
) -> impl Fn(capnp::Error) -> fs_forms_layout::Coded<fs_forms_layout::LayoutCode> {
    move |_| bad(what)
}

fn item<T>(e: Option<fs_forms_layout::ElemId>, v: T) -> Item<T> {
    Item {
        elem: e,
        line: 0,
        v,
    }
}

pub(crate) fn read_doc(d: w::layout_doc::Reader<'_>) -> R<LayoutDoc> {
    let requires = d
        .get_requires()
        .map_err(lost("requires"))?
        .iter()
        .map(|r| {
            Ok(item(
                elem(r.get_elem())?,
                RequiresDoc {
                    range: read_range(r.get_range().map_err(lost("range"))?)?,
                },
            ))
        })
        .collect::<R<Vec<_>>>()?;
    let sources = d
        .get_sources()
        .map_err(lost("sources"))?
        .iter()
        .map(|s| {
            Ok(item(
                elem(s.get_elem())?,
                SourceDoc {
                    range: read_range(s.get_range().map_err(lost("range"))?)?,
                    data_type: txt(s.get_data_type())?,
                    scope: opt(s.get_scope())?.map(|t| word_of(&t)).transpose()?,
                },
            ))
        })
        .collect::<R<Vec<_>>>()?;
    let menu = d
        .get_menu()
        .map_err(lost("menu"))?
        .iter()
        .map(read_menu)
        .collect::<R<Vec<_>>>()?;
    let pages = d
        .get_pages()
        .map_err(lost("pages"))?
        .iter()
        .map(read_page)
        .collect::<R<Vec<_>>>()?;
    let actions = d
        .get_actions()
        .map_err(lost("actions"))?
        .iter()
        .map(read_action)
        .collect::<R<Vec<_>>>()?;
    let views = d
        .get_views()
        .map_err(lost("views"))?
        .iter()
        .map(|v| Ok(item(elem(v.get_elem())?, read_view(v)?)))
        .collect::<R<Vec<_>>>()?;
    let messages = d
        .get_messages()
        .map_err(lost("messages"))?
        .iter()
        .map(|m| {
            Ok(item(
                elem(m.get_elem())?,
                MessageDoc {
                    key: txt(m.get_key())?,
                    lang: txt(m.get_lang())?,
                    message: txt(m.get_message())?,
                },
            ))
        })
        .collect::<R<Vec<_>>>()?;
    Ok(LayoutDoc {
        format: d.get_format(),
        key: FrontendKey {
            extends: txt(d.get_extends())?,
            frontend: txt(d.get_frontend())?,
        },
        exec: txt(d.get_exec())?,
        title_key: txt(d.get_title_key())?,
        style: opt(d.get_style())?,
        requires,
        sources,
        menu,
        pages,
        actions,
        views,
        messages,
    })
}

fn word_of<W: super::words::Words>(t: &str) -> R<W> {
    W::from_word(t).ok_or_else(|| bad(t))
}

fn read_menu(m: w::menu_node::Reader<'_>) -> R<Item<MenuDoc>> {
    let (kind, standard) = (txt(m.get_kind())?, txt(m.get_standard())?);
    let command = opt(m.get_command())?.map(|c| (m.get_source(), c));
    let children = m
        .get_children()
        .map_err(|_| bad("children"))?
        .iter()
        .map(read_menu)
        .collect::<R<Vec<_>>>()?;
    Ok(item(
        elem(m.get_elem())?,
        MenuDoc {
            kind: menu_from_words(&kind, &standard).ok_or_else(|| bad(&kind))?,
            label_key: opt(m.get_label_key())?,
            command,
            on: opt(m.get_on_what())?.map(|t| word_of(&t)).transpose()?,
            shortcut: opt(m.get_shortcut())?,
            view: opt(m.get_view())?,
            children,
        },
    ))
}

fn read_page(p: w::page::Reader<'_>) -> R<Item<PageDoc>> {
    let blocks = p
        .get_blocks()
        .map_err(|_| bad("blocks"))?
        .iter()
        .map(|b| {
            let bindings = b
                .get_binds()
                .map_err(|_| bad("binds"))?
                .iter()
                .map(read_binding)
                .collect::<R<Vec<_>>>()?;
            Ok(item(
                elem(b.get_elem())?,
                BlockDoc {
                    place: txt(b.get_place())?,
                    block: txt(b.get_block())?,
                    source: (b.get_source() != 0).then(|| b.get_source()),
                    title_key: opt(b.get_title_key())?,
                    text_key: opt(b.get_text_key())?,
                    bindings,
                },
            ))
        })
        .collect::<R<Vec<_>>>()?;
    Ok(item(
        elem(p.get_elem())?,
        PageDoc {
            page: txt(p.get_page())?,
            frame: txt(p.get_frame())?,
            navigation: word(p.get_navigation())?,
            blocks,
        },
    ))
}

fn read_binding(b: w::binding::Reader<'_>) -> R<Item<BindingDoc>> {
    let name = || txt(b.get_name());
    let from = txt(b.get_from())?;
    let from = match from.as_str() {
        "records" => BindFrom::Records,
        "field" => BindFrom::Field {
            path: txt(b.get_path())?,
            role: txt(b.get_role())?,
        },
        "grouping" => BindFrom::Grouping(name()?),
        "order" => BindFrom::Order(name()?),
        "command" => BindFrom::Command(name()?),
        "lock" => BindFrom::Lock,
        "condition" => BindFrom::Condition,
        other => return Err(bad(other)),
    };
    let schemes = if b.get_has_schemes() {
        Some(
            b.get_schemes()
                .map_err(|_| bad("schemes"))?
                .iter()
                .map(|s| Ok(item(elem(s.get_elem())?, txt(s.get_scheme())?)))
                .collect::<R<Vec<_>>>()?,
        )
    } else {
        None
    };
    Ok(item(
        elem(b.get_elem())?,
        BindingDoc {
            slot: txt(b.get_slot())?,
            nth: b.get_nth(),
            from,
            schemes,
        },
    ))
}

fn read_action(a: w::action::Reader<'_>) -> R<Item<ActionDoc>> {
    let target = txt(a.get_target())?;
    let target = match target.as_str() {
        "command" => ActionTarget::Command {
            source: a.get_source(),
            command: txt(a.get_command())?,
        },
        "workflow" => ActionTarget::Workflow(Reference(txt(a.get_workflow())?)),
        other => return Err(bad(other)),
    };
    Ok(item(
        elem(a.get_elem())?,
        ActionDoc {
            trigger: word(a.get_trigger())?,
            target,
            confirm: opt(a.get_confirm())?.map(|t| word_of(&t)).transpose()?,
        },
    ))
}
