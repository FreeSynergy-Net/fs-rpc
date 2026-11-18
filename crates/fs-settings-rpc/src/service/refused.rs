//! The hub of a connection that was refused: every call answers the code `caller-unknown`.
//!
//! Pattern: Null object (a capability that serves nobody) + Guard (the refusal is the same on every method, so a client cannot find a
//! method that works).

use crate::problem::caller_unknown;
use crate::settings_capnp::settings_hub;

/// Serves nobody: the system did not give a usable id for the caller.
pub(crate) struct Refused;

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
impl settings_hub::Server for Refused {
    async fn open(
        self: std::rc::Rc<Self>,
        _: settings_hub::OpenParams,
        _: settings_hub::OpenResults,
    ) -> Result<(), capnp::Error> {
        Err(caller_unknown())
    }

    async fn manage(
        self: std::rc::Rc<Self>,
        _: settings_hub::ManageParams,
        _: settings_hub::ManageResults,
    ) -> Result<(), capnp::Error> {
        Err(caller_unknown())
    }

    async fn admin(
        self: std::rc::Rc<Self>,
        _: settings_hub::AdminParams,
        _: settings_hub::AdminResults,
    ) -> Result<(), capnp::Error> {
        Err(caller_unknown())
    }

    async fn instance(
        self: std::rc::Rc<Self>,
        _: settings_hub::InstanceParams,
        _: settings_hub::InstanceResults,
    ) -> Result<(), capnp::Error> {
        Err(caller_unknown())
    }
}
