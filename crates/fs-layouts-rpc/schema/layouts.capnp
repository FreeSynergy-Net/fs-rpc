# DRAFT 0.1 (trait-delta forms F1b, Opus 2026-11-17) for fs-interfaces `schema/layouts.capnp` (capability `layouts`, owner fs-forms).
# STATUS: DRAFT (ADR 0045). Typed, no JSON, no TOML on the wire (ADR 0086): files are parsed in the CLI process (fs-forms-exchange)
# and travel as `LayoutDoc`. Errors: `Common.Problem`, `type` = "layout-error-<code>", parameters as `extensions` (fs-rpc
# `Problem::with_ext`; codes = fs-forms-layout `LayoutCode`). Evolution: only append, never renumber. Not compiled.
# capabilities.txt (append): `layouts | Layouts | planned | Frontend descriptions, overlays and locks (fs-forms, ADR 0096)`
@0xbf2feddf75510ad3;  # PROVISIONAL id of the probe (random, top bit set); the owner run that adds the file to fs-interfaces sets the final one

using Common = import "common.capnp";
using HostBind = import "bind.capnp";   # host contract (H1); only for the navigation method `bind @31`

# PROVISIONAL (probe forms-f2-adapter): a copy of the DRAFT 0.1 sketch (trait-runde/forms-f1b/skizzen/e_layouts.capnp); the owner file lands in fs-interfaces.

# ---- the description (= fs-forms-layout `LayoutDoc`; `elem` "" = not given, format 1 only) ----
struct Range { capability @0 :Text; minMajor @1 :UInt32; minMinor @2 :UInt32; belowMajor @3 :UInt32; belowMinor @4 :UInt32;
              text @5 :Text; }   # PROBE F2 (question to the owner): `text` = the range as written ("name >=a.b <c.d"); a description may carry a range that
                                 # does not parse (`range-invalid` is the SERVER's answer), so the wire must carry the text. Empty = read the fields.
struct Requires { elem @0 :Text; range @1 :Range; }
struct Source { elem @0 :Text; n @1 :UInt32; range @2 :Range; dataType @3 :Text; scope @4 :Text; }      # scope "" | all | selected-list
struct MenuNode {
  elem @0 :Text; kind @1 :Text; labelKey @2 :Text; standard @3 :Text; source @4 :UInt32; command @5 :Text;
  onWhat @6 :Text; shortcut @7 :Text; view @8 :Text; children @9 :List(MenuNode);
}
struct Scheme { elem @0 :Text; scheme @1 :Text; }
struct Binding {
  elem @0 :Text; slot @1 :Text; nth @2 :UInt32;
  from @3 :Text;                      # records | field | grouping | order | command | lock | condition
  path @4 :Text; role @5 :Text; name @6 :Text;
  hasSchemes @7 :Bool; schemes @8 :List(Scheme);   # hasSchemes=false = no list given (≠ empty list: `schemes-empty`)
}
struct Block { elem @0 :Text; place @1 :Text; block @2 :Text; source @3 :UInt32; titleKey @4 :Text; textKey @5 :Text; binds @6 :List(Binding); }
struct Page { elem @0 :Text; page @1 :Text; frame @2 :Text; navigation @3 :Text; blocks @4 :List(Block); }
struct Action { elem @0 :Text; n @1 :UInt32; trigger @2 :Text; target @3 :Text; source @4 :UInt32; command @5 :Text; workflow @6 :Text; confirm @7 :Text; }
struct Order { elem @0 :Text; by @1 :Text; name @2 :Text; descending @3 :Bool; }
struct FilterNode { elem @0 :Text; op @1 :Text; cond @2 :Text; path @3 :Text; value @4 :Text; low @5 :Text; high @6 :Text;
                    values @7 :List(Scheme); children @8 :List(FilterNode);
                    hasValue @9 :Bool; hasLow @10 :Bool; hasHigh @11 :Bool; }   # PROPOSAL contract 0.3 (c0 18.11.): "not given" is typed, never the empty string; value/low/high are meaningful only with their flag   # values reuse (elem, text)
struct View { elem @0 :Text; view @1 :Text; owner @2 :Text; grouping @3 :Text; orders @4 :List(Order); hasFilter @5 :Bool;
              filter @6 :FilterNode; searchKind @7 :Text; }
struct Message { elem @0 :Text; key @1 :Text; lang @2 :Text; message @3 :Text; }
struct LayoutDoc {
  format @0 :UInt32; extends @1 :Text; frontend @2 :Text; exec @3 :Text; titleKey @4 :Text; style @5 :Text;
  requires @6 :List(Requires); sources @7 :List(Source); menu @8 :List(MenuNode); pages @9 :List(Page);
  actions @10 :List(Action); views @11 :List(View); messages @12 :List(Message);
}

