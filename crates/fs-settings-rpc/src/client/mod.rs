//! The client: [`RpcSettingsStore`], the port `SettingsStore` over the contract `settings` (a proxy with a cache per stamp),
//! and [`RpcInstanceAdmin`] for the commands of the administrator.
//!
//! Pattern: Adapter (sync port over an async contract) + Proxy (cache per stamp; the store starts WITHOUT a service and says
//! so: `Health::Defaults[store-unreachable]`, empty snapshots, refused writes; it connects later by itself).

mod admin;
mod cache;
mod ctx;
mod port;
mod store;
mod stuck;
mod watch;
mod worker;

pub use admin::RpcInstanceAdmin;
pub use ctx::unreachable_at;
pub use store::RpcSettingsStore;
pub use stuck::LIVELOCK_MAX;
pub use watch::Watching;
pub use worker::ClientTimes;
