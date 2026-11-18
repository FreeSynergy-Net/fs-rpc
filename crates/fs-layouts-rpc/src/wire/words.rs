//! The closed words of the wire: one table per enum, read and written from the same place (the enums of the file words delegate to the
//! model crate's own table; the others, which the model crate has no words for yet, are here).
//!
//! Pattern: Value Object with a 1:1 text form.

use fs_forms_layout::{
    BindFrom, Cause, Confirm, Field, FilterCond, FilterOp, LogAction, MenuKind, Navigation, OnWhat,
    OrderBy, Origin, OverlayState, Position, SourceScope, Trigger,
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

/// The enums whose words the model crate owns (the words of the exchange file): ONE table, ours only delegates.
macro_rules! shared {
    ($($t:ty),*) => {$(
        impl Words for $t {
            fn word(self) -> &'static str { <$t as fs_forms_layout::Words>::word(self) }
            fn from_word(w: &str) -> Option<Self> { <$t as fs_forms_layout::Words>::from_word(w) }
        }
    )*};
}
shared!(SourceScope, OnWhat, Navigation, Trigger, Confirm, OrderBy);
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

/// The wire has `kind` and `standard`; the model's file word is `standard-program`, `standard-help` or a plain kind.
pub(crate) fn menu_words(k: MenuKind) -> (&'static str, &'static str) {
    let word = fs_forms_layout::Words::word(k);
    word.strip_prefix("standard-")
        .map_or((word, ""), |std| ("standard", std))
}

pub(crate) fn menu_from_words(kind: &str, standard: &str) -> Option<MenuKind> {
    let word = if kind == "standard" {
        format!("standard-{standard}")
    } else {
        kind.to_owned()
    };
    <MenuKind as fs_forms_layout::Words>::from_word(&word)
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
