//! The error as a program reads it: code and parameters.
//!
//! Pattern: Value object. `Display` is the technical text for logs and developers, never for a person: no sentence, so no
//! language. The person's text is the program's `MessageFormat` 2 message under [`RpcError::message_key`], filled with
//! [`RpcError::params`].

use std::fmt;

use super::{Param, RpcErrorCode};

/// An error of the RPC runtime as code and typed parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RpcError {
    code: RpcErrorCode,
    params: Vec<(String, Param)>,
}

impl RpcError {
    /// An error of `code` without parameters.
    #[must_use]
    pub fn new(code: RpcErrorCode) -> Self {
        Self {
            code,
            params: Vec::new(),
        }
    }

    /// The code.
    #[must_use]
    pub fn code(&self) -> RpcErrorCode {
        self.code
    }

    /// The key of the program's `MessageFormat` 2 message (`rpc-error-no-server`): the same for a code, always.
    #[must_use]
    pub fn message_key(&self) -> &'static str {
        self.code.message_key()
    }

    /// The parameters in order, name and value. A name appears once, unless it comes from a repeated problem extension.
    pub fn params(&self) -> impl Iterator<Item = (&str, &Param)> {
        self.params.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The first parameter named `key`.
    #[must_use]
    pub fn param(&self, key: &str) -> Option<&Param> {
        self.params().find_map(|(k, v)| (k == key).then_some(v))
    }

    /// This error with a text parameter (the program adds what the library cannot know, for example the socket).
    #[must_use]
    pub fn with_text(mut self, key: &str, value: impl Into<String>) -> Self {
        self.params
            .push((key.to_owned(), Param::Text(value.into())));
        self
    }

    /// This error with a number parameter.
    #[must_use]
    pub fn with_number(mut self, key: &str, value: u64) -> Self {
        self.params.push((key.to_owned(), Param::Number(value)));
        self
    }
}

impl fmt::Display for RpcError {
    /// `rpc.error: no-server (socket=/run/x.sock, server=people)`: for logs, not for a person.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rpc.error: {}", self.code)?;
        let mut first = true;
        for (key, value) in self.params() {
            f.write_str(if first { " (" } else { ", " })?;
            first = false;
            write!(f, "{key}={value}")?;
        }
        if !first {
            f.write_str(")")?;
        }
        Ok(())
    }
}

impl std::error::Error for RpcError {}
