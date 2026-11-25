//! The settings as a module of the contract `module_v1`: the form `fields-in-groups@1.0.0`, the commands `value.set` and `value.reset`.
//!
//! Pattern: Template Method (steps) - `SettingsCore` delivers only reading and changing, the order check -> change -> report stands
//! once in `fs_rpc::Guarded`; Strategy - [`SettingsRules`] are its rights. Deviation: not a Template Method in the strict sense (the
//! wrapper is generic, there is no base class).
//!
//! It keeps nothing of its own: it reads and writes through the port `SettingsStore` (via `fs_settings::Settings`). It checks NO
//! right (the trait says so): that is the job of [`SettingsRules`], asked by the wrapper before `page` and `apply`; so the view here is
//! the one of a person and nothing is filtered in this file. A LOCKED setting is no question of rights but a state of the data:
//! it ends in the refusal 409 `conflict` with the detail `locked`, said by the store. Level 1: no `write_by` log in settings, so no
//! `changes`, no `revert`, no feed (undo through the own log of settings is a card).

#![allow(clippy::unused_async_trait_impl, clippy::unnecessary_literal_bound)] // the trait is async and says `&str`
#![allow(clippy::result_large_err)] // `Refusal` is the one error type of the contract `ModuleCore`

use std::sync::Arc;

use fs_interfaces::forms::parse_key;
use fs_interfaces::module::{
    Applied, Blob, ChangeFeed, ChangePage, ChangesQuery, Context, Invocation, ModuleCore, Page,
    PageQuery, Refusal, RightsRules, StepRef,
};
use fs_settings::{
    Actor, Catalog, NoSystem, ProgramName, SettingKey, Settings, SettingsStore, Surface, Target,
};

use crate::caller::scoped;
use crate::identity::os_user_of_module;
use crate::module_rows::{FORM, refusal_of, refuse, row, value_of};
use crate::module_rules::SettingsRules;
use crate::service::blocking;

/// The level this module speaks (no `write_by` log in settings).
pub const LEVEL: u32 = 1;
/// The commands: set the own value / take it back (the layer below applies again).
pub const SET: &str = "value.set";
/// See [`SET`].
pub const RESET: &str = "value.reset";
const DEFAULT_PAGE: usize = 100;
const MAX_PAGE: usize = 1000;

/// Pattern: Template Method (steps) - the settings of one program as a module; see the module doc.
///
/// Called directly it is the view of a PERSON and checks no right: serve it ONLY through `fs_rpc::Guarded` with [`SettingsRules`]
/// (never hand it out bare). Its store calls are sync and may block, so `page` and `apply` run them on the blocking pool (as the hub
/// does), with the OS user of the connection set for exactly that call, so the change log names who changed (the same value as via the hub).
pub struct SettingsCore<S: SettingsStore> {
    settings: Arc<Settings>,
    rules: SettingsRules,
    store: Arc<S>,
}

