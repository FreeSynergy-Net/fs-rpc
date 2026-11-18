//! Whether `ensure_single` lets the person decide about a server it could not end: the setting and the surface.
//!
//! Pattern: Policy object (a value the host fills; the default is the plan) + Decorator on the failure ([`SingleOptions::mark`]
//! adds `can_ask` and `choices` to a [`Stuck`], nothing else). Where the person is asked first: with a surface (a window or a terminal)
//! `no-control` and `not-ending` come back with `can_ask` true and the choices `wait`, `replace` (end the old one), `abort`; without
//! one (headless, CI, a service) the program decides and `can_ask` stays false. The value is a setting of the scaffold
//! (`fs.rpc.single.ask`, a yes/no, default yes; fs-settings, ADR 0100): fs-rpc does not read fs-settings (no dependency, no cycle); the
//! program reads the setting and hands it in with [`AskPolicy::from`] (`bool`) and [`SingleOptions::new`].

use super::stuck::{Choices, Stuck};

/// The reasons after which the person is asked: nothing could be saved first (`no-control`), or the end did not work
/// (`not-ending`). The others are not a question (another program, another user, a heartbeat that still answers).
const ASKED_AFTER: [&str; 2] = ["no-control", "not-ending"];

/// What the setting says: ask the person, or never.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AskPolicy {
    /// Ask the person (the default).
    Ask,
    /// Never ask: the program decides.
    Never,
}

impl From<bool> for AskPolicy {
    /// The yes/no setting `fs.rpc.single.ask`: `true` asks.
    fn from(ask: bool) -> Self {
        if ask { Self::Ask } else { Self::Never }
    }
}

/// How `ensure_single_with` treats a server it could not end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SingleOptions {
    /// The setting (`fs.rpc.single.ask`).
    pub ask: AskPolicy,
    /// Whether there is a surface the person can answer on (a window or a terminal): [`has_surface`], or what the program knows.
    pub interactive: bool,
}

impl SingleOptions {
    /// The setting and whether a person is there.
    #[must_use]
    pub fn new(ask: AskPolicy, interactive: bool) -> Self {
        Self { ask, interactive }
    }

    /// Whether a failure may come back as a question: the setting says ask AND there is a surface.
    #[must_use]
    pub fn asks(&self) -> bool {
        self.ask == AskPolicy::Ask && self.interactive
    }

    /// `stuck` with `can_ask` and the choices when it is a case that is asked about and this may ask; else unchanged.
    #[must_use]
    pub fn mark(&self, mut stuck: Stuck) -> Stuck {
        if self.asks() && ASKED_AFTER.contains(&stuck.why) {
            stuck.can_ask = true;
            stuck.choices = Choices::WaitReplaceAbort;
        }
        stuck
    }
}

impl Default for SingleOptions {
    /// Ask (the plan), where there is a surface ([`has_surface`]).
    fn default() -> Self {
        Self::new(AskPolicy::Ask, has_surface())
    }
}

/// The detection behind [`has_surface`], from the facts: a terminal on standard input, or a display name.
#[must_use]
pub fn surface_from(terminal: bool, display: Option<&str>, wayland: Option<&str>) -> bool {
    let named = |v: Option<&str>| v.is_some_and(|v| !v.is_empty());
    terminal || named(display) || named(wayland)
}

/// Whether a person can be asked here: standard input is a terminal, or `DISPLAY` / `WAYLAND_DISPLAY` names a display. Where
/// this does not fit (a window program on Windows, a service with a terminal) the program says it with [`SingleOptions::new`].
#[must_use]
pub fn has_surface() -> bool {
    use std::io::IsTerminal;
    let var = |k: &str| std::env::var(k).ok();
    surface_from(
        std::io::stdin().is_terminal(),
        var("DISPLAY").as_deref(),
        var("WAYLAND_DISPLAY").as_deref(),
    )
}
