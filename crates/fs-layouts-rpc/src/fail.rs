//! The one failure type inside a handler or a client call: a layout code with parameters, or an RPC error that carries its own
//! problem. `?` converts both; the edge turns it into a `capnp::Error` (server) or a `Coded` (client).
//!
//! Pattern: Adapter (error conversions).

use fs_forms_layout::{Coded, LayoutCode};

use crate::problem::{err, meaning};

/// A failure of one call.
#[derive(Debug)]
pub(crate) enum Fail {
    /// A layout code.
    Coded(Coded<LayoutCode>),
    /// An RPC error (a problem of the peer, or the connection).
    Rpc(capnp::Error),
}

impl From<Coded<LayoutCode>> for Fail {
    fn from(c: Coded<LayoutCode>) -> Self {
        Self::Coded(c)
    }
}

impl From<capnp::Error> for Fail {
    fn from(e: capnp::Error) -> Self {
        Self::Rpc(e)
    }
}

impl From<std::str::Utf8Error> for Fail {
    fn from(_: std::str::Utf8Error) -> Self {
        Self::Coded(crate::wire::text::bad("text"))
    }
}

impl From<Fail> for capnp::Error {
    /// The server edge: a code becomes its problem; an RPC error passes unchanged (its problem is not lost).
    fn from(f: Fail) -> Self {
        match f {
            Fail::Coded(c) => err(&c),
            Fail::Rpc(e) => e,
        }
    }
}

impl Fail {
    /// The client edge: a problem of the peer means its code; a lost connection is `None` (the caller says `unreachable`).
    pub(crate) fn into_coded(self) -> Result<Coded<LayoutCode>, capnp::Error> {
        match self {
            Self::Coded(c) => Ok(c),
            Self::Rpc(e) => match fs_rpc::Problem::from_error(&e) {
                Some(p) => Ok(meaning(&p)),
                None => Err(e),
            },
        }
    }
}

/// One handler or call.
pub(crate) type H<T = ()> = Result<T, Fail>;
