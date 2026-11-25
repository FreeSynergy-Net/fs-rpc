//! The values of the module contract on the wire (`formsource.capnp`).
//!
//! Pattern: Adapter - Rust values <-> Cap'n Proto, one translation for all modules (no copies). Deviation from the plan: it lives
//! here and not in `fs_interfaces::module::wire` (that file is not landed; card: move it there with the stage 3 landing).
//! An unknown variant of a `#[non_exhaustive]` enum is written as the safest known one, never as "allowed".

use capnp::Result;
use fs_interfaces::common_capnp::{cursor, uuid as uuid_wire};
use fs_interfaces::formsource_capnp::{
    HiddenReason as WireReason, change_ref, conflict, hidden_field, origin, record_page,
    record_view, step_ref, value, write_result,
};
use fs_interfaces::module::{
    Applied, ChangeRef, Conflict, HiddenReason, Invocation, Origin, Page, RecordView, Refusal,
    StepRef, Value,
};
use uuid::Uuid;

use crate::ids::{read_uuid, write_uuid};
use crate::page::check_intake;
use crate::problem::{Problem, invalid_request};

/// The title of a kind of refusal: a short text for people (RFC 9457 `title`), never the kind itself.
fn title_of(kind: &str) -> String {
    match kind {
        "unknown-form" => "Unknown form",
        "unknown-scope" => "Unknown scope",
        "feed-closed" => "Feed closed",
        "read-again" => "Read again later",
        "bad-arguments" => "Bad arguments",
        "unsupported" => "Not supported",
        "unknown-record" => "Unknown record",
        "unknown-command" => "Unknown command",
        "unknown-change" => "Unknown change",
        "forbidden" => "Not allowed",
        "rejected" => "Rejected",
        "conflict" => "Conflict",
        "internal" => "Internal error",
        other => return format!("Refused ({other})"),
    }
    .to_owned()
}

/// The refusal `bad-arguments` 400: a request the module contract cannot read (a cursor that is not text).
fn bad_arguments(detail: &str) -> Refusal {
    let mut r = Refusal::default();
    r.kind = "bad-arguments".into();
    r.status = 400;
    r.detail = detail.into();
    r
}

/// The problem of a refusal: type `form-source-<kind>`, a text for people as title, the code as detail.
pub(super) fn problem_of(refusal: &Refusal) -> Problem {
    Problem::new(
        format!("form-source-{}", refusal.kind),
        title_of(&refusal.kind),
        refusal.detail.clone(),
        refusal.status,
    )
}

/// A refusal as an RPC exception (reads have no partial answer).
pub(super) fn refusal_error(refusal: &Refusal) -> capnp::Error {
    problem_of(refusal).into_error()
}

pub(super) fn read_values(
    list: capnp::struct_list::Reader<'_, value::Owned>,
) -> Result<Vec<Value>> {
    list.iter()
        .map(|v| Ok(Value::new(v.get_path()?.to_str()?, v.get_text()?.to_str()?)))
        .collect()
}

pub(super) fn read_step(r: step_ref::Reader<'_>) -> Result<StepRef> {
    Ok(StepRef::new(
        r.get_run()?.to_str()?,
        r.get_step()?.to_str()?,
    ))
}

/// The change id as the module knows it (UUID text); all zero is "none" and refused.
pub(super) fn read_change_id(r: uuid_wire::Reader<'_>) -> Result<String> {
    read_uuid(r)
        .map(|u| u.to_string())
        .ok_or_else(|| invalid_request("change is missing"))
}

pub(super) fn cursor_text(r: cursor::Reader<'_>) -> Result<String> {
    Ok(std::str::from_utf8(r.get_opaque()?)
        .map_err(|_| refusal_error(&bad_arguments("cursor is not valid")))?
        .to_owned())
}

pub(super) fn write_cursor(mut out: cursor::Builder<'_>, next: &str) {
    out.set_opaque(next.as_bytes());
}

fn write_origin(mut out: origin::Builder<'_>, o: &Origin) {
    out.set_person(o.person.as_str());
    out.set_program(o.program.as_str());
    out.set_instance(o.instance.as_str());
    out.set_device(o.device.as_str());
}

fn parse_uuid(text: &str) -> Result<Uuid> {
    Uuid::parse_str(text).map_err(|_| invalid_request("a change has no valid id"))
}

