//! The shared server and client side of every contract.
//!
//! Every program serves its contract the same way: a socket per program (a Unix socket, a named pipe on Windows), one two-party RPC
//! system per connection, RFC 9457 problems as RPC errors, cursor pages, a change feed. All of that
//! lives here, once, so a program's server only translates its own methods (ADR 0043 §4). Everything
//! here runs on one thread inside a `tokio::task::LocalSet`, as `capnp-rpc` requires.
//!
//! A consumer ends a subscription by dropping the capability **and** the response it came in: the
//! response keeps its own reference. A `ChangeListener` must not answer `change` with an error: a stream
//! that failed once stays failed, and that ends the feed.
//!
//! The contracts' messages are limited ([`MAX_MESSAGE_BYTES`]); one larger message ends the whole
//! connection with all its capabilities. So a server never answers more than fits: pages are cut by
//! weight ([`Page::of_weighted`]), reads are clamped ([`clamp_read`]), error texts are cut
//! ([`MAX_DETAIL_BYTES`]). An item too large for any message is refused with a cursor past it
//! ([`Problem::next`]), so what is behind it stays readable.
//!
//! A write of more than one step goes through [`run_to_end`], or a client that goes away leaves it half
//! done. A program that stops waits for such writes ([`run_until_signal`], [`FINISH_TIMEOUT`]).
//!
//! Pattern: Facade (declarations).

#![deny(missing_docs)]
#![warn(clippy::pedantic)]

pub mod catalog;
mod cause;
pub mod client;
mod code;
mod control;
mod cursor;
mod datadir;
mod envelope;
mod expected;
mod feed;
mod finish;
mod guarded;
mod handshake;
mod heartbeat;
mod hold;
mod hold_call;
mod hold_in_process;
mod hold_spec;
mod ids;
mod options;
mod page;
mod problem;
mod pump;
mod reconnect;
mod saved;
mod serve;
mod serve_idle;
mod serving;
mod setting_keys;
#[cfg(unix)]
mod single;
#[cfg(not(unix))]
#[path = "single_other.rs"]
mod single;
mod starter;
mod stop;
mod transport;
mod with_form_source;

pub use cause::RestartCause;
pub use code::{Param, RpcError, RpcErrorCode, param_keys as rpc_param_keys};
pub use control::{PROTOCOL, ServerIdentity, stop_server};
pub use cursor::{PlaceParts, decode_place, encode_place};
pub use datadir::{DataDirCause, DataDirError};
pub use envelope::{envelope_weight, read_envelope, write_envelope};
pub use expected::{Expected, HandshakeError, RestartTimes, ServerInfo};
pub use feed::{
    ChangeReceiver, ChangeSender, FEED_CAPACITY, FEED_ENDED_PROBLEM_TYPE, FEED_STALL, Notice,
    change_channel, forward, write_change,
};
pub use finish::{FINISH_TIMEOUT, finish_underway, run_to_end};
pub use guarded::{
    ActorOfConnection, AllBots, FixedPerson, Guarded, LocalPerson, OsUserIsPerson,
    PersonOfConnection, Ports, REVERT_COMMAND, WATCH_COMMAND, WatchHub, id_usable,
};
pub use handshake::{connect_matching, connect_matching_with};
pub use heartbeat::{
    Beat, BeatError, HangTimes, Heartbeat, Hung, Pulse, SaveOnTerm, Verdict, Watch, ask_beat,
    ask_save,
};
pub use hold::{Hold, HoldEvent, HoldObserver, hold, hold_with};
pub use hold_call::{Job, OnAttach, hold_calling};
pub use hold_in_process::hold_in_process;
pub use hold_spec::HoldSpec;
pub use ids::{from_millis, read_object_id, read_uuid, to_millis, write_object_id, write_uuid};
pub use options::{
    FACET_FLAG, LOCAL_OWNER, ServerOptions, default_data_dir, facet_from_args, parse_facet,
    prepare_data_dir,
};
pub use page::{
    DEFAULT_PAGE_SIZE, MAX_INTAKE_BYTES, MAX_ITEM_BYTES, MAX_MESSAGE_BYTES, MAX_PAGE_BYTES, Page,
    check_answer, check_intake, clamp_read,
};
pub use problem::{
    INVALID_REQUEST, MAX_DETAIL_BYTES, MAX_EXTENSIONS, MAX_KEY_BYTES, Problem, TOO_LARGE, capped,
    failed, invalid_request, is_unimplemented, keys as problem_keys, too_large, too_large_by,
    unsupported,
};
pub use reconnect::{
    Backoff, ReconnectEnd, ReconnectTimes, Reconnected, Resync, Session, keep_connected,
};
pub use saved::Saved;
pub use serve::{
    Connection, ReadLimits, connect, connect_with, run_until_signal, run_until_signal_as,
    run_until_signal_callers, run_until_signal_serving, serve, serve_callers, serve_with,
};
pub use serve_idle::{ServeOptions, serve_callers_options, serve_options};
pub use serving::{MODULE_IDLE, Serving};
pub use setting_keys::{MCP_ENABLED_KEY, SINGLE_ASK_KEY};
pub use single::{
    AskPolicy, CHOICE_TOKENS, Choices, Single, SingleError, SingleOptions, SingleTimes, Stuck,
    TakenOver, WHY_TOKENS, ensure_single, ensure_single_with, has_surface, surface_from,
};
#[cfg(all(unix, target_os = "linux"))]
pub use single::{Decision, Image, Own, Peer, Why, decide_takeover};
pub use starter::{CommandStarter, ServerStarter};
pub use stop::stop_self;
pub use transport::{
    BoundSocket, Caller, CallerError, CallerSource, PerConnection, Shared, bind_socket,
    companion_path, default_socket,
};
pub use with_form_source::with_form_source;
