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

Status: probe. The contracts `schema/layouts.capnp` (draft 0.1) and `schema/bind.capnp` are provisional copies of the F1b and H1
sketches, compiled with our own `fs-capnpc` into `src/*_capnp.rs` (command: `capnp-compiler` with `--src-prefix schema`, import path
the folder with `common.capnp` of fs-interfaces v0.0.27; a throwaway helper did it, it is not part of this repo). Questions to the
owner of the contract are listed at the head of the schema (appended fields).
Written with Claude (Anthropic).

## Edge: where this crate lives (D4, c0 18.11.: yes)

The crate is here only as a probe. Before any release it moves to the program repo fs-forms (decision D4); **fs-rpc is never tagged
with this crate** (fs-rpc -> fs-forms git -> fs-rpc would be a repo loop). The actor of every call is the identity of the
connection, never a field of the client: `admin(by)` takes `by` as a claim, hands it to `LayoutService::claimed_by` (the host's
log) and ignores it otherwise.

## Edge: who is the actor (before fs-access)

fs-rpc lets in the same OS user only and hands ONE bootstrap capability to every connection of a socket, so a connection cannot yet
be told from another by its peer credentials. Until fs-access (per-connection identity, bot keys) the actor is therefore bound to
the capability the caller got: `LayoutService::actor_of_connection`, one service per socket (a human socket, a bot socket, each with
its own actor, over ONE store). It is never read from a client field. Administration (`admin`) is refused to every connection whose
actor is not the person (`unsupported{what=on-behalf}`, K78 c). When fs-rpc offers peer credentials per connection, the host fills
`actor_of_connection` from them; the settings service (fs-settings-rpc) must use the same procedure and helper.

The choice of a socket is NOT a security boundary: every process of the same OS user can open the person's socket and is then
"the person". K78 (c) acts only against connections to which the host itself gave a bot actor. Peer credentials alone (same OS
user) would not change this either; what is needed is a bot key per connection or an OS user of its own. No bot socket is offered
productively until fs-rpc has the per-connection factory (peer credentials, later the bot key).

## Edge: one hub per store

Several sockets over ONE store must share ONE `Hub` (`LayoutService::bootstrap_in(&hub)`): what is written through socket A is
heard by the watchers of socket B. `bootstrap()` makes a hub of its own, which is right for a single socket only. The limit
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
