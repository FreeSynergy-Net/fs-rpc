# fs-layouts-rpc (probe)

The descriptions of the frontends (layouts, overlays, locks) live in ONE database owned by ONE process, `fs-forms serve`. The host
and every other program reach them through the contract `layouts`. This crate holds both ends of that wire:

- `RpcLayouts` - the ports `LayoutRead`, `LayoutWrite`, `PackageLayouts` and `OverlayWrite` of `fs-forms-layout` over RPC. Starts
  without a service (every call says `unreachable{socket}`), mends a lost link on the next call, `watch` for the live update.
- `RpcLayoutAdmin` - the port `LayoutAdmin` for the administrator.
- `LayoutService` - the contract over any port (the program fs-forms plugs in its database store). Files, `repair` and `bind`
  answer `unsupported{what}`; `admin` on the person service too.
- `ensure_service` - one service per person.

Errors are codes with parameters (`layout-error-<code>`), never sentences. Limits are settings the program passes in with their
source (`ServiceLimits`, `ClientTimes`).

Status: probe. The contracts `layouts` (draft 0.1) and `bind` are the owner files `schema/layouts.capnp` and `schema/bind.capnp` of
fs-interfaces v0.0.28, with their generated code and their final schema ids (`fs_interfaces::layouts_capnp`, `bind_capnp`, re-exported
here). The provisional copies that this crate carried before (and their provisional ids) are gone. Questions to the owner of the
contract are listed at the head of the schema (appended fields).
Written with Claude (Anthropic).

## Edge: where this crate lives (D4, c0 18.11.: yes)

The crate is here only as a probe. Before any release it moves to the program repo fs-forms (decision D4); **fs-rpc is never tagged
with this crate** (fs-rpc -> fs-forms git -> fs-rpc would be a repo loop). The actor of every call is the identity of the
connection, never a field of the client: `admin(by)` takes `by` as a claim, hands it to `LayoutService::claimed_by` (the host's
log) and ignores it otherwise.

## Edge: who is the actor (fs-rpc `Caller`)

fs-rpc reads the caller of EVERY connection from the system (`SO_PEERCRED` on Unix, the pipe client's token on Windows) and builds the
capability of each connection from it (`serve_callers`, `run_until_signal_callers`, `PerConnection`). `LayoutService::per_connection`
is that factory: the actor of a connection is `LayoutService::actors(&Caller)` and nothing else (`Caller::id`; the name is for
display only). Never a field of the client: `admin(by)` takes `by` as a claim, hands it with the caller to
`LayoutService::claimed_by` (the host's log) and ignores it otherwise. A caller the host's mapping does not know gets a capability
that answers `unknown{what=caller}` on every call; a connection the system could not name is dropped and told to
`LayoutService::refused` (code `caller-credentials`). Administration (`admin`) is refused to every connection whose actor is not the
person (`unsupported{what=on-behalf}`, K78 c). The `Hub` shares ONLY the listeners and their counters between the connections of
one store; no capability that knows a caller lives in shared state (fs-rpc DESIGN H2). Limits of the caller (fs-rpc DESIGN "Caller:
honest limits"): the system names the OS user, not the program; a bot is told apart by its key, later.

The choice of a socket is NOT a security boundary: every process of the same OS user can open any socket and gets the actor the
host's mapping gives that user. K78 (c) acts only against connections the host maps to a bot actor. Peer credentials name the OS
user, not the program; what is needed to tell a bot apart is a bot key per connection or an OS user of its own. No bot socket is
offered productively until the bot key exists.

## Edge: one hub per store

Several sockets over ONE store must share ONE `Hub` (`LayoutService::per_connection(&hub)`): what is written through socket A is
heard by the watchers of socket B. The limit
`watchers_max` counts the watchers of the whole hub.

## Open before release

These stay open on purpose; they belong to fs-forms / the contract owner, not to this probe:

- (b) the UUIDs of layouts and overlays come from the store (fs-forms); then the uuid wrapper of the tests and wire goes.
- (d) the words of the 8 remaining enums (FilterOp, FilterCond, Origin, OverlayState, Position, Cause, Field, LogAction) move into
  `Words` of `fs-forms-layout`.
- (f) `PAGE_MAX` (1000, fixed in `fs-forms-layout`) becomes a setting there.
- (g) `sha2` in ONE version in the fs-forms tree (fs-forms-layout on 0.11; today 0.10 + 0.11 side by side).
- (e) contract 0.3: the flags `hasValue/hasLow/hasHigh` etc. are a PROPOSAL; the contract owner decides the form.

## Auf Deutsch

Die Beschreibungen der Frontends liegen in EINER Datenbank eines Prozesses (`fs-forms serve`). Dieses Crate enthaelt beide Seiten
der Leitung: Client (`RpcLayouts`, `RpcLayoutAdmin`), Server-Geruest (`LayoutService`) und `ensure_service`. Fehler sind Codes mit
Parametern, Grenzen sind Einstellungen. Stand: Probe. Die Wahl des Sockets ist keine Sicherheitsgrenze (jeder Prozess desselben OS-Nutzers kann den Personen-Socket oeffnen); mehrere Sockets eines Speichers teilen EINEN `Hub`. Offen vor Release: (b), (d), (f), (g), (e) siehe oben.
