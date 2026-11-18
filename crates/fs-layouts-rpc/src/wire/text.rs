//! Small helpers shared by the wire modules: text out of a reader, "" as "not given", list sizes.
//!
//! Pattern: none; plain total functions.

use fs_forms_layout::{Coded, ElemId, LayoutCode};

/// A read that can fail with a wire problem.
pub(crate) type R<T> = Result<T, Coded<LayoutCode>>;

/// `format-invalid{found=<what>}`: the wire holds something the model has no value for.
pub(crate) fn bad(what: &str) -> Coded<LayoutCode> {
    Coded::new(LayoutCode::FormatInvalid).with("found", what)
}

/// The text of a wire field as an owned `String` (a text that is no UTF-8 is a wire problem, never replaced).
pub(crate) fn txt(t: capnp::Result<capnp::text::Reader<'_>>) -> R<String> {
    t.ok()
        .and_then(|t| t.to_str().ok().map(str::to_owned))
        .ok_or_else(|| bad("text"))
}

/// `""` is "not given".
pub(crate) fn opt(t: capnp::Result<capnp::text::Reader<'_>>) -> R<Option<String>> {
    txt(t).map(|s| (!s.is_empty()).then_some(s))
}

/// The element id of a wire field; `""` = none; a wrong id is `elem-invalid{elem}` as everywhere.
pub(crate) fn elem(t: capnp::Result<capnp::text::Reader<'_>>) -> R<Option<ElemId>> {
    opt(t)?.map(|s| ElemId::parse(&s)).transpose()
}

/// A word of a closed list, or `format-invalid{found=<word>}`.
pub(crate) fn word<W: super::words::Words>(t: capnp::Result<capnp::text::Reader<'_>>) -> R<W> {
    let w = txt(t)?;
    W::from_word(&w).ok_or_else(|| bad(&w))
}

/// The element count of a list as the wire counts.
pub(crate) fn n<T>(v: &[T]) -> u32 {
    u32::try_from(v.len()).unwrap_or(u32::MAX)
}

/// A list of problems: `Common.Problem` through fs-rpc's own reader/writer, then the code.
pub(crate) fn read_problems(
    l: capnp::struct_list::Reader<'_, fs_interfaces::common_capnp::problem::Owned>,
) -> R<Vec<Coded<LayoutCode>>> {
    l.iter()
        .map(|p| {
            let p = fs_rpc::Problem::read(p).map_err(|_| bad("problem"))?;
            Ok(crate::problem::meaning(&p))
        })
        .collect()
}

pub(crate) fn write_problems(
    mut out: capnp::struct_list::Builder<'_, fs_interfaces::common_capnp::problem::Owned>,
    list: &[Coded<LayoutCode>],
) {
    for (i, c) in (0..).zip(list) {
        crate::problem::to_problem(c).write(out.reborrow().get(i));
    }
}
