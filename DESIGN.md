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
| `src/client/` | Command (+ Strategy `Remote`, RAII guard `Reply`), Decorator (`call_limited`) | the client half: thread that owns the `!Send` connection, deadlines, `Fault` |

## Generated code (Cap'n Proto)

Checked for v0.0.2: `fs-rpc` needs no code generation of its own. It uses the committed generated modules of `fs-interfaces` (and `fs_types::types_capnp::type_catalog` for the catalog server) (`common_capnp::{control, change, change_listener, subscription, envelope, problem, uuid}`, in the tests also `tasks_capnp`, `texts_capnp`, `places_capnp`, `people_capnp`) through `fs_interfaces::...`, and `capnp-rpc` for the two-party transport. The only change to the moved source was the path (`crate::` to `fs_interfaces::` at 13 places in `src/`). Generated types of two `fs-interfaces` or `fs-types` tags are different Rust types, so `fs-rpc`, `fs-objects` and the program must all use the same tags (checked by one source per library in the lock, `.fs-lock-sources.toml`, `strict`).

## Tests

`tests/catalog_roundtrip.rs` (acceptance test of ADR 0093): the three libraries meet only here. `CatalogServer` and an `Edit` with values of `fs-types` go through the `People` contract over an in-memory pipe (all systems) and a Unix socket (Unix; Windows has the named pipe, tested elsewhere).

## History

The design notes, limits and findings of this code up to v0.0.25 are in the DESIGN file of `fs-interfaces` (sections on `rpc`, hardening, handshake; kept there as record).
