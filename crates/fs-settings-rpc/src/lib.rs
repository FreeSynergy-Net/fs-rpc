//! FreeSynergy settings over RPC (ADR 0100; PLAN-speicher-schnittstelle): ONE process owns the settings database, every program
//! reaches it through the contract `settings`. This crate holds both ends and what starts the service:
//!
//! - [`RpcSettingsStore`]: the port `fs_settings::SettingsStore` over the contract (client; cache per stamp; starts without a
//!   service as `store-unreachable`; `update_index` is compare-and-set, at most 3 rounds, then `store-busy`),
//! - [`RpcInstanceAdmin`]: the port `InstanceStore` for the administrator's commands,
//! - [`SettingsService`]: the contract over ANY port (server skeleton; the program fs-settings plugs in its database store),
//! - [`ensure_service`]: one settings service per person (fs-rpc `ensure_single`, the rule of the starter's `SingleGate`),
//! - [`to_problem`] / [`from_problem`]: the one mapping between a code with parameters and a `Problem`.
//! - [`SettingsCore`] + [`SettingsRules`]: the settings as a module of the contract `module_v1` (level 1; served by `fs_rpc::Guarded`): the form
//!   `fields-in-groups@1.0.0`, the commands `value.set` / `value.reset`; a `human_only` key does not exist for a bot (rules, not the module).
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
//! The contract `settings` (0.2) and its generated code are the owner's: `fs-interfaces` v0.0.29 (`fs_interfaces::settings_capnp`),
//! re-exported here; this crate keeps no copy of the schema.
//!
//! Pattern: Adapter (both directions) + Proxy (client cache) + Facade (service); Template Method (steps) - `SettingsCore`; Strategy - `SettingsRules`.
//! Written with Claude (Anthropic) as the coding assistant.

#![deny(missing_docs)]
#![warn(clippy::pedantic)]

mod actor;
mod caller;
mod client;
mod ensure;
mod identity;
mod module_core;
mod module_rows;
mod module_rules;
mod problem;
mod service;
pub use fs_interfaces::settings_capnp;

mod wire;
mod wire_log;

pub(crate) use fs_settings::{Coded, IndexFn};

pub use actor::{Actor, ActorOfConnection, FailClosed, OsUserIsPerson};
pub use caller::current_os_user;
pub use client::{
    ClientTimes, LIVELOCK_MAX, RpcInstanceAdmin, RpcSettingsStore, Watching, unreachable_at,
};
pub use ensure::{SERVICE_PROGRAM, ensure_service, ensure_service_with, find_service_program};
pub use identity::{
    actor_of, claim_line, id_usable, log_claim, log_refused, os_user_of, refused_line,
};
pub use module_core::{LEVEL as MODULE_LEVEL, RESET, SET, SettingsCore};
pub use module_rows::FORM;
pub use module_rules::{HUMAN_ONLY, SettingsRules};
pub use problem::{CALLER_UNKNOWN, HISTORY_CUT, HISTORY_STUCK, from_problem, to_problem};
pub use service::{ClaimNote, OsUserOf, SettingsCallers, SettingsService};
