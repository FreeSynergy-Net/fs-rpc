//! The client: the ports `LayoutRead`, `LayoutWrite`, `PackageLayouts` and `OverlayWrite` over the contract `layouts`
//! (`RpcLayouts`), `LayoutAdmin` for the administrator (`RpcLayoutAdmin`), and `watch` outside the port.
//!
//! Pattern: Facade (declarations only); the type is in `rpc`, one port per file.

mod admin;
mod ctx;
mod overlay;
mod read;
mod rpc;
mod watch;
mod write;

pub use admin::RpcLayoutAdmin;
pub use ctx::ClientTimes;
pub use rpc::RpcLayouts;
pub use watch::Watching;
