//! The description to the wire (`LayoutDoc` struct): layer 1 out. `elem` None is `""`, the line is not sent (0 = wire).
//!
//! Pattern: Adapter (model -> wire), one small function per struct.

use fs_forms_layout::{
    ActionDoc, ActionTarget, BindFrom, ElemId, Item, LayoutDoc, MenuDoc, PageDoc, SourceDoc,
};

use super::text::n;
use super::words::{Words, from_word, menu_words};
use crate::layouts_capnp as w;

pub(crate) fn elem_text<T>(i: &Item<T>) -> &str {
    i.elem.as_ref().map_or("", ElemId::as_str)
}

/// Writes a range text as the wire's `Range`: the text always, the fields when it parses.
pub(crate) fn write_range(mut out: w::range::Builder<'_>, text: &str) {
    out.set_text(text);
    let mut it = text.split_whitespace();
    out.set_capability(it.next().unwrap_or(""));
    for t in it {
        let v = |s: &str| {
            let (a, b) = s.split_once('.')?;
            Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))
        };
        if let Some((a, b)) = t.strip_prefix(">=").and_then(v) {
            out.set_min_major(a);
            out.set_min_minor(b);
        } else if let Some((a, b)) = t.strip_prefix('<').and_then(v) {
            out.set_below_major(a);
            out.set_below_minor(b);
        }
    }
}

pub(crate) fn write_doc(mut out: w::layout_doc::Builder<'_>, d: &LayoutDoc) {
    out.set_format(d.format);
    out.set_extends(&d.key.extends);
    out.set_frontend(&d.key.frontend);
    out.set_exec(&d.exec);
    out.set_title_key(&d.title_key);
    out.set_style(d.style.as_deref().unwrap_or(""));
    let mut l = out.reborrow().init_requires(n(&d.requires));
    for (i, r) in (0..).zip(&d.requires) {
        let mut e = l.reborrow().get(i);
        e.set_elem(elem_text(r));
        write_range(e.init_range(), &r.v.range);
    }
    let mut l = out.reborrow().init_sources(n(&d.sources));
    for (i, s) in (0..).zip(&d.sources) {
        write_source(l.reborrow().get(i), i + 1, s);
    }
    let mut l = out.reborrow().init_menu(n(&d.menu));
    for (i, m) in (0..).zip(&d.menu) {
        write_menu(l.reborrow().get(i), m);
    }
    let mut l = out.reborrow().init_pages(n(&d.pages));
    for (i, p) in (0..).zip(&d.pages) {
        write_page(l.reborrow().get(i), p);
    }
    let mut l = out.reborrow().init_actions(n(&d.actions));
    for (i, a) in (0..).zip(&d.actions) {
        write_action(l.reborrow().get(i), i + 1, a);
    }
    let mut l = out.reborrow().init_views(n(&d.views));
    for (i, v) in (0..).zip(&d.views) {
        super::filter::write_view(l.reborrow().get(i), v);
    }
    let mut l = out.init_messages(n(&d.messages));
    for (i, m) in (0..).zip(&d.messages) {
        let mut e = l.reborrow().get(i);
        e.set_elem(elem_text(m));
        e.set_key(&m.v.key);
        e.set_lang(&m.v.lang);
        e.set_message(&m.v.message);
    }
}

fn write_source(mut e: w::source::Builder<'_>, nth: u32, s: &Item<SourceDoc>) {
    e.set_elem(elem_text(s));
    e.set_n(nth);
    write_range(e.reborrow().init_range(), &s.v.range);
    e.set_data_type(&s.v.data_type);
    e.set_scope(s.v.scope.map_or("", Words::word));
}

fn write_menu(mut e: w::menu_node::Builder<'_>, m: &Item<MenuDoc>) {
    let (kind, standard) = menu_words(m.v.kind);
    e.set_elem(elem_text(m));
    e.set_kind(kind);
    e.set_standard(standard);
    e.set_label_key(m.v.label_key.as_deref().unwrap_or(""));
    if let Some((source, command)) = &m.v.command {
        e.set_source(*source);
        e.set_command(command);
    }
    e.set_on_what(m.v.on.map_or("", Words::word));
    e.set_shortcut(m.v.shortcut.as_deref().unwrap_or(""));
    e.set_view(m.v.view.as_deref().unwrap_or(""));
    let mut l = e.init_children(n(&m.v.children));
    for (i, c) in (0..).zip(&m.v.children) {
        write_menu(l.reborrow().get(i), c);
    }
}

fn write_page(mut e: w::page::Builder<'_>, p: &Item<PageDoc>) {
    e.set_elem(elem_text(p));
    e.set_page(&p.v.page);
    e.set_frame(&p.v.frame);
    e.set_navigation(p.v.navigation.word());
    let mut l = e.init_blocks(n(&p.v.blocks));
    for (i, b) in (0..).zip(&p.v.blocks) {
        let mut x = l.reborrow().get(i);
        x.set_elem(elem_text(b));
        x.set_place(&b.v.place);
        x.set_block(&b.v.block);
        x.set_source(b.v.source.unwrap_or(0));
        x.set_title_key(b.v.title_key.as_deref().unwrap_or(""));
        x.set_text_key(b.v.text_key.as_deref().unwrap_or(""));
        let mut bl = x.init_binds(n(&b.v.bindings));
        for (j, bi) in (0..).zip(&b.v.bindings) {
            write_binding(bl.reborrow().get(j), bi);
        }
    }
}

fn write_binding(mut x: w::binding::Builder<'_>, b: &Item<fs_forms_layout::BindingDoc>) {
    x.set_elem(elem_text(b));
    x.set_slot(&b.v.slot);
    x.set_nth(b.v.nth);
    x.set_from(from_word(&b.v.from));
    match &b.v.from {
        BindFrom::Field { path, role } => {
            x.set_path(path);
            x.set_role(role);
        }
        BindFrom::Grouping(name) | BindFrom::Order(name) | BindFrom::Command(name) => {
            x.set_name(name);
        }
        _ => {}
    }
    x.set_has_schemes(b.v.schemes.is_some());
    let list = b.v.schemes.as_deref().unwrap_or(&[]);
    let mut l = x.init_schemes(n(list));
    for (i, s) in (0..).zip(list) {
        let mut e = l.reborrow().get(i);
        e.set_elem(elem_text(s));
        e.set_scheme(&s.v);
    }
}

fn write_action(mut e: w::action::Builder<'_>, nth: u32, a: &Item<ActionDoc>) {
    e.set_elem(elem_text(a));
    e.set_n(nth);
    e.set_trigger(a.v.trigger.word());
    match &a.v.target {
        ActionTarget::Command { source, command } => {
            e.set_target("command");
            e.set_source(*source);
            e.set_command(command);
        }
        ActionTarget::Workflow(r) => {
            e.set_target("workflow");
            e.set_workflow(&r.0);
        }
    }
    e.set_confirm(a.v.confirm.map_or("", Words::word));
}
