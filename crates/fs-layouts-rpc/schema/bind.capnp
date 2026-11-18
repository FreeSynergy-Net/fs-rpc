# SKETCH (H1, Teil G) contract for [CLI-ABDECKUNG]: OWN file `schema/bind.capnp` (interface `Bind`, owner = this H1 draft; its
# ordinals are set HERE). Reached by ONE navigation method of the root `Layouts` (F1b `trait-runde/forms-f1b/skizzen/e_layouts.capnp`,
# owner F1b, c0 question 59 = JA): `bind @31 () -> (bind :HostBind.Bind)` — ordinal SET BY F1b (was a placeholder @25 here, which
# collided with F1b `installFile @25`; c0 17.11. rule "Vergabe von Ordnungszahlen": only the owner of an interface sets its ordinals).
# fs-cli (ENTWURF-conformance-v3-cli-abdeckung.md §1.2) turns a parameterless navigation method into a command group, so the generated
# words are `fs-forms bind plan|place|choose|check` - no CLI code in fs-forms (question 80/89). Served by `fs-forms serve` for the asking
# person: the service gathers live provider facts and runs fs-forms-bind (pure library, same code as the host).
# Errors as RFC 9457 `Common.Problem` (F1 O3, common.capnp), codes `bind-error-*`, `accepts-error-*`, `layout-error-*`, `records-error-*`.

@0xd1a5c3e7b9f20468;  # PROVISIONAL id of the probe

using Common = import "common.capnp";

interface Bind {
  # fs-forms bind plan --extends <AppStream-Id> --frontend <FrontendId> [--style <StyleId>]
  # Builds the plan with the server's live provider facts for the asking person; returns the report (no window).
  plan  @0 (extends :Text, frontend :Text, style :Text) -> (report :PlanReport);

  # fs-forms bind place --extends .. --frontend .. --page board --place main
  place @1 (extends :Text, frontend :Text, page :Text, place :Text) -> (report :PlaceReport);

  # fs-forms bind choose --basic email --target "" --role input   (asks the catalog; answers the block name, H-K2 by hand)
  choose @2 (basic :Text, target :Text, role :Text) -> (block :Text);

  # fs-forms bind check --extends .. --frontend ..   (PlanSuite `check_plan` on the live plan; exit != 0 when a rule fails)
  check @3 (extends :Text, frontend :Text) -> (verdicts :List(Verdict));
}

struct PlanReport {
  state     @0 :Text;                 # ready | partial | refused
  stamp     @1 :UInt64;
  rev       @2 :UInt64;
  style     @3 :Text;
  places    @4 :List(PlaceReport);
  notices   @5 :List(Common.Problem);        # Common.Problem from common.capnp (O3)
  menu      @6 :List(Text);           # MenuPlan::outline
}
struct PlaceReport {
  page      @0 :Text;
  place     @1 :Text;
  block     @2 :Text;
  ready     @3 :Bool;
  locks     @4 :List(Common.Problem);        # each `bind-error-locked{part, cause, cause-*}`
  fields    @5 :List(FieldChoice);
}
struct FieldChoice { slot @0 :Text; nth @1 :UInt32; path @2 :Text; role @3 :Text; block @4 :Text; schemes @5 :List(Text); }
struct Verdict { rule @0 :Text; pass @1 :Bool; code @2 :Text; }
# Common.Problem: imported from common.capnp (not repeated here).
