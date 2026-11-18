//! FreeSynergy layouts over RPC (probe forms F2): the descriptions of the frontends (`fs-forms-layout`) live in ONE database of
//! ONE process, `fs-forms serve`; the host and every other program reach them through the contract `layouts`. This crate holds
//! both ends of that wire:
//!
//! - [`RpcLayouts`]: the ports `LayoutRead`, `LayoutWrite`, `PackageLayouts` and `OverlayWrite` over the contract (client; starts
//!   without a service: every call says `unreachable{socket}`), plus [`RpcLayouts::watch`] for the live update,
//! - [`RpcLayoutAdmin`]: the port `LayoutAdmin` for the administrator,
//! - [`LayoutService`]: the contract over ANY port (server skeleton; `fs-forms serve` plugs in its database store),
//! - [`ensure_service`]: one service per person (fs-rpc `ensure_single`),
//! - [`to_problem`] / [`from_problem`]: the one mapping between a code with parameters and a `Problem` (`layout-error-<code>`).
//!
//! Errors are codes with parameters, never sentences. Limits are settings ([`ServiceLimits`], [`ClientTimes`]), passed in by the
//! program with their source; only the 64 MiB of one message are fixed (`fs_rpc::MAX_MESSAGE_BYTES`). Nothing is silent: what the
//! port cannot do (the exchange files, `repair`, `bind`) answers `unsupported{what}`.
//!
//! PROVISIONAL (probe): the contract `layouts` (draft 0.1) and the host's `bind` are provisional copies in `schema/` of the F1b and
//! H1 sketches (with four appended fields, see the schema); `fs-forms-layout` is a git pin. The tag replaces both before release.
//!
//! Pattern: Adapter (both directions) + Proxy (client link) + Facade (service) + Observer (watch).
//! Written with Claude (Anthropic) as the coding assistant.

#![deny(missing_docs)]
#![warn(clippy::pedantic)]

mod client;
mod ensure;
mod fail;
mod limits;
mod problem;
mod service;
mod wire;

// The generated files name `crate::common_capnp` and `crate::bind_capnp`, as they do inside fs-interfaces.
pub(crate) use fs_interfaces::common_capnp;
#[rustfmt::skip]
#[allow(clippy::all, clippy::pedantic, unused_parens, dead_code, missing_docs)]
pub mod bind_capnp;
#[rustfmt::skip]
#[allow(clippy::all, clippy::pedantic, unused_parens, dead_code, missing_docs)]
pub mod layouts_capnp;

pub use client::{ClientTimes, RpcLayoutAdmin, RpcLayouts, Watching};
pub use ensure::{
    SERVICE_PROGRAM, ensure_service, ensure_service_with, find_service_program, unreachable_at,
};
pub use limits::ServiceLimits;
pub use problem::{from_problem, to_problem};
pub use service::{LayoutService, Watchers};
