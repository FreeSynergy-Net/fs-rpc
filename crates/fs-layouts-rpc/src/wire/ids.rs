//! Ids and times on the wire: an `ObjectId` (UUID text) is `Common.Uuid`, `Utc` is `Common.Timestamp`; reading a list of problems.
//!
//! Pattern: Adapter (value <-> wire struct), small total functions.

use fs_forms_layout::{Coded, LayoutCode, LayoutId, ObjectId, OverlayId, Utc};
use fs_interfaces::common_capnp::{timestamp, uuid};

/// The all-zero UUID: "no id" in a cursor (`after`).
pub(crate) const NIL: (u64, u64) = (0, 0);

fn bad(found: &str) -> Coded<LayoutCode> {
    Coded::new(LayoutCode::FormatInvalid).with("found", found)
}

/// `8-4-4-4-12` hex text to the two halves.
pub(crate) fn parse_uuid(text: &str) -> Result<(u64, u64), Coded<LayoutCode>> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    let dashes_ok = text.len() == 36
        && [8, 13, 18, 23]
            .iter()
            .all(|i| text.as_bytes().get(*i) == Some(&b'-'));
    if hex.len() != 32 || !dashes_ok || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("uuid"));
    }
    let v = u128::from_str_radix(&hex, 16).map_err(|_| bad("uuid"))?;
    #[allow(clippy::cast_possible_truncation)] // the two halves of a 128 bit value
    Ok(((v >> 64) as u64, v as u64))
}

/// The two halves to `8-4-4-4-12` lower-case hex text.
pub(crate) fn uuid_text(hi: u64, lo: u64) -> String {
    let h = format!("{hi:016x}{lo:016x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

pub(crate) fn write_id(mut out: uuid::Builder<'_>, id: &ObjectId) -> Result<(), Coded<LayoutCode>> {
    let (hi, lo) = parse_uuid(&id.0)?;
    out.set_hi(hi);
    out.set_lo(lo);
    Ok(())
}

pub(crate) fn write_nil(mut out: uuid::Builder<'_>) {
    out.set_hi(NIL.0);
    out.set_lo(NIL.1);
}

pub(crate) fn read_id(r: uuid::Reader<'_>) -> ObjectId {
    ObjectId(uuid_text(r.get_hi(), r.get_lo()))
}

/// `None` for the all-zero id.
pub(crate) fn read_cursor(r: uuid::Reader<'_>) -> Option<ObjectId> {
    ((r.get_hi(), r.get_lo()) != NIL).then(|| read_id(r))
}

pub(crate) fn layout_id(r: uuid::Reader<'_>) -> LayoutId {
    LayoutId(read_id(r))
}

pub(crate) fn overlay_id(r: uuid::Reader<'_>) -> OverlayId {
    OverlayId(read_id(r))
}

pub(crate) fn write_time(mut out: timestamp::Builder<'_>, at: Utc) {
    out.set_unix_millis(at.0);
}

pub(crate) fn read_time(r: timestamp::Reader<'_>) -> Utc {
    Utc(r.get_unix_millis())
}
