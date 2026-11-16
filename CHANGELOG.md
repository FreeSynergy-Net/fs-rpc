# Changelog

Version = tag.

## 0.0.1

- First version. The code is the module `rpc` of `fs-interfaces` v0.0.25 (38 source files) with its tests, moved here unchanged except for the paths: `crate::common_capnp` and friends became `fs_interfaces::...`, `crate::rpc::X` became `crate::X`, `fs_interfaces::rpc::X` became `fs_rpc::X` in the tests. Reason: the cycle `fs-interfaces` to `fs-objects` to `fs-types` to `fs-interfaces` (ADR 0092 accepted).
- Depends on `fs-interfaces` v0.0.26 (9df19ee323145119e89c003e64982be68e9fc1da) and `fs-objects` v0.0.5 (6cb627253b9fd7005ce98be44f88e2e430d7de0f) by tag.
- Auf Deutsch: Erste Version; der Code ist das Modul `rpc` aus `fs-interfaces` v0.0.25 samt Tests, nur die Pfade wurden angepasst. Abhaengig von `fs-interfaces` v0.0.26 und `fs-objects` v0.0.5 (je per Tag; Koepfe 9df19ee323145119e89c003e64982be68e9fc1da und 6cb627253b9fd7005ce98be44f88e2e430d7de0f).
