//! The server skeleton: the contract `layouts` over ANY port (`LayoutStore`, plus `LayoutAdmin` for the instance service).
//!
//! Pattern: Facade (declarations only); the service is in `hub`, one handler family per file.

mod admin;
mod hub;
mod listeners;
mod overlay;
mod read;
mod root;
mod write;

pub(crate) use hub::Shared;
pub use hub::{ActorOf, ClaimNote, LayoutService, Layouts, RefusedNote};
pub use listeners::{Hub, Watchers};
