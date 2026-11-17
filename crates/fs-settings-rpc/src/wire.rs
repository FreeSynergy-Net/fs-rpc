//! Values between Rust and the contract `settings`: raw values, scopes, places, snapshots, the profile index.
//!
//! Pattern: Adapter (both directions). A wire message that does not fit what the port knows is a `Coded` error
//! (`store-broken{cause=wire}`), never a panic and never a silent default.

use std::collections::BTreeMap;

use fs_interfaces::common_capnp::problem;
use fs_rpc::Problem;
use fs_types::{ObjectId, Utc};
use fs_settings::{
    Coded, InstanceLogEntry, Lock, Place, ProfileName, ProfileRef, ProfileSet, ProgramName, Raw, Scope, SettingCode,
    SettingKey, Snapshot, keys,
};

use crate::problem::{from_problem, to_problem};
use crate::settings_capnp::{self as w, entry, index, named, raw, scope};

/// `store-broken{cause=wire}`: the other end sent something the contract does not allow.
#[must_use]
pub fn broken(what: &str) -> Coded<SettingCode> {
    Coded::new(SettingCode::StoreBroken)
        .with_text(keys::CAUSE, "wire")
        .with_text(keys::KIND, what)
}

fn from_capnp(e: &capnp::Error) -> Coded<SettingCode> {
    broken(&e.to_string())
}

/// Writes a raw value.
pub fn write_raw(mut out: raw::Builder<'_>, r: &Raw) {
    match r {
        Raw::Bool(b) => out.set_flag(*b),
        Raw::Int(i) => out.set_whole(*i),
        Raw::Str(s) => out.set_text(s.as_str()),
    }
}

/// Reads a raw value.
///
/// # Errors
/// `store-broken{cause=wire}`.
pub fn read_raw(r: raw::Reader<'_>) -> Result<Raw, Coded<SettingCode>> {
    Ok(match r.which().map_err(|e| broken(&e.to_string()))? {
        raw::Flag(b) => Raw::Bool(b),
        raw::Whole(i) => Raw::Int(i),
        raw::Text(t) => Raw::Str(t.map_err(|e| from_capnp(&e))?.to_string().map_err(|_| broken("utf8"))?),
    })
}

/// Writes a scope.
pub fn write_scope(mut out: scope::Builder<'_>, s: &Scope) {
    match s {
        Scope::General => out.set_general(()),
        Scope::Program(p) => out.set_program(p.as_str()),
    }
}

/// Reads a scope.
///
/// # Errors
/// `program-invalid` for a bad program name; `store-broken{cause=wire}` otherwise.
pub fn read_scope(r: scope::Reader<'_>) -> Result<Scope, Coded<SettingCode>> {
    match r.which().map_err(|e| broken(&e.to_string()))? {
        scope::General(()) => Ok(Scope::General),
        scope::Program(t) => {
            let name = t.map_err(|e| from_capnp(&e))?.to_str().map_err(|_| broken("utf8"))?;
            Ok(Scope::Program(ProgramName::new(name)?))
        }
    }
}

/// The wire form of a place.
#[must_use]
pub fn place_to_wire(p: Place) -> w::Place {
    match p {
        Place::Profile => w::Place::Profile,
        Place::Device => w::Place::Device,
    }
}

/// The port form of a place.
#[must_use]
pub fn place_of_wire(p: w::Place) -> Place {
    match p {
        w::Place::Profile => Place::Profile,
        w::Place::Device => Place::Device,
    }
}

/// Writes the problems of a list as wire problems.
pub fn write_problems(
    list: capnp::struct_list::Builder<'_, problem::Owned>,
    problems: &[Coded<SettingCode>],
) {
    let mut list = list;
    for (i, c) in (0..).zip(problems) {
        to_problem(c).write(list.reborrow().get(i));
    }
}

/// Reads wire problems back as codes; a problem of another type becomes `store-broken{cause=wire}`.
///
/// # Errors
/// A malformed list.
pub fn read_problems(
    list: capnp::struct_list::Reader<'_, problem::Owned>,
) -> Result<Vec<Coded<SettingCode>>, Coded<SettingCode>> {
    let mut out = Vec::new();
    for p in list {
        let p = Problem::read(p).map_err(|e| from_capnp(&e))?;
        out.push(from_problem(&p).unwrap_or_else(|| broken("problem")));
    }
    Ok(out)
}

