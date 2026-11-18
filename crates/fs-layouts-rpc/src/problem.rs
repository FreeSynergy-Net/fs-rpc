//! Code <-> `Problem`, the ONE mapping (layout rule: `type` = message key `layout-error-<code>`, each parameter one extension in
//! the order it was added; title and detail are the code name, never a sentence). Parameters of `Coded` are text, so they travel as
//! text. A problem of another type is no layout code (`None`); the explicit "not possible" of the provider rule keeps its own meaning.
//!
//! Pattern: Adapter (code <-> wire problem), one mapping for both directions.

use fs_forms_layout::{Coded, ErrorCode, LayoutCode};
use fs_rpc::Problem;

/// The HTTP-like status of a code (for logs and generic clients only; the code is what counts).
fn status(code: LayoutCode) -> u16 {
    match code {
        LayoutCode::Locked | LayoutCode::RevChanged | LayoutCode::Exists => 409,
        LayoutCode::ReadOnly | LayoutCode::PackageOwned | LayoutCode::NotOverlayable => 403,
        LayoutCode::Unknown => 404,
        LayoutCode::Unsupported => 501,
        LayoutCode::ContentTooLarge => 413,
        LayoutCode::StoreBroken
        | LayoutCode::StoreNewer
        | LayoutCode::StoreBusy
        | LayoutCode::Unreachable => 503,
        _ => 422,
    }
}

/// The problem of a code: type = message key, title and detail = the code name, one extension per parameter.
#[must_use]
pub fn to_problem(c: &Coded<LayoutCode>) -> Problem {
    let name = c.code.as_str();
    c.params.iter().fold(
        Problem::new(c.code.message_key(), name, name, status(c.code)),
        |p, (k, v)| p.with_ext(k, v),
    )
}

/// The code of a problem, or `None` when its type is no `layout-error-<code>`.
#[must_use]
pub fn from_problem(p: &Problem) -> Option<Coded<LayoutCode>> {
    let code = LayoutCode::parse(p.problem_type.strip_prefix(LayoutCode::PREFIX)?)?;
    Some(Coded {
        code,
        params: p.extensions.clone(),
    })
}

/// What a problem of ANY type means to a layouts client: its layout code; the explicit "not possible" is `unsupported{what}`
/// (`what` = the capability named in the problem); anything else is `format-invalid{found=problem-type}` (the answer is not of
/// this contract; never guessed into another code).
#[must_use]
pub(crate) fn meaning(p: &Problem) -> Coded<LayoutCode> {
    from_problem(p).unwrap_or_else(|| {
        if p.is_unsupported() {
            Coded::new(LayoutCode::Unsupported).with(
                "what",
                p.detail
                    .split_once(": ")
                    .map_or(p.detail.as_str(), |(what, _)| what),
            )
        } else {
            Coded::new(LayoutCode::FormatInvalid).with("found", "problem-type")
        }
    })
}

/// A `Coded` as the RPC error a client reads back.
#[must_use]
pub(crate) fn err(c: &Coded<LayoutCode>) -> capnp::Error {
    to_problem(c).into_error()
}
