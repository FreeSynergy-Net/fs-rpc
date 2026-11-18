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

## Auf Deutsch

Die Beschreibungen der Frontends liegen in EINER Datenbank eines Prozesses (`fs-forms serve`). Dieses Crate enthaelt beide Seiten
der Leitung: Client (`RpcLayouts`, `RpcLayoutAdmin`), Server-Geruest (`LayoutService`) und `ensure_service`. Fehler sind Codes mit
Parametern, Grenzen sind Einstellungen. Stand: Probe.