impl<S: SettingsStore + 'static> SettingsCore<S> {
    /// The settings of `program` with `catalog` in `store`; the rules are made of the catalog (its `human_only` keys).
    #[must_use]
    pub fn new(program: ProgramName, catalog: Catalog, store: Arc<S>) -> Self {
        let rules = SettingsRules::of(&catalog);
        let settings = Arc::new(Settings::open(
            program,
            catalog,
            store.clone(),
            Arc::new(NoSystem),
        ));
        Self {
            settings,
            rules,
            store,
        }
    }

    /// The store the module reads and writes through.
    #[must_use]
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }

    /// The window of a page: (skip, size); a cursor is a number as text.
    fn window(query: &PageQuery) -> Result<(usize, usize), Refusal> {
        let skip = if query.cursor.is_empty() {
            0
        } else {
            query
                .cursor
                .parse()
                .map_err(|_| refuse("bad-arguments", 400, "bad cursor"))?
        };
        let size = match query.limit {
            0 => DEFAULT_PAGE,
            n => usize::try_from(n).unwrap_or(MAX_PAGE).min(MAX_PAGE),
        };
        Ok((skip, size))
    }

    /// Runs the sync store calls `f` on the blocking pool with the OS user of the connection (`cx.caller`) set for exactly that call.
    async fn off<T: Send + 'static>(
        &self,
        cx: &Context,
        f: impl FnOnce(&Settings) -> Result<T, Refusal> + Send + 'static,
    ) -> Result<T, Refusal> {
        let (settings, user) = (self.settings.clone(), os_user_of_module(&cx.caller));
        blocking(move || scoped(user, || f(&settings)))
            .await
            .map_err(|_| refuse("unavailable", 503, "store call failed"))?
    }

    /// One page of the view of a person (reads again first: another program may have written since, the view is cached per stamp).
    fn read_page(settings: &Settings, skip: usize, size: usize) -> Page {
        settings.refresh();
        // The view of a person: who may see what is the question of the rules, asked by the wrapper (see the module doc).
        let surface = Surface::new(settings, Actor::Person);
        let all = surface.list();
        let may = [SET.to_owned(), RESET.to_owned()];
        let rows: Vec<_> = all
            .iter()
            .enumerate()
            .skip(skip)
            .take(size)
            .filter_map(|(i, d)| Some(row(d, &surface.get(&d.key).ok()?, i, &may)))
            .collect();
        let mut page = Page::default();
        page.total = Some(all.len() as u64);
        if skip + size < all.len() {
            page.next = (skip + size).to_string();
        }
        page.records = rows;
        page
    }

    /// `value.set` (`text` = `Some`) or `value.reset`.
    fn save(
        settings: &Settings,
        key: &SettingKey,
        text: Option<&str>,
        set: bool,
    ) -> Result<Applied, Refusal> {
        let d = settings
            .catalog()
            .get(key)
            .ok_or_else(|| refuse("unknown-record", 404, "no such setting"))?;
        let value = match text {
            Some(t) => Some(
                value_of(&d.kind, t)
                    .ok_or_else(|| refuse("bad-arguments", 400, "value does not fit"))?,
            ),
            None => None,
        };
        Surface::new(settings, Actor::Person)
            .save(key, value.as_ref(), Target::ThisProgram)
            .map_err(|c| refusal_of(&c))?;
        let mut done = Applied::default();
        key.as_str().clone_into(&mut done.record);
        (if set { "set" } else { "reset" }).clone_into(&mut done.text);
        Ok(done)
    }

    /// The scope: empty = the settings of this program; its own name is the same.
    fn check_scope(&self, scope: &str) -> Result<(), Refusal> {
        if scope.is_empty() || scope == self.settings.program().as_str() {
            Ok(())
        } else {
            Err(refuse("unknown-scope", 404, "no such program"))
        }
    }

    /// The key of the record a command is about: ONLY the record (the rules decide by the record, so a key in an argument could pass them by).
    fn key_of(command: &Invocation) -> Result<SettingKey, Refusal> {
        let bad = || refuse("bad-arguments", 400, "a setting is named by its record");
        let record = command.record.as_deref().filter(|r| !r.is_empty());
        let key = SettingKey::parse(record.ok_or_else(bad)?)
            .map_err(|_| refuse("unknown-record", 404, "no such setting"))?;
        Ok(key)
    }

    /// The arguments: `value` (only at `value.set`), `key` and `program` may repeat what the record and the module already say.
    fn text_arg<'a>(&self, command: &'a Invocation, set: bool) -> Result<Option<&'a str>, Refusal> {
        let (mut text, record) = (None, command.record.as_deref());
        for a in &command.args {
            let ok = match a.path.as_str() {
                "value" if set && text.is_none() => {
                    text = Some(a.text.as_str());
                    true
                }
                "key" => Some(a.text.as_str()) == record,
                "program" => a.text == self.settings.program().as_str(),
                _ => false,
            };
            if !ok {
                return Err(refuse("bad-arguments", 400, "unknown or doubled argument"));
            }
        }
        if set && text.is_none() {
            return Err(refuse("bad-arguments", 400, "value is required"));
        }
        Ok(text)
    }
}

impl<S: SettingsStore + 'static> ModuleCore for SettingsCore<S> {
    fn level(&self) -> u32 {
        LEVEL
    }

    fn provider(&self) -> &str {
        "settings"
    }

    fn rules(&self) -> &dyn RightsRules {
        &self.rules
    }

    async fn page(&self, cx: &Context, query: PageQuery) -> Result<Page, Refusal> {
        if !parse_key(&query.form).is_some_and(|(n, v)| n == FORM && (v.major, v.minor) == (1, 0)) {
            return Err(refuse("unknown-form", 404, "no such form"));
        }
        self.check_scope(&query.scope)?;
        let (skip, size) = Self::window(&query)?;
        self.off(cx, move |settings| {
            Ok(Self::read_page(settings, skip, size))
        })
        .await
    }

    async fn blob(&self, _cx: &Context, _record: &str, _field: &str) -> Result<Blob, Refusal> {
        Err(refuse("unsupported", 501, "settings have no binary field"))
    }

    async fn apply(&self, cx: &Context, command: Invocation) -> Result<Applied, Refusal> {
        let (set, reset) = (command.command == SET, command.command == RESET);
        if !set && !reset {
            return Err(refuse("unknown-command", 404, "no such command"));
        }
        let key = Self::key_of(&command)?;
        let text = self.text_arg(&command, set)?.map(str::to_owned);
        self.off(cx, move |settings| {
            Self::save(settings, &key, text.as_deref(), set)
        })
        .await
    }

    async fn revert(
        &self,
        _cx: &Context,
        _change: &str,
        _step: StepRef,
    ) -> Result<Applied, Refusal> {
        Err(refuse(
            "unsupported",
            501,
            "level 1: no log to take a change back from",
        ))
    }

    async fn changes(&self, _cx: &Context, _query: ChangesQuery) -> Result<ChangePage, Refusal> {
        Ok(ChangePage::default())
    }

    fn feed(&self, _sink: ChangeFeed) {
        // Level 1: no change feed (a program that writes through the store is told by the stamp, not here).
    }
}
