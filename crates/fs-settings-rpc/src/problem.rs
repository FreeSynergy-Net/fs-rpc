//! Code <-> `Problem`, the ONE mapping: the problem type is the message key (`setting-error-<code>`), each parameter one extension
//! in the order it was added. A parameter that is a number travels as decimal text and is read back as a number by its key
//! (`line`, `found`, `known`); every other parameter is text. A problem of another type is no setting code (`None`).
//!
//! Pattern: Adapter (code <-> wire problem), one mapping for both directions.

use crate::Coded;
use fs_rpc::Problem;
use fs_settings::SettingCode;
use fs_settings::coded_stub::{ErrorCode, Param, message_key};

/// The parameter keys whose values are numbers.
const NUMBERS: [&str; 3] = ["line", "found", "known"];

/// The HTTP-like status of a code (only for logs and generic clients; the code is what counts).
fn status(code: SettingCode) -> u16 {
    match code {
        SettingCode::Locked | SettingCode::IndexChanged => 409,
        SettingCode::ReadOnly | SettingCode::NoRight => 403,
        SettingCode::StoreUnreachable
        | SettingCode::StoreBusy
        | SettingCode::StoreBroken
        | SettingCode::StoreNewer => 503,
        _ => 422,
    }
}

/// The problem of a code: type = message key, title and detail = the code name (no sentence), one extension per parameter.
#[must_use]
pub fn to_problem(c: &Coded<SettingCode>) -> Problem {
    let name = c.code().as_str();
    let mut p = Problem::new(message_key(c.code()), name, name, status(c.code()));
    for (key, value) in c.params() {
        p = match value {
            Param::Text(t) => p.with_ext(key, t),
            Param::Number(n) => p.with_ext_u64(key, *n),
            _ => p,
        };
    }
    p
}

/// The code of a problem, or `None` when its type is no `setting-error-<code>` of this crate.
#[must_use]
pub fn from_problem(p: &Problem) -> Option<Coded<SettingCode>> {
    let name = p.problem_type.strip_prefix(SettingCode::PREFIX)?;
    let code = SettingCode::ALL
        .iter()
        .copied()
        .find(|c| c.as_str() == name)?;
    let mut out = Coded::new(code);
    for (key, value) in &p.extensions {
        out = match (NUMBERS.contains(&key.as_str()), value.parse::<u64>()) {
            (true, Ok(n)) if value.bytes().all(|b| b.is_ascii_digit()) => out.with_number(key, n),
            _ => out.with_text(key, value.clone()),
        };
    }
    Some(out)
}

/// What a problem of ANY type means to a settings client: a setting code; the explicit "not possible" of the provider rule
/// (the person's service has no administrator) is `read-only{which=instance}`; anything else is `store-broken{cause=wire}`.
#[must_use]
pub(crate) fn meaning(p: &Problem) -> Coded<SettingCode> {
    from_problem(p).unwrap_or_else(|| {
        if p.is_unsupported() {
            Coded::new(SettingCode::ReadOnly).with_text(fs_settings::keys::WHICH, "instance")
        } else {
            crate::wire::broken("problem-type")
        }
    })
}
