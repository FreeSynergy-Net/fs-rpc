# FreeSynergy RPC — the one way programs talk to each other

**This is a building block. It is why every program's server only translates its own methods and every client behaves the same.**

FreeSynergy RPC is the shared server and client side of every interface contract in [fs-interfaces](https://github.com/FreeSynergy/fs-interfaces). Each program serves its contract the same way: one socket per program (a Unix socket, a named pipe on Windows), one two-party Cap'n Proto RPC system per connection, the common envelope on the wire, errors in the RFC 9457 form, cursor pages and a change feed. All of that lives here, once.

What is different here: it holds only the runtime. The contracts (schemas and generated code) stay in fs-interfaces; the object store stays in fs-objects. This library sits above both, so the repositories form a graph without a cycle (ADR 0092, accepted).

## What it can do today

Latest tag: `v0.0.2`.

- `catalog::CatalogServer`: answers `TypeCatalog.describe` from a `fs_types::TypeRegistry` (the server half of ADR 0066; it was in fs-types).
- Serve a contract on a local socket (`serve`, `run_until_signal`), connect to it (`connect`, `connect_matching` with version handshake and restart of an old server).
- The client half every program shares: `client::Worker` (the thread that owns the connection), `Timeouts`, `Fault`, deadlines per call.
- RFC 9457 problems as RPC errors with typed extensions, pages cut by weight, the change feed, stop from outside.
- Written for Unix (socket, signals) and Windows (named pipe with a protected descriptor).

## What is not there yet

- It pins fs-types v0.0.1, fs-interfaces v0.0.27 and fs-objects v0.0.6 by tag; `Cargo.lock` is made against exactly these tags (one source per library, ADR 0093).
- The limits listed in the DESIGN file of fs-interfaces (history of this code) are still to be closed before 1.0.

## Part of FreeSynergy

FreeSynergy is a family of programs that are free for everyone, forever, and that keep your data yours. The documentation, the manifesto and the overall plan live in [fs-documentation](https://github.com/FreeSynergy/fs-documentation). Written with the help of Claude (Anthropic).

## For developers

- One library crate: `fs-rpc` (`crates/fs-rpc`). It was the module `rpc` of fs-interfaces up to v0.0.25: `fs_interfaces::rpc::X` is now `fs_rpc::X`.
- Dependencies by git tag: `fs-types` v0.0.1, `fs-interfaces` v0.0.27, `fs-objects` v0.0.6 (only `ObjectId` and `Envelope` of the v4 layer, no feature `v5`).
- No code generation here: the generated Cap'n Proto modules (`common_capnp` and so on) come from fs-interfaces; there is no `build.rs` and no schema compiler.
- Use it by git tag: `fs-rpc = { git = "https://github.com/FreeSynergy/fs-rpc", tag = "v0.0.2" }`, and pin the same fs-types and fs-interfaces tags as this library does, or your lock holds two copies of the generated types. `.fs-lock-sources.toml` makes the lock check of fs-release strict.
- Gates: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::pedantic`, `cargo test --workspace --all-features`.
- Licence: MIT OR Apache-2.0.

## Auf Deutsch

FreeSynergy RPC ist die gemeinsame Server- und Client-Seite aller Schnittstellenvertraege aus fs-interfaces: ein Socket je Programm (Unix-Socket, unter Windows eine benannte Pipe), ein Cap'n-Proto-RPC-System je Verbindung, Huelle auf dem Draht, Fehler nach RFC 9457, Seiten und Aenderungsstrom. Es war bis v0.0.25 das Modul `rpc` von fs-interfaces (`fs_interfaces::rpc::X` heisst jetzt `fs_rpc::X`). Es enthaelt nur die Laufzeit; Vertraege (fs-interfaces) und Speicher (fs-objects) bleiben getrennt, damit die Repositories keinen Kreis bilden (ADR 0092, akzeptiert). 
