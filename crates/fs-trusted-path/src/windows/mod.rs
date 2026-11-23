//! The Windows side: who a process is (`sid`), the rights of a folder (`acl`), who is on the other end (`peer`, re-exported by `sid`), and the check of a part of a way (`check`).

pub mod acl;
pub(crate) mod check;
#[cfg(test)]
mod check_tests;
mod form;
#[cfg(test)]
mod form_tests;
mod peer;
pub mod sid;
