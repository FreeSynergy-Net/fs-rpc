# FreeSynergy RPC — the one way programs talk to each other

**This is a building block. It is why every program's server only translates its own methods and every client behaves the same.**

FreeSynergy RPC is the shared server and client side of every interface contract in [fs-interfaces](https://github.com/FreeSynergy/fs-interfaces). Each program serves its contract the same way: one socket per program (a Unix socket, a named pipe on Windows), one two-party Cap'n Proto RPC system per connection, the common envelope on the wire, errors in the RFC 9457 form, cursor pages and a change feed. All of that lives here, once.

What is different here: it holds only the runtime. The contracts (schemas and generated code) stay in fs-interfaces; the object store stays in fs-objects. This library sits above both, so the repositories form a graph without a cycle (ADR 0092, accepted).

## What it can do today

Latest tag: `v0.0.2`.

- `catalog::CatalogServer`: answers `TypeCatalog.describe` from a `fs_types::TypeRegistry` (the server half of ADR 0066; it was in fs-types).
- Serve a contract on a local socket (`serve`, `run_until_signal`; a stop while the program still opens its store ends the run in bounded time, `open` is dropped, no socket stays), connect to it (`connect`, `connect_matching` with version handshake and restart of an old server).
- The client half every program shares: `client::Worker` (the thread that owns the connection), `Timeouts`, `Fault`, deadlines per call.
- Errors as code and parameters (`RpcError`, `RpcErrorCode`): a program words them through its MessageFormat 2 files (`message_key()`, `params()`, template in `docs/errors.md`); no English sentence reaches a person from this library.
- `prepare_data_dir`: the data folder private to its user; an open own folder is corrected, a link, a file, a foreign folder is refused (`data-dir-not-private`, parameter `cause`).
- `keep_connected` (`Session`, `Resync`, `Backoff`): reconnect after a lost connection, a restart, an update or a take-over: connect (and start the server), subscribe again, tell the program `Reconnected { attempt, resync_needed }` so it loads anew; pauses 1 s / 2 s / 5 s, every step with a limit, after 30 s the program is told and the loop ends.
- `Heartbeat` (Unix): a thread of its own answers `alive` or `busy` (what, progress) and the tick of the work thread, and runs the program's `SaveOnTerm` hook; `ensure_single` (Linux): the one server per program; a hanging server of this program is saved, ended (SIGTERM, SIGKILL only with proof) and replaced. The heartbeat comes from a thread of its own, never from the work loop; the 2 s of the handshake only detect that the contract is silent, nothing is ended before the watch has lasted 10 s (no heartbeat, or a standing tick without `busy`) or 90 s (no progress), and only after a save attempt; a busy one is never ended; another program is only reported (`server-unresponsive`, `server-busy`).
- `Stuck`: the one description of a silent server for every program (`server-unresponsive`: `pid` as `u32`, `start`, `since`, `exe`, `can-ask`, `choices` (`wait,replace,abort`), `why` in `WHY_TOKENS` (exported at the crate root: the ten tokens, among them `same-program`), `limit-ms`); `Stuck::from_error` reads it back; the program words it.
- `SingleOptions` / `AskPolicy` / `ensure_single_with`: who decides when a silent server was not ended (`no-control`, `not-ending`): by default the person (`can-ask`, `choices` `wait,replace,abort`), not without a surface (`has_surface()`) or with the setting `fs.rpc.single.ask` off. The program reads the setting from fs-settings and passes it: `SingleOptions::new(AskPolicy::from(setting), interactive)`; fs-rpc has no dependency on fs-settings.
- RFC 9457 problems as RPC errors with typed extensions, pages cut by weight, the change feed, stop from outside.
- Written for Unix (socket, signals) and Windows (named pipe with a protected descriptor).

## Who is calling (Caller)

- The caller of a connection comes from the system only (`SO_PEERCRED` on Unix, the pipe client's token on Windows), never from a field the client sends, an argument or the environment. The actor of a request is `Caller::id()`; `Caller::name()` is for display only and is `None` for users that only NSS or LDAP know (the name is read from `/etc/passwd`).
- Today the transport lets only the user who runs the server connect, so the caller is always the server's own user: the API is built per connection and tells callers apart as soon as a transport admits other users, but with the current transport it cannot yet tell two apart.
- A program MUST NOT put a `Caller`-bound capability (or anything it reaches) into state that other connections can fetch: a capability handed to a second connection would carry the first caller's identity over.

## Lift note (v0.0.3, for callers)

- `prepare_data_dir` answers `Result<PathBuf, DataDirError>` instead of `io::Result<PathBuf>`: a caller with an `io::Result` annotation and no `?` must convert (`io::Error::from(e)`) or use `?`; an open own folder is now corrected, not refused.
- `HandshakeError::CannotRestart` carries `cause` (see the changelog of the error codes).
- Programs that use `Heartbeat` MUST call `tick()` at every turn of their work or window loop.

## What is not there yet

- It pins fs-types v0.0.1, fs-interfaces v0.0.27 and fs-objects v0.0.6 by tag; `Cargo.lock` is made against exactly these tags (one source per library, ADR 0093).
- The limits listed in the DESIGN file of fs-interfaces (history of this code) are still to be closed before 1.0.

## Part of FreeSynergy

FreeSynergy is a family of programs that are free for everyone, forever, and that keep your data yours. The documentation, the manifesto and the overall plan live in [fs-documentation](https://github.com/FreeSynergy/fs-documentation). Written with the help of Claude (Anthropic).

## For developers

- Library crates: `fs-rpc` (`crates/fs-rpc`; it was the module `rpc` of fs-interfaces up to v0.0.25: `fs_interfaces::rpc::X` is now `fs_rpc::X`) and, as a probe not yet released, `fs-starter` (`crates/fs-starter`: the program shell; written with the help of Claude (Anthropic); `fs-conformance` only behind its optional feature `conformance`, never in `default`).
- Dependencies by git tag: `fs-types` v0.0.1, `fs-interfaces` v0.0.27, `fs-objects` v0.0.6 (only `ObjectId` and `Envelope` of the v4 layer, no feature `v5`).
- No code generation here: the generated Cap'n Proto modules (`common_capnp` and so on) come from fs-interfaces; there is no `build.rs` and no schema compiler.
- Use it by git tag: `fs-rpc = { git = "https://github.com/FreeSynergy/fs-rpc", tag = "v0.0.2" }`, and pin the same fs-types and fs-interfaces tags as this library does, or your lock holds two copies of the generated types. `.fs-lock-sources.toml` makes the lock check of fs-release strict.
- Gates: `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings -W clippy::pedantic`, `cargo test --workspace --all-features`.
- Licence: MIT OR Apache-2.0.

## Auf Deutsch

Neu (unveroeffentlicht): ein Stopp waehrend `open` beendet `run_until_signal` in begrenzter Zeit; `Stuck` ist die eine Beschreibung eines schweigenden Servers (Code `server-unresponsive` mit Parametern, `from_error` liest zurueck).

Neu in v0.0.3: Fehler-Codes (18), `prepare_data_dir` mit Typfehler, `keep_connected` (Wiederverbinden mit Voll-Reload-Hinweis), `Heartbeat` und `ensure_single` (haengende Instanz erkennen, retten, beenden; Busy nie; fremdes Programm nur melden).

FreeSynergy RPC ist die gemeinsame Server- und Client-Seite aller Schnittstellenvertraege aus fs-interfaces: ein Socket je Programm (Unix-Socket, unter Windows eine benannte Pipe), ein Cap'n-Proto-RPC-System je Verbindung, Huelle auf dem Draht, Fehler nach RFC 9457, Seiten und Aenderungsstrom. Es war bis v0.0.25 das Modul `rpc` von fs-interfaces (`fs_interfaces::rpc::X` heisst jetzt `fs_rpc::X`). Es enthaelt nur die Laufzeit; Vertraege (fs-interfaces) und Speicher (fs-objects) bleiben getrennt, damit die Repositories keinen Kreis bilden (ADR 0092, akzeptiert). 
