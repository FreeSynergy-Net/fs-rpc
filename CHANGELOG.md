# Changelog

Version = tag.

## 0.0.1

- First version. The code is the module `rpc` of `fs-interfaces` v0.0.25 (38 source files) with its tests, moved here unchanged except for the paths: `crate::common_capnp` and friends became `fs_interfaces::...`, `crate::rpc::X` became `crate::X`, `fs_interfaces::rpc::X` became `fs_rpc::X` in the tests. Reason: the cycle `fs-interfaces` to `fs-objects` to `fs-types` to `fs-interfaces` (ADR 0092 accepted).
- Depends on `fs-interfaces` v0.0.26 and `fs-objects` v0.0.4 by tag.
- Auf Deutsch: Erste Version; der Code ist das Modul `rpc` aus `fs-interfaces` v0.0.25 samt Tests, nur die Pfade wurden angepasst. Abhaengig von `fs-interfaces` v0.0.26 und `fs-objects` v0.0.4 (je per Tag).