/// Writes a snapshot.
pub fn write_snapshot(mut out: w::snapshot::Builder<'_>, s: &Snapshot) {
    let mut values = out.reborrow().init_values(u32::try_from(s.values.len()).unwrap_or(0));
    for (i, (k, v)) in (0..).zip(&s.values) {
        let mut e = values.reborrow().get(i);
        e.set_key(k.as_str());
        write_raw(e.init_value(), v);
    }
    write_problems(
        out.init_problems(u32::try_from(s.problems.len()).unwrap_or(0)),
        &s.problems,
    );
}

/// Reads a snapshot.
///
/// # Errors
/// `store-broken{cause=wire}` for a key or value that is no setting.
pub fn read_snapshot(r: w::snapshot::Reader<'_>) -> Result<Snapshot, Coded<SettingCode>> {
    let mut values = BTreeMap::new();
    for e in r.get_values().map_err(|e| from_capnp(&e))? {
        let e: entry::Reader<'_> = e;
        let key = e.get_key().map_err(|e| from_capnp(&e))?.to_str().map_err(|_| broken("utf8"))?;
        let key = SettingKey::parse(key)?;
        values.insert(key, read_raw(e.get_value().map_err(|e| from_capnp(&e))?)?);
    }
    let problems = read_problems(r.get_problems().map_err(|e| from_capnp(&e))?)?;
    Ok(Snapshot { values, problems })
}

/// The profile index as the wire index: base value = `top`, `active` empty = the base.
pub fn write_index(mut out: index::Builder<'_>, set: &ProfileSet<u64>) {
    out.set_top(set.get(&ProfileRef::Base).copied().unwrap_or(0));
    let names: Vec<&ProfileName> = set.names().collect();
    let mut list = out.reborrow().init_named(u32::try_from(names.len()).unwrap_or(0));
    for (i, n) in (0..).zip(names) {
        let mut e: named::Builder<'_> = list.reborrow().get(i);
        e.set_id(set.get(&ProfileRef::Named(n.clone())).copied().unwrap_or(0));
        e.set_name(n.as_str());
    }
    out.set_active(match set.active() {
        ProfileRef::Base => "",
        ProfileRef::Named(n) => n.as_str(),
    });
}

/// Reads a profile index.
///
/// # Errors
/// `store-broken{cause=wire}`: a bad or doubled name, or an active name that is not in the list.
pub fn read_index(r: index::Reader<'_>) -> Result<ProfileSet<u64>, Coded<SettingCode>> {
    let mut set = ProfileSet::new(r.get_top());
    for e in r.get_named().map_err(|e| from_capnp(&e))? {
        let name = e.get_name().map_err(|e| from_capnp(&e))?.to_str().map_err(|_| broken("utf8"))?;
        let name = ProfileName::new(name).map_err(|_| broken("profile-name"))?;
        set.create(name.clone(), None).map_err(|_| broken("profile-name"))?;
        if let Some(id) = set.get_mut(&ProfileRef::Named(name)) {
            *id = e.get_id();
        }
    }
    let active = r.get_active().map_err(|e| from_capnp(&e))?.to_str().map_err(|_| broken("utf8"))?;
    if !active.is_empty() {
        let name = ProfileName::new(active).map_err(|_| broken("profile-name"))?;
        set.switch(ProfileRef::Named(name)).map_err(|_| broken("active-profile"))?;
    }
    Ok(set)
}

/// Writes a lock.
pub fn write_lock(mut out: w::lock::Builder<'_>, l: &Lock) {
    out.set_reason(l.reason.as_str());
    out.set_locked_by(l.locked_by.as_str());
    out.reborrow().init_locked_at().set_unix_millis(l.locked_at.millis());
}

/// Reads a lock.
///
/// # Errors
/// `store-broken{cause=wire}`.
pub fn read_lock(r: w::lock::Reader<'_>) -> Result<Lock, Coded<SettingCode>> {
    let text = |t: capnp::Result<capnp::text::Reader<'_>>| -> Result<String, Coded<SettingCode>> {
        t.map_err(|e| from_capnp(&e))?.to_string().map_err(|_| broken("utf8"))
    };
    Ok(Lock {
        reason: text(r.get_reason())?,
        locked_by: text(r.get_locked_by())?,
        locked_at: Utc::from_millis(r.get_locked_at().map_err(|e| from_capnp(&e))?.get_unix_millis()),
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
        t.map_err(|e| from_capnp(&e))?.to_string().map_err(|_| broken("utf8"))
    };
    let os_user = text(r.get_os_user())?;
    let reason = text(r.get_reason())?;
    let value = |has: bool, v: capnp::Result<raw::Reader<'_>>| -> Result<Option<Raw>, Coded<SettingCode>> {
        has.then(|| read_raw(v.map_err(|e| from_capnp(&e))?)).transpose()
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
