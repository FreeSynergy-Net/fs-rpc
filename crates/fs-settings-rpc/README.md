# fs-settings-rpc (probe)

The settings of a person live in ONE database owned by ONE process, `fs-settings serve`. Every program reaches it through the
contract `settings`. This crate holds both ends of that wire:

- `RpcSettingsStore` - the port `fs_settings::SettingsStore` over RPC. Starts without a service (`Health::Defaults[store-unreachable]`),
  caches per stamp, `update_index` is compare-and-set (3 rounds, then `store-busy`).
- `RpcInstanceAdmin` - the port `InstanceStore` for the administrator (`SettingsHub.admin(by)`).
- `SettingsService<S, I>` - the contract over any port (the program fs-settings plugs in its database).
- `ensure_service` - one service per person.

Caller identity: the connection (fs-rpc lets in the same OS user only); time and OS user of the instance log are the service's.
Status: probe, contract draft 0.2 compiled in the crate until fs-interfaces 0.0.28 carries it. Written with Claude (Anthropic).

## Auf Deutsch

Die Einstellungen einer Person liegen in EINER Datenbank eines Prozesses. Dieses Crate enthaelt beide Seiten der Leitung:
Client (`RpcSettingsStore`, `RpcInstanceAdmin`), Server-Geruest (`SettingsService`) und `ensure_service`. Aufrufer = Verbindung;
Zeit und OS-Nutzer im Protokoll setzt der Dienst. Stand: Probe.
