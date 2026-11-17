//! Locks, ids and the instance log on the wire.
//!
//! Pattern: Adapter (both directions), as `wire`.

use fs_settings::{Coded, InstanceLogEntry, Lock, Raw, SettingCode, SettingKey};
use fs_types::{ObjectId, Utc};

use crate::settings_capnp::{self as w, raw};
use crate::wire::{broken, read_raw, write_raw};

fn from_capnp(e: &capnp::Error) -> Coded<SettingCode> {
    broken(&e.to_string())
}

/// Writes a lock.
pub fn write_lock(mut out: w::lock::Builder<'_>, l: &Lock) {
    out.set_reason(l.reason.as_str());
    out.set_locked_by(l.locked_by.as_str());
    out.reborrow()
        .init_locked_at()
        .set_unix_millis(l.locked_at.millis());
}

/// Reads a lock.
///
/// # Errors
/// `store-broken{cause=wire}`.
pub fn read_lock(r: w::lock::Reader<'_>) -> Result<Lock, Coded<SettingCode>> {
    let text = |t: capnp::Result<capnp::text::Reader<'_>>| -> Result<String, Coded<SettingCode>> {
        t.map_err(|e| from_capnp(&e))?
            .to_string()
            .map_err(|_| broken("utf8"))
    };
    Ok(Lock {
        reason: text(r.get_reason())?,
        locked_by: text(r.get_locked_by())?,
        locked_at: Utc::from_millis(
            r.get_locked_at()
                .map_err(|e| from_capnp(&e))?
                .get_unix_millis(),
        ),
    })
}

/// Writes an id (16 bytes, big endian) as the wire `Uuid`.
pub fn write_id(mut out: fs_interfaces::common_capnp::uuid::Builder<'_>, id: Option<ObjectId>) {
    let b = id.map_or([0_u8; 16], |i| i.0);
    out.set_hi(u64::from_be_bytes(b[..8].try_into().unwrap_or_default()));
    out.set_lo(u64::from_be_bytes(b[8..].try_into().unwrap_or_default()));
}

/// Reads an id; all zero is `None`.
#[must_use]
pub fn read_id(r: fs_interfaces::common_capnp::uuid::Reader<'_>) -> Option<ObjectId> {
    let mut b = [0_u8; 16];
    b[..8].copy_from_slice(&r.get_hi().to_be_bytes());
    b[8..].copy_from_slice(&r.get_lo().to_be_bytes());
    (b != [0_u8; 16]).then_some(ObjectId(b))
}

fn action_to_wire(a: &str) -> w::LogAction {
    match a {
        "clear" => w::LogAction::Clear,
        "lock" => w::LogAction::Lock,
        "unlock" => w::LogAction::Unlock,
        _ => w::LogAction::Set,
    }
}

fn action_of_wire(a: w::LogAction) -> &'static str {
    match a {
        w::LogAction::Set => "set",
        w::LogAction::Clear => "clear",
        w::LogAction::Lock => "lock",
        w::LogAction::Unlock => "unlock",
    }
}

/// Writes a log entry.
pub fn write_log_entry(mut out: w::log_entry::Builder<'_>, e: &InstanceLogEntry) {
    write_id(out.reborrow().init_id(), Some(e.id));
    out.reborrow().init_at().set_unix_millis(e.at.millis());
    out.set_by(e.by.as_str());
    out.set_os_user(e.os_user.as_deref().unwrap_or(""));
    out.set_action(action_to_wire(e.action));
    out.set_key(e.key.as_str());
    out.set_reason(e.reason.as_deref().unwrap_or(""));
    if let Some(o) = &e.old {
        out.set_has_old(true);
        write_raw(out.reborrow().init_old(), o);
    }
    if let Some(n) = &e.new {
        out.set_has_new(true);
        write_raw(out.reborrow().init_new(), n);
    }
}

/// Reads a log entry.
///
/// # Errors
/// `store-broken{cause=wire}`.
pub fn read_log_entry(r: w::log_entry::Reader<'_>) -> Result<InstanceLogEntry, Coded<SettingCode>> {
    let text = |t: capnp::Result<capnp::text::Reader<'_>>| -> Result<String, Coded<SettingCode>> {
        t.map_err(|e| from_capnp(&e))?
            .to_string()
            .map_err(|_| broken("utf8"))
    };
    let os_user = text(r.get_os_user())?;
    let reason = text(r.get_reason())?;
    let value =
        |has: bool, v: capnp::Result<raw::Reader<'_>>| -> Result<Option<Raw>, Coded<SettingCode>> {
            has.then(|| read_raw(v.map_err(|e| from_capnp(&e))?))
                .transpose()
        };
    Ok(InstanceLogEntry {
        id: read_id(r.get_id().map_err(|e| from_capnp(&e))?).ok_or_else(|| broken("log-id"))?,
        at: Utc::from_millis(r.get_at().map_err(|e| from_capnp(&e))?.get_unix_millis()),
        by: text(r.get_by())?,
        os_user: Some(os_user).filter(|s| !s.is_empty()),
        action: action_of_wire(r.get_action().map_err(|e| broken(&e.to_string()))?),
        key: SettingKey::parse(&text(r.get_key())?)?,
        reason: Some(reason).filter(|s| !s.is_empty()),
        old: value(r.get_has_old(), r.get_old())?,
        new: value(r.get_has_new(), r.get_new())?,
    })
}
