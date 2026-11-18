//! Builders of descriptions for the tests: a small one and a rich one that touches every family of the wire.
#![allow(dead_code)]

use fs_forms_layout::*;

pub fn elem(s: &str) -> ElemId {
    ElemId::parse(s).expect("valid elem")
}
pub fn item<T>(e: &str, v: T) -> Item<T> {
    Item {
        elem: Some(elem(e)),
        line: 0,
        v,
    }
}
pub fn pkg(version: &str) -> PackageRef {
    PackageRef {
        package: "net.example.pkg".to_owned(),
        version: version.to_owned(),
    }
}
pub fn key(frontend: &str) -> FrontendKey {
    FrontendKey {
        extends: "net.example.Tasks".into(),
        frontend: frontend.into(),
    }
}

/// One page `board`, one block `main`, one binding `title`, one message; format 2 with every elem.
pub fn doc(frontend: &str) -> LayoutDoc {
    let binding = BindingDoc {
        slot: "title".into(),
        nth: 0,
        from: BindFrom::Field {
            path: "title".into(),
            role: "display".into(),
        },
        schemes: None,
    };
    let block = BlockDoc {
        place: "main".into(),
        block: "board".into(),
        source: None,
        title_key: None,
        text_key: None,
        bindings: vec![item("binding:board/main/title/0", binding)],
    };
    let page = PageDoc {
        page: "board".into(),
        frame: "workbench".into(),
        navigation: Navigation::Single,
        blocks: vec![item("block:board/main", block)],
    };
    LayoutDoc {
        format: 2,
        key: key(frontend),
        exec: "fs-forms-run".into(),
        title_key: "board-title".into(),
        style: None,
        requires: vec![],
        sources: vec![],
        menu: vec![],
        pages: vec![item("page:board", page)],
        actions: vec![],
        views: vec![],
        messages: vec![item(
            "message:board-title/de",
            MessageDoc {
                key: "board-title".into(),
                lang: "de".into(),
                message: "Aufgaben".into(),
            },
        )],
    }
}

/// Every family: requires, source, menu with a command and children, a block with source and keys, a binding with schemes, an
/// action on a workflow, a view with orders and a filter tree.
pub fn rich(frontend: &str) -> LayoutDoc {
    let mut d = doc(frontend);
    d.style = Some("calm".into());
    d.requires = vec![item(
        "requires:tasks",
        RequiresDoc {
            range: "tasks >=0.2 <0.3".into(),
        },
    )];
    d.sources = vec![item(
        "source:1",
        SourceDoc {
            range: "tasks >=0.2 <0.3".into(),
            data_type: "task".into(),
            scope: Some(SourceScope::SelectedList),
        },
    )];
    let leaf = MenuDoc {
        kind: MenuKind::Command,
        label_key: Some("new".into()),
        command: Some((1, "create".into())),
        on: Some(OnWhat::Always),
        shortcut: Some("ctrl+n".into()),
        view: None,
        children: vec![],
    };
    let std_program = MenuDoc {
        kind: MenuKind::Standard(MenuStandard::Program),
        label_key: None,
        command: None,
        on: None,
        shortcut: None,
        view: None,
        children: vec![],
    };
    let std_help = MenuDoc {
        kind: MenuKind::Standard(MenuStandard::Help),
        label_key: None,
        command: None,
        on: None,
        shortcut: None,
        view: None,
        children: vec![],
    };
    d.menu = vec![
        item(
            "menu:file",
            MenuDoc {
                kind: MenuKind::Menu,
                label_key: Some("file".into()),
                command: None,
                on: None,
                shortcut: None,
                view: None,
                children: vec![item("menu:file/new", leaf)],
            },
        ),
        item("menu:help", std_help),
        item("menu:program", std_program),
    ];
    let page = &mut d.pages[0].v;
    page.navigation = Navigation::ListDetail;
    let block = &mut page.blocks[0].v;
    block.source = Some(1);
    block.title_key = Some("t".into());
    block.text_key = Some("x".into());
    block.bindings[0].v.schemes = Some(vec![
        item("scheme:a", "https".to_owned()),
        item("scheme:b", "mailto".to_owned()),
    ]);
    block.bindings.push(item(
        "binding:board/main/group/0",
        BindingDoc {
            slot: "group".into(),
            nth: 0,
            from: BindFrom::Grouping("by-state".into()),
            schemes: None,
        },
    ));
    block.bindings.push(item(
        "binding:board/main/lock/0",
        BindingDoc {
            slot: "lock".into(),
            nth: 1,
            from: BindFrom::Lock,
            schemes: None,
        },
    ));
    // the store keeps bindings in the order of their element ids
    block.bindings.sort_by(|a, b| a.elem.cmp(&b.elem));
    d.actions = vec![item(
        "action:go",
        ActionDoc {
            trigger: Trigger::QuickAdd,
            target: ActionTarget::Workflow(Reference("https://example.org/wf/1".into())),
            confirm: Some(Confirm::Choice),
        },
    )];
    let filter = FilterDoc::Op {
        op: FilterOp::And,
        children: vec![
            item(
                "filter:a",
                FilterDoc::Is {
                    cond: FilterCond::Equals,
                    path: "state".into(),
                    value: Some("open".into()),
                    low: None,
                    high: None,
                    values: vec![],
                },
            ),
            item(
                "filter:c",
                FilterDoc::Is {
                    cond: FilterCond::Equals,
                    path: "note".into(),
                    value: Some(String::new()),
                    low: Some(String::new()),
                    high: None,
                    values: vec![],
                },
            ),
            item(
                "filter:b",
                FilterDoc::Is {
                    cond: FilterCond::AnyOf,
                    path: "tag".into(),
                    value: None,
                    low: None,
                    high: None,
                    values: vec![item("fv:1", "x".to_owned())],
                },
            ),
        ],
    };
    d.views = vec![item(
        "view:open",
        ViewDoc {
            view: "open".into(),
            owner: Some(Reference("https://example.org/people/test".into())),
            grouping: Some("by-state".into()),
            orders: vec![item(
                "order:1",
                OrderDoc {
                    by: OrderBy::Field,
                    name: "title".into(),
                    descending: true,
                },
            )],
            filter: Some(item("filter:root", filter)),
            search_kind: Some("prefix".into()),
        },
    )];
    d
}

pub fn set_text(field: Field, to: &str) -> OpKind {
    OpKind::Set {
        target: ElemRef {
            kind: ElemKind::Block,
            elem: elem("block:board/main"),
        },
        field,
        value: Value::Text(to.into()),
        expect: Value::Text("wrong".into()),
    }
}
