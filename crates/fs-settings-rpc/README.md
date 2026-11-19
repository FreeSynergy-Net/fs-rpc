# fs-settings-rpc (probe)

The settings of a person live in ONE database owned by ONE process, `fs-settings serve`. Every program reaches it through the
contract `settings`. This crate holds both ends of that wire:

- `RpcSettingsStore` - the port `fs_settings::SettingsStore` over RPC. Starts without a service (`Health::Defaults[store-unreachable]`),
  caches per stamp, `update_index` is compare-and-set (3 rounds, then `store-busy`).
- `RpcInstanceAdmin` - the port `InstanceStore` for the administrator (`SettingsHub.admin(by)`).
- `SettingsService<S, I>` - the contract over any port (the program fs-settings plugs in its database).
- `ensure_service` - one service per person.
- `ActorOfConnection` (`FailClosed` by default, `OsUserIsPerson`) - who is a person; the service leads every way a bot can reach through `fs_settings::Surface` (`human_only` keys do not exist for a bot). `RpcInstanceAdmin::instance_log_page` guards the log read against a livelock (`history-cut`, `history-stuck`).

Caller identity: the connection (fs-rpc lets in the same OS user only); time and OS user of the instance log are the service's.
Status: probe; the contract `settings` (0.2) comes from `fs-interfaces` v0.0.28 (`fs_interfaces::settings_capnp`, re-exported), no copy here. Written with Claude (Anthropic).

## Refused callers (stable code)

A connection whose caller the system names with an unusable id (empty, or not only letters, digits, `-`, `_`, `.`) is refused: every call
answers `caller-unknown` (a client sees `store-broken{cause=caller-unknown}`) and the host's log hears it (`refused_note`). The actor is never
the name. A connection the system cannot name at all is `caller-credentials` (fs-rpc).

## Auf Deutsch

Die Einstellungen einer Person liegen in EINER Datenbank eines Prozesses. Dieses Crate enthaelt beide Seiten der Leitung:
Client (`RpcSettingsStore`, `RpcInstanceAdmin`), Server-Geruest (`SettingsService`) und `ensure_service`. Aufrufer = Verbindung;
Zeit und OS-Nutzer im Protokoll setzt der Dienst. Stand: Probe.
