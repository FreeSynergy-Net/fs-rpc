//! Error codes with parameters: what a program translates, instead of an English sentence from the library.
//!
//! Pattern: Value objects (`RpcErrorCode`, `Param`, `RpcError`) + Adapters (`from`: every error type of this crate and
//! `capnp::Error` become an `RpcError`). The library says WHAT happened (a stable code in kebab-case) and WITH WHAT (typed
//! parameters); the program words it for the person through its `MessageFormat` 2 messages (ADR 0057), in the language of the
//! window. `docs/errors.md` lists keys, parameters and recommended texts (de, en).

mod error;
mod from;
mod kind;
mod param;
mod token;

pub use error::RpcError;
pub use kind::RpcErrorCode;
pub use param::{Param, param_keys};