# ---- overlays (16.8) ----
struct Value { union { none @0 :Void; text @1 :Text; int @2 :Int64; bool @3 :Bool; } }
struct Op {
  seq @0 :UInt32; op @1 :Text; targetKind @2 :Text; targetElem @3 :Text; field @4 :Text; value @5 :Value; expect @6 :Value;
  anchorElem @7 :Text; position @8 :Text; newElem @9 :Text; author @10 :Text; actor @11 :Text; at @12 :Common.Timestamp;
}
struct Overlay {
  id @0 :Common.Uuid; layout @1 :Common.Uuid; scope @2 :Text; owner @3 :Text; titleKey @4 :Text; lang @5 :Text;
  author @6 :Text; actor @7 :Text; basePackage @8 :Text; baseExtends @9 :Text; baseFrontend @10 :Text; baseVersion @11 :Text;
  state @12 :Text; rev @13 :UInt64; ops @14 :List(Op);
}
struct OverlayDoc { format @0 :UInt32; package @1 :Text; extends @2 :Text; frontend @3 :Text; baseVersion @4 :Text; lang @5 :Text;
                    titleKey @6 :Text; ops @7 :List(Op); }
struct Conflict { overlay @0 :Common.Uuid; seq @1 :UInt32; baseVersion @2 :Text; cause @3 :Text; found @4 :Value;
                  at @5 :Common.Timestamp; resolution @6 :Text; resolvedBy @7 :Text; resolvedAt @8 :Common.Timestamp; }
struct ElemLock { kind @0 :Text; elem @1 :Text; reason @2 :Text; lockedBy @3 :Text; lockedAt @4 :Common.Timestamp; }  # kind "" = whole layout

# ---- answers ----
struct Summary {
  id @0 :Common.Uuid; extends @1 :Text; frontend @2 :Text; origin @3 :Text; package @4 :Text; packageVersion @5 :Text;
  owner @6 :Text; rev @7 :UInt64; updated @8 :Common.Timestamp; deleted @9 :Common.Timestamp; isDeleted @10 :Bool;
  overlays @11 :UInt32; openConflicts @12 :UInt32;
}
struct Provenance { kind @0 :Text; elem @1 :Text; layer @2 :Text; overlay @3 :Common.Uuid; }   # layer original|instance|person
struct Resolved {
  summary @0 :Summary; layout @1 :LayoutDoc; stamp @2 :UInt64; layers @3 :List(Overlay);   # layers without ops (ops via `overlay`)
  conflicts @4 :List(Conflict); locks @5 :List(ElemLock); provenance @6 :List(Provenance); problems @7 :List(Common.Problem);
  effectiveDigest @8 :Data;                                     # sha256 of the effective layout; host rebuilds only on change
  warnings  @9 :List(Common.Problem);                           # not blocking (provider-unchecked); `problems` = blocking (c0 18.11.)
}
struct Report { hasLayout @0 :Bool; layout @1 :Common.Uuid; rev @2 :UInt64; problems @3 :List(Common.Problem);
                warnings @4 :List(Common.Problem); conflicts @5 :List(Conflict);
                hasRev @6 :Bool; }   # PROBE F2 (question to the owner): `rev` 0 is a real counter, so absence needs its own flag
struct Health { union { ok @0 :Void; defaults @1 :List(Common.Problem); } }
struct LogEntry {
  id @0 :Common.Uuid; at @1 :Common.Timestamp; by @2 :Text; author @3 :Text; osUser @4 :Text; action @5 :Text;
  layout @6 :Common.Uuid; overlay @7 :Common.Uuid; elem @8 :Text; revBefore @9 :UInt64; revAfter @10 :UInt64;
  package @11 :Text; version @12 :Text; count @13 :UInt32; reason @14 :Text;
  hasRevBefore @15 :Bool; hasRevAfter @16 :Bool; hasCount @17 :Bool;   # PROBE F2 (question to the owner): 0 is a real value, absence needs a flag
}

