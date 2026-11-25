//! G8 for real (Stufe 4, A2): the whole suite `module_v1` level 2 of `fs-module` runs against `Guarded<FakeCore>` over the two
//! connections of one wrapper, and three counter-proofs show that the suite really tests the wrapper.
//!
//! Pattern: Template Method (`run_conformance` is the order of the rules; this file only plugs the subject into `FakeFixture`).
//! Subject: `Guarded<M>` (Decorator over a `ModuleCore`); the rights come from the rules of the module, the wrapper asks them.
//! Counter-proofs: (i) the module breaks a rule (`IgnoreStep`): L2-3 red. (ii) the rules allow everything: rule 11 red (the
//! ban comes from the rules through the wrapper). (iii) the person of the connection is a stranger: 403 and `apply` never ran.
#![allow(
    clippy::many_single_char_names,
    clippy::unused_async_trait_impl,
    clippy::unnecessary_literal_bound
)]
#![allow(clippy::result_large_err)] // `module::Refusal` is the error type of the contract `ModuleCore`

mod guarded_support;

use std::cell::RefCell;
use std::rc::Rc;

use capnp::capability::FromClientHook;
use fs_interfaces::formsource_capnp::form_source;
use fs_interfaces::module::fake::{CREATE as OWNED_CREATE, OwnerOnlyModule, SET as OWNED_SET};
use fs_interfaces::module::{
    Actor, Applied, Ask, Blob, ChangeFeed, ChangePage, ChangesQuery, Context, FeedBreak, Follow,
    HiddenReason, Invocation, ModuleCore, Page, PageQuery, Refusal, RightsRules, StepRef, Verdict,
};
use fs_interfaces::owner::fake::person;
use fs_module::conformance::{Report, run_conformance};
use fs_module::{FakeCore, FakeFixture, Mode};
use fs_rpc::{Caller, Guarded, PerConnection, Ports};
use fs_types::PersonRef;
use guarded_support::client::{invoke, run};

const OWNER: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e01";
const STRANGER: &str = "urn:uuid:018f3a52-7c1e-7b3a-9d4e-5a6b7c8d9e99";

/// The ports of the device: every connection acts for the person in `who` (read at `bootstrap`, so a test can switch it).
fn ports_of(who: &Rc<RefCell<PersonRef>>) -> Ports {
    let who = who.clone();
    Ports::new(
        |_: &Caller| Actor::Person,
        move |_: &Caller| Some(who.borrow().clone()),
    )
}

fn open<M: ModuleCore>(g: &mut Guarded<M>) -> form_source::Client {
    FromClientHook::new(g.bootstrap(&Caller::new("1000", None)).hook)
}

/// The suite against one `Guarded`: `client` and `second` are two connections of the same wrapper.
async fn suite<M: ModuleCore>(core: M, blob: bool) -> Report {
    let who = Rc::new(RefCell::new(person(OWNER)));
    let mut g = Guarded::new(core, ports_of(&who));
    let fx = FakeFixture::new(open(&mut g))
        .with_second(open(&mut g))
        .with_blob(blob);
    run_conformance(&fx).await
}

/// The rules say yes to everything (the wrong rules of counter-proof ii).
struct AllowAll;

impl RightsRules for AllowAll {
    fn may(&self, _: &Ask<'_>) -> Verdict {
        Verdict::Allow
    }
    fn hidden(&self, _: &Context, _: &str, _: &str) -> Option<HiddenReason> {
        None
    }
}

/// Pattern: Decorator - the module `M` with the rules of `AllowAll`; everything else is `M`.
struct Lax<M: ModuleCore> {
    inner: M,
    rules: AllowAll,
}

