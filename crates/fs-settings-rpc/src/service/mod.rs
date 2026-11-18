//! The server skeleton: the contract `settings` over ANY port (`SettingsStore` + `InstanceStore`).
//!
//! Pattern: Facade + Adapter (contract -> port). One `Store` capability per `open(program)`, bound to that program (the scope
//! check is here, not in the port); port calls are sync and may block, so each runs on `spawn_blocking`. Time and OS user of the
//! log come from the service (`now`, `os_user`), never from the caller. Must run on a `LocalSet` (capnp-rpc), as fs-rpc `serve`.

mod admin;
mod hub;
mod listeners;
mod refused;
mod store;

pub use hub::{ClaimNote, OsUserOf, SettingsCallers, SettingsService};
pub(crate) use hub::{Shared, blocking, err};
