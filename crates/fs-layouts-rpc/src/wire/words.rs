//! The closed words of the wire: one table per enum, read and written from the same place (the same words as the exchange file of
//! fs-forms-exchange, whose table is private; this is the one copy of the adapter and a test walks every variant).
//!
//! Pattern: Value Object with a 1:1 text form.

use fs_forms_layout::{
    BindFrom, Cause, Confirm, Field, FilterCond, FilterOp, LogAction, MenuKind, MenuStandard,
    Navigation, OnWhat, OrderBy, Origin, OverlayState, Position, SourceScope, Trigger,
};

/// A closed word list: every variant has exactly one word.
pub(crate) trait Words: Sized + Copy {
    fn word(self) -> &'static str;
    fn from_word(w: &str) -> Option<Self>;
}

macro_rules! words {
    ($t:ty { $($v:ident => $w:literal),* $(,)? }) => {
        impl Words for $t {
            fn word(self) -> &'static str { match self { $(<$t>::$v => $w),* } }
            fn from_word(w: &str) -> Option<Self> { match w { $($w => Some(<$t>::$v),)* _ => None } }
        }
    };
}
words!(SourceScope { All => "all", SelectedList => "selected-list" });
words!(OnWhat { Always => "always", Selected => "selected" });
words!(Navigation { Single => "single", ListDetail => "list-detail" });
words!(Trigger { Button => "button", Menu => "menu", Key => "key", Drop => "drop", QuickAdd => "quick-add" });
words!(Confirm { None => "none", Confirm => "confirm", Choice => "choice" });
words!(OrderBy { Field => "field", Order => "order" });
words!(FilterOp { And => "and", Or => "or", Not => "not", Xor => "xor", Is => "is" });
words!(FilterCond { Equals => "equals", AnyOf => "any-of", Range => "range", IsEmpty => "is-empty", Term => "term" });
words!(Origin { Package => "package", Person => "person", Import => "import" });
words!(OverlayState { Applied => "applied", Conflict => "conflict", Orphaned => "orphaned", Off => "off" });
words!(Position { Before => "before", After => "after", FirstChild => "first-child", LastChild => "last-child" });
words!(Cause {
    TargetRemoved => "target-removed", TargetChanged => "target-changed", AnchorRemoved => "anchor-removed",
    FieldUnknown => "field-unknown", TypeMismatch => "type-mismatch", Locked => "locked", ResultInvalid => "result-invalid",
});
words!(Field {
    TitleKey => "title-key", Style => "style", LabelKey => "label-key", Shortcut => "shortcut", OnWhat => "on-what",
    Command => "command", View => "view", TextKey => "text-key", Frame => "frame", Navigation => "navigation", Block => "block",
    Source => "source", Path => "path", Role => "role", Name => "name", Scheme => "scheme", Message => "message",
    Grouping => "grouping", SearchKind => "search-kind", Descending => "descending", Value => "value", Low => "low", High => "high",
    Trigger => "trigger", Confirm => "confirm", Workflow => "workflow", Range => "range", DataType => "data-type",
});

words!(LogAction {
    Create => "create", Save => "save", Delete => "delete", Import => "import", Install => "install", Update => "update",
    Uninstall => "uninstall", OverlayCreate => "overlay-create", OverlayOps => "overlay-ops", OverlayState => "overlay-state",
    OverlayDelete => "overlay-delete", OverlayImport => "overlay-import", ConflictResolve => "conflict-resolve",
    Lock => "lock", Unlock => "unlock",
});

/// The wire has `kind` and `standard`; the model has one enum.
pub(crate) fn menu_words(k: MenuKind) -> (&'static str, &'static str) {
    match k {
        MenuKind::Menu => ("menu", ""),
        MenuKind::Standard(MenuStandard::Program) => ("standard", "program"),
        MenuKind::Standard(MenuStandard::Help) => ("standard", "help"),
        MenuKind::Command => ("command", ""),
        MenuKind::View => ("view", ""),
        MenuKind::Separator => ("separator", ""),
    }
}

pub(crate) fn menu_from_words(kind: &str, standard: &str) -> Option<MenuKind> {
    Some(match (kind, standard) {
        ("menu", _) => MenuKind::Menu,
        ("standard", "program") => MenuKind::Standard(MenuStandard::Program),
        ("standard", "help") => MenuKind::Standard(MenuStandard::Help),
        ("command", _) => MenuKind::Command,
        ("view", _) => MenuKind::View,
        ("separator", _) => MenuKind::Separator,
        _ => return None,
    })
}

/// `from` word of a binding source.
pub(crate) fn from_word(b: &BindFrom) -> &'static str {
    match b {
        BindFrom::Records => "records",
        BindFrom::Field { .. } => "field",
        BindFrom::Grouping(_) => "grouping",
        BindFrom::Order(_) => "order",
        BindFrom::Command(_) => "command",
        BindFrom::Lock => "lock",
        BindFrom::Condition => "condition",
    }
}