/// # Errors
/// The activity (or the change it takes back) is not a UUID.
pub(super) fn write_change_ref(mut out: change_ref::Builder<'_>, c: &ChangeRef) -> Result<()> {
    write_uuid(
        out.reborrow().init_activity(),
        Some(parse_uuid(&c.activity)?),
    );
    out.set_object(c.object.as_str());
    out.set_revision(c.revision.as_str());
    out.set_before(c.before.as_str());
    write_origin(out.reborrow().init_origin(), &c.origin);
    out.set_kind(c.kind.as_str());
    out.reborrow().init_at().set_unix_millis(c.at.millis());
    let undoes = c.undoes.as_deref().map(parse_uuid).transpose()?;
    write_uuid(out.init_undoes(), undoes);
    Ok(())
}

pub(super) fn write_conflict(mut out: conflict::Builder<'_>, c: &Conflict) -> Result<()> {
    write_change_ref(out.reborrow().init_by(), &c.by)?;
    let mut list = out.init_fields(u32::try_from(c.fields.len()).unwrap_or(0));
    for (i, f) in (0..).zip(&c.fields) {
        let mut e = list.reborrow().get(i);
        e.set_path(f.path.as_str());
        e.set_current(f.current.as_str());
        e.set_mine(f.mine.as_str());
    }
    Ok(())
}

pub(super) fn write_applied(mut out: write_result::Builder<'_>, a: &Applied) -> Result<()> {
    out.set_record(a.record.as_str());
    out.set_text(a.text.as_str());
    if let Some(change) = &a.change {
        write_change_ref(out.init_change(), change)?;
    }
    Ok(())
}

fn write_reason(r: HiddenReason) -> WireReason {
    match r {
        HiddenReason::Sensitive => WireReason::Sensitive,
        // `NoRight` and every reason this code does not know: the safe wording.
        _ => WireReason::NoRight,
    }
}

fn write_hidden(mut out: hidden_field::Builder<'_>, path: &str, reason: HiddenReason) {
    out.set_path(path);
    out.set_reason(write_reason(reason));
}

fn write_record(mut out: record_view::Builder<'_>, r: &RecordView) {
    out.set_id(r.id.as_str());
    let mut may = out
        .reborrow()
        .init_may(u32::try_from(r.may.len()).unwrap_or(0));
    for (i, c) in (0..).zip(&r.may) {
        may.set(i, c.as_str());
    }
    let mut hidden = out
        .reborrow()
        .init_hidden(u32::try_from(r.hidden.len()).unwrap_or(0));
    for (i, h) in (0..).zip(&r.hidden) {
        write_hidden(hidden.reborrow().get(i), &h.path, h.reason);
    }
    let mut values = out.init_values(u32::try_from(r.values.len()).unwrap_or(0));
    for (i, v) in (0..).zip(&r.values) {
        let mut e = values.reborrow().get(i);
        e.set_path(v.path.as_str());
        e.set_text(v.text.as_str());
    }
}

pub(super) fn write_page(mut out: record_page::Builder<'_>, p: &Page) {
    let mut records = out
        .reborrow()
        .init_records(u32::try_from(p.records.len()).unwrap_or(0));
    for (i, r) in (0..).zip(&p.records) {
        write_record(records.reborrow().get(i), r);
    }
    if let Some(total) = p.total {
        out.set_total(total);
        out.set_has_total(true);
    }
    if let Some(estimate) = p.estimate {
        out.set_total_estimated(estimate);
        out.set_has_estimate(true);
    }
    let mut may = out.init_may(u32::try_from(p.may.len()).unwrap_or(0));
    for (i, c) in (0..).zip(&p.may) {
        may.set(i, c.as_str());
    }
}

/// A rough weight of a page in bytes (texts only), for the answer limit.
pub(super) fn weight(p: &Page) -> usize {
    p.records
        .iter()
        .map(|r| {
            r.id.len()
                + r.may.iter().map(String::len).sum::<usize>()
                + r.values
                    .iter()
                    .map(|v| v.path.len() + v.text.len())
                    .sum::<usize>()
        })
        .sum()
}

pub(super) fn invocation(
    command: capnp::text::Reader<'_>,
    record: capnp::text::Reader<'_>,
    args: capnp::struct_list::Reader<'_, fs_interfaces::formsource_capnp::value::Owned>,
) -> capnp::Result<Invocation> {
    let (command, record) = (command.to_string()?, record.to_string()?);
    let args = read_values(args)?;
    check_intake(
        args.iter()
            .map(|v| v.path.len() + v.text.len())
            .sum::<usize>()
            + command.len()
            + record.len(),
    )?;
    let mut inv = Invocation::default();
    inv.command = command;
    inv.record = (!record.is_empty()).then_some(record);
    inv.args = args;
    Ok(inv)
}
