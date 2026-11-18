//! FreeSynergy settings over RPC (ADR 0100; PLAN-speicher-schnittstelle): ONE process owns the settings database, every program
//! reaches it through the contract `settings`. This crate holds both ends and what starts the service:
//!
//! - [`RpcSettingsStore`]: the port `fs_settings::SettingsStore` over the contract (client; cache per stamp; starts without a
//!   service as `store-unreachable`; `update_index` is compare-and-set, at most 3 rounds, then `store-busy`),
//! - [`RpcInstanceAdmin`]: the port `InstanceStore` for the administrator's commands,
//! - [`SettingsService`]: the contract over ANY port (server skeleton; the program fs-settings plugs in its database store),
//! - [`ensure_service`]: one settings service per person (fs-rpc `ensure_single`, the rule of the starter's `SingleGate`),
//! - [`to_problem`] / [`from_problem`]: the one mapping between a code with parameters and a `Problem`.
//!
//! Who calls: the CALLER of each connection is named by the system (fs-rpc `Caller`: `SO_PEERCRED` on Unix, the pipe client's token on
//! Windows), never by a field the client sends, an argument or the environment. [`SettingsService::callers`] builds one hub per
//! connection with its caller; the actor of the administrator is derived from `Caller::id` only (`urn:fs:os-user:<id>`), the log's OS
//! user is the NAME the system has (the number when it has none; one form). The `by` of `admin(by)` is only a claim: it goes to
//! `SettingsService::claimed_by` (default: standard error) and is ignored otherwise ([AKTEUR-AUS-VERBINDUNG]; `by` leaves the
//! contract in 0.3). No capability that carries a caller is kept in state the connections share (fs-rpc H2). Today the transport
//! admits only the service's own user (fs-rpc H1), so every caller is that user. The program is the word of `open` (an API promise, no
//! security boundary until fs-access); time of the log is the service's.
//!
//! PROVISIONAL (probe): the contract `settings` (draft 0.2) is a byte-equal copy of the files of the fs-interfaces branch
//! `unreviewed/interfaces-settings` (08f0a93: `schema/settings.capnp`, `settings_capnp.rs`) until fs-interfaces 0.0.28 carries it.
//!
//! Pattern: Adapter (both directions) + Proxy (client cache) + Facade (service).
//! Written with Claude (Anthropic) as the coding assistant.

#![deny(missing_docs)]
#![warn(clippy::pedantic)]

mod caller;
mod client;
mod ensure;
mod identity;
mod problem;
mod service;
// The copy of B0's generated file names `crate::common_capnp`, as it does inside fs-interfaces.
pub(crate) use fs_interfaces::common_capnp;
#[rustfmt::skip]
#[allow(clippy::all, clippy::pedantic, unused_parens, dead_code, missing_docs)]
pub mod settings_capnp;

mod wire;
mod wire_log;

pub(crate) use fs_settings::{Coded, IndexFn};

pub use caller::current_os_user;
pub use client::{ClientTimes, RpcInstanceAdmin, RpcSettingsStore, Watching, unreachable_at};
pub use ensure::{SERVICE_PROGRAM, ensure_service, ensure_service_with, find_service_program};
pub use identity::{actor_of, claim_line, log_claim, log_refused, os_user_of, refused_line};
pub use problem::{from_problem, to_problem};
pub use service::{ClaimNote, SettingsCallers, SettingsService};
