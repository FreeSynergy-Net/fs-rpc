# fs-rpc – Design

The runtime half of the interface contracts: what every program's server and client share (ADR 0043 §4). The contracts (schemas, generated code, `provider`, `values`) are in `fs-interfaces`; this crate adds no schema. Repo graph (ADR 0092, draft): `fs-interfaces` <- `fs-objects` <- `fs-rpc` <- programs. No edge goes upward.

## Structure and pattern map

| Part | Pattern | Why |
|---|---|---|
| `src/lib.rs` | Facade (declarations) | the public names of the former `fs_interfaces::rpc` |
| `src/serve.rs`, `transport/` | Adapter, Strategy (`Endpoint`: Unix socket, named pipe) | one RPC system per connection on a local endpoint |
| `src/envelope.rs`, `ids.rs`, `cursor.rs` | Adapter | `fs_objects::Envelope` / `ObjectId` to the `common.capnp` wire form |
| `src/problem/` | Value object (`Problem`), Adapter (`wire`: error text), constants (`keys`) | RFC 9457 problems as RPC errors |
| `src/feed.rs`, `pump.rs` | Observer | change feed with a bounded queue |
| `src/handshake.rs`, `control.rs`, `starter.rs`, `stop.rs`, `finish.rs` | Strategy (`ServerStarter`), Command | version handshake, stop from outside, writes that finish |
| `src/client/` | Command (+ Strategy `Remote`, RAII guard `Reply`), Decorator (`call_limited`) | the client half: thread that owns the `!Send` connection, deadlines, `Fault` |

## Generated code (Cap'n Proto)

Checked 2026-11-16: `fs-rpc` needs no code generation of its own. It uses the committed generated modules of `fs-interfaces` (`common_capnp::{control, change, change_listener, subscription, envelope, problem, uuid}`, in the tests also `files_capnp`, `store_capnp`, `tasks_capnp`, `types_capnp`, `texts_capnp`, `places_capnp`, `comments_capnp`) through `fs_interfaces::...`, and `capnp-rpc` for the two-party transport. The only change to the moved source was the path (`crate::` to `fs_interfaces::` at 13 places in `src/`). Generated types of two `fs-interfaces` tags are different Rust types, so `fs-rpc`, `fs-objects` and the program must all use the same `fs-interfaces` tag (checked by one `fs-types` in the lock).

## History

The design notes, limits and findings of this code up to v0.0.25 are in the DESIGN file of `fs-interfaces` (sections on `rpc`, hardening, handshake; kept there as record).
