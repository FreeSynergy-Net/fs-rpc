//! One server per program, and no hanging one: `ensure_single` gives the connection to the server that fits, or says why not.
//!
//! Pattern: Template method (handshake -> look at a silent server -> end it -> handshake again) around `connect_matching` (the
//! version handshake, which starts the server through the program's `ServerStarter`) + Policy/Guard (`rule::decide`) +
//! Active object (the program's `Heartbeat` tells "busy" from "hanging"). Generalised from fs-tasks 0.0.10 for every program.
//!
//! A server that holds the socket and does not answer `Control.version` within the call limit is looked at. Hanging means: no
//! heartbeat for 10 s, or the work thread's tick stands still for 10 s without `busy`, or no progress for 90 s (a program that says "busy" with a number that moves is never ended). Only a
//! process that is provably this program of this user is ended: the process is found through the socket (`SO_PEERCRED`), its user
//! and program file (`/proc/<pid>/exe`: the same file, or the same real path after an update; the name alone never counts) and
//! its start time are checked before and after a `pidfd` is taken; it is asked to save first (the heartbeat's `save`), then
//! gets SIGTERM, and SIGKILL only if it still stands and no heartbeat answers. Another program is only reported (code
//! `server-unresponsive`, parameters `pid`, `why`, `exe`): no yes-button, never ended. Linux only; elsewhere the failure says
//! `unsupported`. The limits can be set ([`SingleTimes`]); nothing waits without one.
//!
//! Who decides when it was not ended (`no-control`, `not-ending`): by default the person, first ([`SingleOptions`]): the failure then has
//! `can_ask` true and the `choices` `wait`, `replace`, `abort`. Without a surface (headless, CI, a service) or with the setting
//! `rpc.single.ask` off, `can_ask` is false and the program decides. `ensure_single_with` takes the options; `ensure_single` the default.

mod ask;
#[cfg(test)]
mod ask_tests;
#[cfg(target_os = "linux")]
mod ending;
#[cfg(all(test, target_os = "linux"))]
mod ending_fake;
#[cfg(all(test, target_os = "linux"))]
mod ending_tests;
mod error;
#[cfg(target_os = "linux")]
mod procfs;
#[cfg(all(test, target_os = "linux"))]
mod procfs_tests;
#[cfg(target_os = "linux")]
mod rule;
#[cfg(all(test, target_os = "linux"))]
mod rule_tests;
mod stuck;
#[cfg(test)]
mod stuck_tests;
#[cfg(target_os = "linux")]
mod watching;

mod ensure;

pub use ask::{AskPolicy, SingleOptions, has_surface, surface_from};
pub use ensure::{Single, SingleTimes, ensure_single, ensure_single_with};
pub use error::{CHOICE_TOKENS, Choices, SingleError, Stuck, TakenOver, WHY_TOKENS};
#[cfg(target_os = "linux")]
pub use rule::{Decision, Image, Own, Peer, Why, decide as decide_takeover};
