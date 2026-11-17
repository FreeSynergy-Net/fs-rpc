# fs-settings contract - DRAFT 0.2 (2026-11-17)
#
# STATUS: DRAFT (ADR 0045). Owner program: fs-settings (library `fs-settings`). Lives in fs-interfaces (ADR 0045, 0100).
# Replaces draft 0.1 (catalog / layers / profiles on one `Settings` interface): the store lives in a database owned by ONE
# process, `fs-settings serve` (PLAN-speicher-schnittstelle.md section 1, c0 decisions 1-2). Every program reaches it through this contract.
# Consumers so far: none (0 of 2).
#
# Typed, no JSON (ADR 0086): a value is the union `Raw` (flag, whole, text); it needs no `fs-types` type, so this file imports only
# `common.capnp`. Errors: `Common.Problem` with `type` = "setting-error-<code>" and the parameters as `extensions`
# (fs-rpc `Problem::with_ext`; codes = fs-settings `SettingCode`). Times are UTC milliseconds (`Common.Timestamp`).
# Caller identity: the service sets time and OS user, never the caller.
# Evolution (Cap'n Proto): only append, never renumber or remove.
# Compiled with common.capnp by capnp-compiler (see crates/regenerate).

@0x962084020d866277;

using Common = import "common.capnp";

# ---- values (crate fs-settings) ----
struct Raw { union { flag @0 :Bool; whole @1 :Int64; text @2 :Text; } }           # `Raw::{Bool, Int, Str}`
struct Scope { union { general @0 :Void; program @1 :Text; } }                   # `Scope`; program = `ProgramName`
enum Place { profile @0; device @1; }
struct Entry { key @0 :Text; value @1 :Raw; }
struct Lock { reason @0 :Text; lockedBy @1 :Text; lockedAt @2 :Common.Timestamp; }  # UTC ms (R7: was seconds in 0.1)
struct LockEntry { key @0 :Text; value @1 :Raw; lock @2 :Lock; }
struct Snapshot { values @0 :List(Entry); problems @1 :List(Common.Problem); }

struct Health { union { ok @0 :Void; defaults @1 :List(Common.Problem); } }      # `Health`: emergency mode is visible, never silent

# The person's profile index = `ProfileSet<u64>`: base value = the highest id ever issued (`top`), named = id; `active` "" = base.
struct Named { id @0 :UInt64; name @1 :Text; }
struct Index { top @0 :UInt64; named @1 :List(Named); active @2 :Text; }

enum LogAction { set @0; clear @1; lock @2; unlock @3; }
struct LogEntry {
  id @0 :Common.Uuid; at @1 :Common.Timestamp; by @2 :Text; osUser @3 :Text;  # osUser "" = unknown
  action @4 :LogAction; key @5 :Text; reason @6 :Text;
  old @7 :Raw; hasOld @8 :Bool; new @9 :Raw; hasNew @10 :Bool;
}

# ---- bootstrap of `fs-settings serve` ----
interface SettingsHub {
  open   @0 (program :Text) -> (store :Store);
  # The store for one program (`ProgramName`): writes only `general` and `program(<this one>)`, reads the same two scopes; any other
  # scope answers `write-failed{kind=not-own}`. Same OS user only (fs-rpc transport); the name is the caller's word: an API promise,
  # not a security boundary until fs-access (draft O9). "" is refused (`program-invalid`).
  manage @1 () -> (store :Store);
  # Every scope (the program fs-settings itself: "all installed settings in one place", ADR 0100 decision 1).
  admin  @2 (by :Text) -> (admin :Admin);
  # Only on the instance service (`serve --instance`); the person service answers `Common.Problem` unsupported. `by` = actor IRI (mandatory).
  instance @3 () -> (values :Snapshot, locks :List(LockEntry));
  # The instance layer, read only (person service in v1: empty, c0 decision 5).
}

interface Store {                     # = the port `fs_settings::SettingsStore`, call by call
  stamp   @0 () -> (stamp :UInt64);                                        # `settings_rev.rev`; equal = nothing to read again
  health  @1 () -> (health :Health);
  profile @2 (scope :Scope, place :Place, profile :UInt64) -> (snapshot :Snapshot);
  write   @3 (scope :Scope, place :Place, profile :UInt64, key :Text, value :Raw, remove :Bool) -> (stamp :UInt64);
  # One transaction. Refused: `locked{key, reason, by}` (fresh instance state, decision Q5), `read-only{which=person}` in emergency mode.
  clear   @4 (scope :Scope, profile :UInt64) -> (stamp :UInt64);
  index   @5 () -> (index :Index, problems :List(Common.Problem));
  replaceIndex @6 (expected :Index, wanted :Index) -> (index :Index);
  # Compare-and-set for `update_index` (a closure cannot travel): `expected` differs from the stored index -> `index-changed`
  # (the client reads again and retries, at most 3 times, then `store-busy`); `wanted` must be a legal successor of `expected`
  # (`fs_settings::index_step_ok`: ids never reused, base never deleted) else `profile{cause=id-invalid}`.
  watch   @7 (listener :Listener) -> (subscription :Common.Subscription);
  interface Listener { changed @0 (stamp :UInt64) -> stream; }
}

interface Admin {                     # = the port `fs_settings::InstanceStore` for one actor; the service sets time and OS user, never the caller
  set    @0 (key :Text, value :Raw) -> (changed :Bool);                    # changed = false: same value, no log row (idempotent script)
  clear  @1 (key :Text) -> (changed :Bool);                                # locked -> `locked`
  lock   @2 (key :Text, value :Raw, reason :Text) -> (changed :Bool);      # reason empty after trim -> `reason-missing{key}`
  unlock @3 (key :Text, reason :Text) -> (changed :Bool);                  # reason mandatory too (c0 decision 6)
  log    @4 (after :Common.Uuid, size :UInt32) -> (entries :List(LogEntry)); # after all-zero = from the start
}
