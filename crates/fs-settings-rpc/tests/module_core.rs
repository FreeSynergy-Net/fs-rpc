//! `SettingsCore` under `Guarded`: the suite `module_v1` level 1, the commands, the lock, and the counter-proof that a bot does not see
//! and cannot set a `human_only` setting BECAUSE of `SettingsRules` (with rules "everything is allowed" both tests are red).
//!
//! Pattern: none (tests).
#![allow(
    clippy::unused_async_trait_impl,
    clippy::unnecessary_literal_bound,
    clippy::result_large_err
)]

mod module_support;

use fs_interfaces::formsource_capnp::form_source;
use fs_interfaces::module::{
    Applied, Ask, Blob, ChangeFeed, ChangePage, ChangesQuery, Context, FeedBreak, Follow,
    HiddenReason, Invocation, ModuleCore, Page, PageQuery, Refusal, RightsRules, StepRef, Verdict,
};
use fs_module::conformance::run_conformance;
use fs_rpc::Guarded;
use fs_settings::{MemoryStore, Place, Raw, Scope, SettingsStore};
use fs_settings_rpc::SettingsCore;
use module_support::{
    AI, DENSITY, PINNED, SettingsFixture, WEEK, connect, core, invoke, lock_pinned, ports, rows,
    run,
};

/// The rules say yes to everything (the wrong rules of the counter-proof).
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
    fn follows(&self, cx: &Context, form: &str, scope: &str) -> Result<Follow, Refusal> {
        self.inner.follows(cx, form, scope)
    }
}

fn stored(store: &MemoryStore, key: &str) -> Option<Raw> {
    let scope = Scope::Program(fs_settings::ProgramName::new("tasks").unwrap());
    store
        .profile(&scope, Place::Profile, 0)
        .values
        .get(&module_support::key(key))
        .cloned()
}

/// Level 1, the whole suite, over the wrapper: 0 failed; the rules of levels 2 and about rights per record are skipped, never passed.
#[test]
fn the_suite_level_1_passes_over_guarded_settings_core() {
    run(async {
        let store = MemoryStore::new();
        let mut g = Guarded::new(core(&store), ports());
        let fx = SettingsFixture {
            client: connect(&mut g, "person"),
        };
        let report = run_conformance(&fx).await;
        assert!(report.failures.is_empty(), "failed: {:?}", report.failures);
        assert!(report.checked > 20, "the suite ran: {}", report.checked);
        eprintln!("skipped: {:?}", report.skipped);
    });
}

/// Two connections of one wrapper over `core`: the bot and a person.
fn two<M: ModuleCore>(core: M) -> (Guarded<M>, form_source::Client, form_source::Client) {
    let mut g = Guarded::new(core, ports());
    let (bot, person) = (connect(&mut g, "bot"), connect(&mut g, "person"));
    (g, bot, person)
}

fn lax(store: &MemoryStore) -> Lax<SettingsCore<MemoryStore>> {
    Lax {
        inner: core(store),
        rules: AllowAll,
    }
}

/// Test 1 (a bot sees nothing of it), one body for the proof and the counter-proof: does the bot see `AI`?
async fn bot_sees<M: ModuleCore>(core: M) -> bool {
    let (_g, bot, person) = two(core);
    assert!(
        rows(&person).await.iter().any(|(id, _)| id == AI),
        "a person sees it"
    );
    rows(&bot).await.iter().any(|(id, _)| id == AI)
}

/// Test 2 (a bot cannot set it), the same: did the bot's `value.set` change the store?
async fn bot_sets<M: ModuleCore>(core: M, store: &MemoryStore) -> bool {
    let (_g, bot, _person) = two(core);
    let _answer = invoke(&bot, "value.set", AI, &[("value", "true")]).await;
    stored(store, AI).is_some()
}

#[test]
fn a_bot_does_not_see_a_human_only_setting() {
    run(async {
        let store = MemoryStore::new();
        assert!(
            !bot_sees(core(&store)).await,
            "the bot must not see the setting"
        );
        // the other settings are there for the bot
        let (_g, bot, _person) = two(core(&store));
        let ids: Vec<_> = rows(&bot).await.into_iter().map(|r| r.0).collect();
        assert_eq!(ids, [DENSITY, PINNED, WEEK]);
    });
}

#[test]
fn a_bot_cannot_set_a_human_only_setting() {
    run(async {
        let store = MemoryStore::new();
        assert!(
            !bot_sets(core(&store), &store).await,
            "the bot must not set it"
        );
        let (_g, bot, _person) = two(core(&store));
        let refused = invoke(&bot, "value.set", AI, &[("value", "true")]).await;
        assert_eq!((refused.failed, refused.status), (true, 403), "{refused:?}");
    });
}

/// The counter-proof: with rules "everything is allowed" the bot sees it AND sets it (both facts of the two tests above turn around).
#[test]
fn counter_rules_that_allow_everything_let_the_bot_in() {
    run(async {
        let (a, b) = (MemoryStore::new(), MemoryStore::new());
        assert!(
            bot_sees(lax(&a)).await,
            "with AllowAll the bot sees the setting"
        );
        assert!(bot_sets(lax(&b), &b).await, "with AllowAll the bot sets it");
    });
}
#[test]
fn a_person_sets_and_resets_and_a_locked_setting_is_409_locked() {
    run(async {
        let store = MemoryStore::new();
        lock_pinned(&store);
        let mut g = Guarded::new(core(&store), ports());
        let me = connect(&mut g, "person");
        let done = invoke(&me, "value.set", WEEK, &[("value", "4")]).await;
        assert!(!done.failed, "{done:?}");
        assert_eq!(done.record, WEEK);
        assert_eq!(stored(&store, WEEK), Some(Raw::Int(4)));
        assert!(!invoke(&me, "value.reset", WEEK, &[]).await.failed);
        assert_eq!(stored(&store, WEEK), None);
        // a locked setting is a state of the data: 409, from the store, whoever asks
        let locked = invoke(&me, "value.set", PINNED, &[("value", "false")]).await;
        assert_eq!(
            (locked.failed, locked.status, locked.kind.as_str()),
            (true, 409, "form-source-conflict"),
            "{locked:?}"
        );
        assert_eq!(stored(&store, PINNED), None);
    });
}

#[test]
fn bad_calls_are_refused_and_change_nothing() {
    run(async {
        let store = MemoryStore::new();
        let mut g = Guarded::new(core(&store), ports());
        let me = connect(&mut g, "person");
        for (command, record, args, status) in [
            ("value.set", WEEK, vec![("value", "9")], 400),
            ("value.set", WEEK, vec![("value", "x")], 400),
            ("value.set", WEEK, vec![], 400),
            ("value.set", "", vec![("value", "3")], 400),
            ("value.set", WEEK, vec![("key", AI), ("value", "3")], 400),
            ("value.set", "program.tasks.nope", vec![("value", "3")], 404),
            ("value.lock", WEEK, vec![], 404),
        ] {
            let r = invoke(&me, command, record, &args).await;
            assert_eq!(
                (r.failed, r.status),
                (true, status),
                "{command} {args:?}: {r:?}"
            );
        }
        assert_eq!(stored(&store, WEEK), None);
    });
}
