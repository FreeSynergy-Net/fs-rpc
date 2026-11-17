# fs-rpc – Design

The runtime half of the interface contracts: what every program's server and client share (ADR 0043 §4). The topic contracts (schemas, generated code, `provider`) are in `fs-interfaces`; the value objects and `types.capnp` are in `fs-types`; this crate adds no schema. Repo graph (ADR 0093): `fs-types` <- {`fs-interfaces`, `fs-objects`, `fs-jscalendar`} <- `fs-rpc` <- programs. No edge goes upward. Pins of v0.0.2: `fs-types` v0.0.1, `fs-interfaces` v0.0.27, `fs-objects` v0.0.6.

## Structure and pattern map

| Part | Pattern | Why |
|---|---|---|
| `src/lib.rs` | Facade (declarations) | the public names of the former `fs_interfaces::rpc` |
| `src/catalog.rs` | Adapter | `type_catalog::Server` over a `fs_types::TypeRegistry` (moved here from `fs-types`; the registry and the schema stay there) |
| `src/serve.rs`, `transport/` | Adapter, Strategy (`Endpoint`: Unix socket, named pipe) | one RPC system per connection on a local endpoint |
| `src/envelope.rs`, `ids.rs`, `cursor.rs` | Adapter | `fs_objects::Envelope` / `ObjectId` to the `common.capnp` wire form |
| `src/problem/` | Value object (`Problem`), Adapter (`wire`: error text), constants (`keys`) | RFC 9457 problems as RPC errors |
| `src/code/`, `src/cause.rs` | Value objects, Adapter (`from`) | error code + typed parameters for the program's MessageFormat 2 messages (ADR 0057); `RestartCause` types the reason of a failed replacement |
| `src/feed.rs`, `pump.rs` | Observer | change feed with a bounded queue |
| `src/handshake.rs`, `control.rs`, `starter.rs`, `stop.rs`, `finish.rs` | Strategy (`ServerStarter`), Command | version handshake, stop from outside, writes that finish |
| `src/datadir.rs` | Policy + Guard (`decide` over `Meta`), typed error | the data folder is private to its user; corrected (bits only taken away) or refused with a cause |
| `src/reconnect/` | Template method (`keep_connected`) + Strategy (`Session`) + Observer (`Resync`), Value object (`Backoff`) | stay connected over a server restart: connect, subscribe, watch (ping, `ended`), pause, deadline, resync hint. Mechanism only; the comparison with `generation()` is the contract's and the program's |
| `src/heartbeat/` | Active object (own thread, plain line protocol on `<socket>.beat`), State machine (`Watch`), Strategy (`SaveOnTerm`) | tells hanging from busy without the work thread: `alive`/`busy`, tick age of the work thread, `save` on the heartbeat thread |
| `src/single/` | Template method (`ensure_single`), Policy + Guard (`rule::decide`), Command over an `Env` (`ending`), Gateway (`procfs`) | one server per program; a hanging one of this program is saved, ended and replaced, any other is only reported; generalised from fs-tasks 0.0.10 |
| `src/client/` | Command (+ Strategy `Remote`, RAII guard `Reply`), Decorator (`call_limited`) | the client half: thread that owns the `!Send` connection, deadlines, `Fault` |

## Hanging versus busy (heartbeat, `ensure_single`)

The contract's control channel is answered by the work thread, so "long" and "stuck" look the same from outside. The heartbeat is a second channel answered by its own thread (no Cap'n Proto, so no schema change). Hanging = no heartbeat for 10 s, or the heartbeat answers but the work thread's tick stands still for 10 s without `busy`, or no progress for 90 s (a `busy` number that stands still, or `alive` while the contract is silent; 90 s because a self-update may stand still 60 s while loading). `busy` with a moving number is never ended. Ping 2 s. Everything is a field of `HangTimes` / `SingleTimes`.

The ending: look (user, program file by inode or real path, start time, never pid <= 1 or itself) -> `save` (heartbeat) -> `pidfd_open` -> look again -> SIGTERM -> wait 10 s -> only if it still stands, the program HAS a heartbeat channel and none answers: look a third time, SIGKILL. A program without a channel (older, or a window before it has one) gets NO signal at all (nothing could be saved first, a signal would lose work): `server-unresponsive` with `why` = `no-control` ("please close it"). Another program: `why` = `other-program`, nothing is saved or signalled. The socket file is removed only if it is the inode that was seen. Every step goes to `<socket>.takeover.log` (0600). How to build it in (for program authors): (1) start `Heartbeat::start(&socket)` after the server's socket is bound and keep it as long as the program runs; (2) call `heartbeat.tick()` at every turn of the work loop (server) or window loop (frontend): it is MANDATORY, the age counts from `start`, so a program that never ticks is counted as hanging 10 s after its contract falls silent (the tests `rpc_single::a_hanging_server_with_a_heartbeat_is_saved_then_ended` has a child that never ticks); (3) call `busy(what, progress)` around long work with a text key and a growing number, `idle()` after; (4) `on_save(hook)` with a hook that works from shared state. Where the pieces belong: the draft auto-save beat (every few seconds) is the job of fs-sessions (it owns drafts and the session); fs-rpc only offers the hook (`SaveOnTerm`) that the beat channel and the program's stop path both call.

## Generated code (Cap'n Proto)

Checked for v0.0.2: `fs-rpc` needs no code generation of its own. It uses the committed generated modules of `fs-interfaces` (and `fs_types::types_capnp::type_catalog` for the catalog server) (`common_capnp::{control, change, change_listener, subscription, envelope, problem, uuid}`, in the tests also `tasks_capnp`, `texts_capnp`, `places_capnp`, `people_capnp`) through `fs_interfaces::...`, and `capnp-rpc` for the two-party transport. The only change to the moved source was the path (`crate::` to `fs_interfaces::` at 13 places in `src/`). Generated types of two `fs-interfaces` or `fs-types` tags are different Rust types, so `fs-rpc`, `fs-objects` and the program must all use the same tags (checked by one source per library in the lock, `.fs-lock-sources.toml`, `strict`).

## Tests

`tests/catalog_roundtrip.rs` (acceptance test of ADR 0093): the three libraries meet only here. `CatalogServer` and an `Edit` with values of `fs-types` go through the `People` contract over an in-memory pipe (all systems) and a Unix socket (Unix; Windows has the named pipe, tested elsewhere).

`tests/rpc_reconnect.rs`, `tests/rpc_datadir.rs`, `tests/rpc_heartbeat.rs`, `tests/rpc_single.rs` (real child processes that hang: with and without heartbeat, busy moving or standing still, blocked work thread, another program as a copy of the file, a program that ignores SIGTERM); unit tests with made-up facts in `src/single/*_tests.rs`, `src/heartbeat/watch_tests.rs`, `src/datadir_tests.rs`. Each test file lists its mutants in its `//!` head.

## History

The design notes, limits and findings of this code up to v0.0.25 are in the DESIGN file of `fs-interfaces` (sections on `rpc`, hardening, handshake; kept there as record).