impl<M: ModuleCore> ModuleCore for Lax<M> {
    fn level(&self) -> u32 {
        self.inner.level()
    }
    fn provider(&self) -> &str {
        self.inner.provider()
    }
    fn rules(&self) -> &dyn RightsRules {
        &self.rules
    }
    async fn page(&self, cx: &Context, query: PageQuery) -> Result<Page, Refusal> {
        self.inner.page(cx, query).await
    }
    async fn blob(&self, cx: &Context, record: &str, field: &str) -> Result<Blob, Refusal> {
        self.inner.blob(cx, record, field).await
    }
    async fn apply(&self, cx: &Context, command: Invocation) -> Result<Applied, Refusal> {
        self.inner.apply(cx, command).await
    }
    async fn revert(&self, cx: &Context, change: &str, step: StepRef) -> Result<Applied, Refusal> {
        self.inner.revert(cx, change, step).await
    }
    async fn changes(&self, cx: &Context, query: ChangesQuery) -> Result<ChangePage, Refusal> {
        self.inner.changes(cx, query).await
    }
    fn feed(&self, sink: ChangeFeed) {
        self.inner.feed(sink);
    }
    fn on_feed_break(&self, sink: FeedBreak) {
        self.inner.on_feed_break(sink);
    }
    async fn resolve_scope(
        &self,
        cx: &Context,
        form: &str,
        scope: &str,
    ) -> Result<String, Refusal> {
        self.inner.resolve_scope(cx, form, scope).await
    }
    fn follows(&self, cx: &Context, form: &str, scope: &str) -> Result<Follow, Refusal> {
        self.inner.follows(cx, form, scope)
    }
}

fn has(list: &[String], text: &str) -> bool {
    list.iter().any(|f| f.contains(text))
}

/// The skips that are right by design: "9 card.clear" (the fake LOCKS that set command on purpose, rule 11b uses it), "12 blob" (the
/// fake has an image, rule 20 reads it), "L2-9 gone" (the contract `module` has no redaction, so `FakeFixture` cannot black out a change; card).
const ALLOWED_SKIPS: [&str; 3] = ["9 card.clear:", "12 blob", "L2-9 gone"];

/// G8 for real: the whole suite over the wrapper: 0 failed, L2-2 to L2-11 PASSED; nothing else is skipped than `ALLOWED_SKIPS`.
#[test]
fn g8_the_suite_passes_over_guarded_fake_core() {
    run(async {
        let report = suite(FakeCore::new(Mode::Strict).with_blob(), true).await;
        assert!(report.failures.is_empty(), "failed: {:?}", report.failures);
        assert!(report.checked > 50, "the suite ran: {}", report.checked);
        assert!(
            report
                .skipped
                .iter()
                .all(|s| ALLOWED_SKIPS.iter().any(|a| s.starts_with(a))),
            "skipped: {:?}",
            report.skipped
        );
        eprintln!("skipped: {:?}", report.skipped);
    });
}

/// Counter-proof (i): a module that runs the same step twice is caught by L2-3 through the wrapper.
#[test]
fn counter_i_a_module_that_ignores_the_step_turns_l2_3_red() {
    run(async {
        let report = suite(FakeCore::new(Mode::IgnoreStep), false).await;
        assert!(has(&report.failures, "L2-3"), "{:?}", report.failures);
    });
}

/// Counter-proof (ii): with rules "everything is allowed" the ban of rule 11 is gone: it came from the rules, asked by the wrapper.
#[test]
fn counter_ii_rules_that_allow_everything_turn_rule_11_red() {
    run(async {
        let lax = Lax {
            inner: FakeCore::new(Mode::Strict),
            rules: AllowAll,
        };
        let report = suite(lax, false).await;
        assert!(
            report.failures.iter().any(|f| f.starts_with("11 ")),
            "{:?}",
            report.failures
        );
    });
}

/// Counter-proof (iii): the connection's person is a stranger, the rule is "only the owner": 403 and `apply` ran 0 times for it.
#[test]
fn counter_iii_a_stranger_as_the_person_of_the_connection_is_refused_and_applies_nothing() {
    run(async {
        let who = Rc::new(RefCell::new(person(OWNER)));
        let mut g = Guarded::new(OwnerOnlyModule::new(), ports_of(&who));
        let owner = open(&mut g);
        let made = invoke(&owner, OWNED_CREATE, "", &[("text", "mine")]).await;
        assert!(!made.failed, "{made:?}");
        assert_eq!(g.core().applied(), 1);

        *who.borrow_mut() = person(STRANGER);
        let stranger = open(&mut g);
        let refused = invoke(&stranger, OWNED_SET, &made.record, &[("text", "x")]).await;
        assert_eq!((refused.failed, refused.status), (true, 403), "{refused:?}");
        assert_eq!(g.core().applied(), 1, "apply never ran for the stranger");
    });
}