# ---- the contract; every method reachable by a CLI word (rule [CLI-ABDECKUNG], f_cli.rs) ----
interface Layouts {
  capabilities  @0 () -> (capabilities :Common.CapabilityList);
  supportReport @1 () -> (report :Common.SupportReport);                    # provider rule (ADR 0045)
  stamp         @2 () -> (stamp :UInt64);
  health        @3 () -> (health :Health);
  list          @4 (extends :Text, origin :Text, package :Text, withDeleted :Bool, after :Common.Uuid, size :UInt32)
                   -> (items :List(Summary), hasNext :Bool, next :Common.Uuid);
  original      @5 (id :Common.Uuid) -> (found :Bool, layout :LayoutDoc, summary :Summary);
  resolve       @6 (extends :Text, frontend :Text) -> (found :Bool, resolved :Resolved);
  create        @7 (doc :LayoutDoc, author :Text) -> (report :Report);
  import        @8 (doc :LayoutDoc, dryRun :Bool, author :Text) -> (report :Report);
  save          @9 (id :Common.Uuid, expectedRev :UInt64, doc :LayoutDoc, author :Text) -> (report :Report);
  delete        @10 (id :Common.Uuid, expectedRev :UInt64, author :Text) -> ();
  install       @11 (doc :LayoutDoc, package :Text, version :Text, dryRun :Bool, author :Text) -> (report :Report);
  uninstall     @12 (package :Text, author :Text) -> (reports :List(Report));
  overlays      @13 (layout :Common.Uuid) -> (overlays :List(Overlay));
  overlay       @14 (id :Common.Uuid) -> (found :Bool, overlay :Overlay);
  createOverlay @15 (layout :Common.Uuid, lang :Text, titleKey :Text, author :Text) -> (id :Common.Uuid);
  replaceOps    @16 (id :Common.Uuid, expectedRev :UInt64, ops :List(Op), author :Text) -> (report :Report);
  setOverlayState @17 (id :Common.Uuid, expectedRev :UInt64, on :Bool, author :Text) -> (rev :UInt64);
  deleteOverlay @18 (id :Common.Uuid, expectedRev :UInt64, author :Text) -> ();
  importOverlay @19 (doc :OverlayDoc, dryRun :Bool, author :Text) -> (report :Report);
  conflicts     @20 (layout :Common.Uuid, all :Bool, openOnly :Bool) -> (conflicts :List(Conflict));   # all=true: layout ignored
  resolveConflict @21 (overlay :Common.Uuid, seq :UInt32, baseVersion :Text, how :Text, retarget :Text, author :Text) -> (rev :UInt64);
  log           @22 (after :Common.Uuid, size :UInt32) -> (entries :List(LogEntry));      # after all-zero = from the start
  watch         @23 (listener :Listener) -> (subscription :Common.Subscription);
  admin         @24 (by :Text) -> (admin :LayoutAdmin);                       # person service: Problem `unsupported{what=admin}` (v1)
  # ---- exchange files (bytes; parsed/written ONLY in `fs-forms serve` by fs-forms-exchange; the generated CLI passes them as
  # `--content @PATH` / writes `Data` results to a file, conformance_v3 §1.2 `Data` rule). `format` = toml | json. ----
  installFile   @25 (content :Data, name :Text, format :Text, package :Text, version :Text, dryRun :Bool, author :Text) -> (report :Report);
  # The way of fs-packages (c0 question 55): contract over fs-rpc, never another program's CLI. `name` = file name for reports.
  importFile    @26 (content :Data, name :Text, format :Text, dryRun :Bool, author :Text) -> (report :Report);
  exportFile    @27 (id :Common.Uuid, format :Text, effective :Bool, withOwnViews :Bool) -> (content :Data);
  importOverlayFile @28 (content :Data, name :Text, format :Text, dryRun :Bool, author :Text) -> (report :Report);
  exportOverlayFile @29 (id :Common.Uuid, format :Text) -> (content :Data);
  repair        @30 (confirm :Text) -> (movedTo :Text, offer :Text);
  # Only in emergency mode (store-broken/-newer), else Problem `unsupported{what=repair}`. `confirm` must equal the DB file name
  # (a human types it; MCP mark excluded). Renames forms.db (+ -wal/-shm) to forms.db.broken-<yyyymmddThhmmssZ>, opens a new DB.
  bind          @31 () -> (bind :HostBind.Bind);
  # Navigation to the HOST's interface `Bind` (file `bind.capnp`, owner: H1 draft trait-runde/forms-host/; its ordinals are set
  # there). This ordinal is set HERE because `Layouts` belongs to F1b (rule "Vergabe von Ordnungszahlen", ENTWURF §17).
  interface Listener { changed @0 (stamp :UInt64) -> stream; }
}
# `author` = the person the work is for (IRI); the ACTOR is the connection's identity (same OS user; the server's `--actor` until
# fs-access, an AI connects as its own actor). "" = the server's own person. Time and os_user are set by the service, never sent.
interface LayoutAdmin {
  lock    @0 (layout :Common.Uuid, kind :Text, elem :Text, reason :Text) -> (changed :Bool);   # reason empty after trim -> reason-missing
  unlock  @1 (layout :Common.Uuid, kind :Text, elem :Text, reason :Text) -> (changed :Bool);
  createInstanceOverlay @2 (layout :Common.Uuid, lang :Text, titleKey :Text) -> (id :Common.Uuid);
}
