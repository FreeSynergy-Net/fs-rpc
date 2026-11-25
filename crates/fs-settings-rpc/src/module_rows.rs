//! The rows of the form `fields-in-groups@1.0.0` of the settings module, and the words of the module for what the store says.
//!
//! Pattern: Adapter - `fs_settings` values and codes to the values (`Value`, `Refusal`) of the module contract; no storage here.
//! Texts are no sentences: the label and the hint are the MF2 keys of the declaration (`setting-label-...`, `setting-about-...`).

use fs_interfaces::module::{RecordView, Refusal, Value as Field};
use fs_settings::{Coded, Declaration, Effective, Kind, SettingCode, Value};
use fs_types::{BasicValue, Language};

/// The form this module serves.
pub const FORM: &str = "fields-in-groups";

/// What a refusal of the contract is made of: the bare kind, its status and a code.
pub(crate) fn refuse(kind: &str, status: u16, detail: &str) -> Refusal {
    let mut r = Refusal::default();
    kind.clone_into(&mut r.kind);
    r.status = status;
    detail.clone_into(&mut r.detail);
    r
}

/// A code of the store as a refusal: a locked setting is a state of the data (409 `conflict`, detail `locked`), an unknown key is an
/// unknown record, a value that does not fit is `bad-arguments`, a store that cannot write is `internal`.
pub(crate) fn refusal_of(code: &Coded<SettingCode>) -> Refusal {
    match code.code() {
        SettingCode::Locked => refuse("conflict", 409, "locked"),
        SettingCode::NotDeclared => refuse("unknown-record", 404, "no such setting"),
        SettingCode::WrongType
        | SettingCode::OutOfRange
        | SettingCode::UnknownChoice
        | SettingCode::BadLanguage
        | SettingCode::TextTooLong => refuse("bad-arguments", 400, "value does not fit"),
        _ => refuse("internal", 500, "store"),
    }
}

/// The text of a value as a form shows it (`true`/`false`, a number, a word).
pub(crate) fn text_of(v: &Value) -> String {
    match v {
        Value::Flag(b) => b.to_string(),
        Value::Whole(n) => n.to_string(),
        Value::Choice(s) | Value::Text(s) => s.clone(),
        Value::Language(l) => l.canonical(),
    }
}

/// A text as the value of `kind`; `None` when it is not even of the right shape (the store then checks the limits).
pub(crate) fn value_of(kind: &Kind, text: &str) -> Option<Value> {
    match kind {
        Kind::Flag => text.parse().ok().map(Value::Flag),
        Kind::Whole { .. } => text.parse().ok().map(Value::Whole),
        Kind::Choice { .. } => Some(Value::Choice(text.to_owned())),
        Kind::Language => Language::parse(text).ok().map(Value::Language),
        Kind::Text { .. } => Some(Value::Text(text.to_owned())),
        _ => None,
    }
}

fn kind_word(kind: &Kind) -> &'static str {
    match kind {
        Kind::Flag => "flag",
        Kind::Whole { .. } => "whole",
        Kind::Choice { .. } => "choice",
        Kind::Language => "language",
        Kind::Text { .. } => "text",
        _ => "other",
    }
}

/// The record of one setting: id = the key; the fields of `fields-in-groups@1.0.0`.
pub(crate) fn row(d: &Declaration, e: &Effective, order: usize, may: &[String]) -> RecordView {
    let mut v = RecordView::default();
    d.key.as_str().clone_into(&mut v.id);
    v.may = may.to_vec();
    let mut put = |path: &str, text: String| v.values.push(Field::new(path, &text));
    put("label", d.label_key());
    put("basic", kind_word(&d.kind).to_owned());
    put("value", text_of(&e.value));
    put("group", d.group.as_str().to_owned());
    put("hint", d.about_key());
    put("default", text_of(&d.default));
    put("layer", e.from.as_str().to_owned());
    put("locked", e.lock.is_some().to_string());
    put("order", order.to_string());
    if let Some(lock) = &e.lock {
        put("lock-reason", lock.reason.clone());
    }
    match &d.kind {
        Kind::Choice { keys: list } => put("choices", list.join(",")),
        Kind::Whole { min, max } => {
            put("min", min.to_string());
            put("max", max.to_string());
        }
        _ => {}
    }
    v
}
