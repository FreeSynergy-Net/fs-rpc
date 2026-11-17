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
//! Who calls: the OS user is the connection (fs-rpc lets only the same OS user in); the program is the word of `open` (an API
//! promise, no security boundary until fs-access); the actor of the administrator is `by`; time and OS user of the log are the
//! service's, never the caller's.
//!
//! PROVISIONAL (probe): the contract `settings` (draft 0.2) comes from the fs-interfaces branch `unreviewed/interfaces-settings`
//! by `rev`; the tag of 0.0.28 replaces the pin.
//!
//! Pattern: Adapter (both directions) + Proxy (client cache) + Facade (service).
//! Written with Claude (Anthropic) as the coding assistant.

#![deny(missing_docs)]
#![warn(clippy::pedantic)]

mod caller;
mod client;
mod ensure;
mod problem;
mod service;
pub(crate) use fs_interfaces::settings_capnp;

mod wire;
mod wire_log;

pub(crate) use fs_settings::coded_stub::Coded;

/// The closure type of `SettingsStore::update_index` (the port does not export its alias).
pub(crate) type IndexFn<'a> = dyn FnMut(&mut fs_settings::ProfileSet<u64>) -> Result<(), Coded<fs_settings::ProfileCode>>
    + 'a;

pub use caller::current_os_user;
pub use client::{ClientTimes, RpcInstanceAdmin, RpcSettingsStore, Watching, unreachable_at};
pub use ensure::{SERVICE_PROGRAM, ensure_service, ensure_service_with, find_service_program};
pub use problem::{from_problem, to_problem};
pub use service::SettingsService;
