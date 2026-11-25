//! One connection of a guarded module: its `Context` and the `FormSource` server for it.
//!
//! Pattern: Template Method as a wrapper - the order parse -> check the rules -> change -> answer is written once here.
//! Deviation: none; the module never sees a step it could leave out, and the context never comes from a call argument.

#![allow(clippy::field_reassign_with_default)] // the values are `#[non_exhaustive]`: built by Default plus assignment
#![allow(clippy::result_large_err)] // `Refusal` is the contract's own value (the trait of the module returns it as it is)
#![allow(clippy::unused_async_trait_impl)] // the generated server trait is async; some methods have nothing to await

use std::cell::RefCell;
use std::rc::Rc;

use fs_interfaces::formsource_capnp::form_source::{
    BlobParams, BlobResults, ChangesParams, ChangesResults, HelloParams, HelloResults,
    IntroduceParams, IntroduceResults, InvokeIfParams, InvokeIfResults, InvokeParams,
    InvokeResults, PageParams, PageResults, RevertParams, RevertResults, Server, WatchParams,
    WatchResults,
};
use fs_interfaces::module::{
    Applied, Ask, ChangePage, ChangesQuery, Claimed, Context, HiddenField, Invocation, ModuleCore,
    Page, PageQuery, Refusal, StepRef, Verdict,
};

use super::blob::Bytes;
use super::wire::{
    cursor_text, problem_of, read_change_id, read_step, read_values, refusal_error, weight,
    write_applied, write_change_ref, write_conflict, write_cursor, write_page,
};
use super::{Clock, REVERT_COMMAND, WatchHub};
use crate::page::{check_answer, check_intake};
use crate::problem::invalid_request;

/// The longest text `introduce` takes for program, instance or device.
const CLAIM_MAX: usize = 256;

pub(super) struct Conn<M: ModuleCore> {
    pub(super) core: Rc<M>,
    pub(super) hub: Rc<WatchHub>,
    pub(super) clock: Clock,
    /// `None`: the connection has no known person; everything is refused (fail closed).
    pub(super) cx: RefCell<Option<Context>>,
}

impl<M: ModuleCore> Conn<M> {
    fn context(&self) -> Result<Context, Refusal> {
        self.cx
            .borrow()
            .clone()
            .ok_or_else(|| Refusal::forbidden("no-person"))
    }

    /// Ask the rules. Only `is_allowed()` lets a call pass: a verdict this code does not know is a refusal.
    fn permit(&self, cx: &Context, command: &str, record: Option<&str>) -> Result<(), Refusal> {
        let at = (self.clock)();
        let verdict = self.core.rules().may(&Ask::new(cx, command, record, at));
        if verdict.is_allowed() {
            return Ok(());
        }
        Err(Refusal::forbidden(match &verdict {
            Verdict::Deny { reason } => reason,
            _ => "denied",
        }))
    }

    async fn write(&self, mut inv: Invocation, level_two: bool) -> Result<Applied, Refusal> {
        if !level_two {
            inv.expect = None;
            inv.step = None;
        }
        let cx = self.context()?;
        self.permit(&cx, &inv.command, inv.record.as_deref())?;
        self.core.apply(&cx, inv).await
    }

    /// Fill `may` and `hidden` from the rules. The module lists the commands it offers; the wrapper only strikes out what the
    /// rules refuse, and a hidden field loses its value (nothing leaks through `values`).
    fn screen(&self, cx: &Context, page: &mut Page) {
        let rules = self.core.rules();
        let at = (self.clock)();
        page.may
            .retain(|c| rules.may(&Ask::new(cx, c, None, at)).is_allowed());
        for rec in &mut page.records {
            let id = rec.id.clone();
            rec.may
                .retain(|c| rules.may(&Ask::new(cx, c, Some(&id), at)).is_allowed());
            let paths: Vec<String> = rec.values.iter().map(|v| v.path.clone()).collect();
            for path in paths {
                if let Some(reason) = rules.hidden(cx, &id, &path) {
                    rec.values.retain(|v| v.path != path);
                    if !rec.hidden.iter().any(|h| h.path == path) {
                        rec.hidden.push(HiddenField::new(&path, reason));
                    }
                }
            }
        }
    }

    async fn read_page(&self, query: PageQuery) -> Result<Page, Refusal> {
        let cx = self.context()?;
        let mut page = self.core.page(&cx, query).await?;
        self.screen(&cx, &mut page);
        Ok(page)
    }

    async fn read_blob(
        &self,
        record: &str,
        field: &str,
    ) -> Result<fs_interfaces::module::Blob, Refusal> {
        let cx = self.context()?;
        if self.core.rules().hidden(&cx, record, field).is_some() {
            return Err(Refusal::forbidden("hidden"));
        }
        self.core.blob(&cx, record, field).await
    }

    async fn revert(&self, change: &str, step: StepRef) -> Result<Applied, Refusal> {
        let cx = self.context()?;
        self.permit(&cx, REVERT_COMMAND, Some(change))?;
        self.core.revert(&cx, change, step).await
    }

    async fn changes(&self, query: ChangesQuery) -> Result<ChangePage, Refusal> {
        let cx = self.context()?;
        self.core.changes(&cx, query).await
    }
}

/// An answer of a write: `failed` + `problem`; a refusal leaves `result` empty.
macro_rules! answer {
    ($results:expr, $outcome:expr) => {{
        let mut out = $results.get();
        match $outcome {
            Ok(applied) => {
                out.set_failed(false);
                write_applied(out.init_result(), applied)?;
            }
            Err(refusal) => {
                out.set_failed(true);
                problem_of(refusal).write(out.init_problem());
            }
        }
    }};
}

impl<M: ModuleCore> Server for Conn<M> {
    async fn hello(
        self: Rc<Self>,
        _: HelloParams,
        mut results: HelloResults,
    ) -> Result<(), capnp::Error> {
        let mut out = results.get();
        out.set_level(self.core.level());
        out.set_provider(self.core.provider());
        Ok(())
    }

    async fn page(
        self: Rc<Self>,
        params: PageParams,
        mut results: PageResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let mut query = PageQuery::default();
        query.form = p.get_form()?.to_string()?;
        query.scope = p.get_scope()?.to_string()?;
        query.cursor = cursor_text(p.get_cursor()?)?;
        query.limit = p.get_limit();
        let page = self.read_page(query).await.map_err(|r| refusal_error(&r))?;
        check_answer(weight(&page))?;
        let mut out = results.get();
        write_page(out.reborrow().init_page(), &page);
        write_cursor(out.init_next(), &page.next);
        Ok(())
    }

    async fn blob(
        self: Rc<Self>,
        params: BlobParams,
        mut results: BlobResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let (record, field) = (p.get_record()?.to_string()?, p.get_field()?.to_string()?);
        let blob = self
            .read_blob(&record, &field)
            .await
            .map_err(|r| refusal_error(&r))?;
        results
            .get()
            .set_blob(capnp_rpc::new_client(Bytes::new(blob)));
        Ok(())
    }

    async fn invoke(
        self: Rc<Self>,
        params: InvokeParams,
        mut results: InvokeResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let inv = invocation(p.get_command()?, p.get_record()?, p.get_args()?)?;
        let outcome = self.write(inv, false).await;
        answer!(results, &outcome);
        Ok(())
    }

    async fn invoke_if(
        self: Rc<Self>,
        params: InvokeIfParams,
        mut results: InvokeIfResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let mut inv = invocation(p.get_command()?, p.get_record()?, p.get_args()?)?;
        let expect = p.get_expect()?.to_string()?;
        inv.expect = (!expect.is_empty()).then_some(expect);
        let step = read_step(p.get_step()?)?;
        inv.step = (!(step.run.is_empty() && step.step.is_empty())).then_some(step);
        let outcome = self.write(inv, true).await;
        answer!(results, &outcome);
        if let Err(Refusal {
            conflict: Some(c), ..
        }) = &outcome
        {
            write_conflict(results.get().init_conflict(), c)?;
        }
        Ok(())
    }

    async fn revert(
        self: Rc<Self>,
        params: RevertParams,
        mut results: RevertResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let change = read_change_id(p.get_change()?)?;
        let step = read_step(p.get_step()?)?;
        let outcome = Conn::revert(&self, &change, step).await;
        answer!(results, &outcome);
        if let Err(Refusal {
            conflict: Some(c), ..
        }) = &outcome
        {
            write_conflict(results.get().init_conflict(), c)?;
        }
        Ok(())
    }

    async fn changes(
        self: Rc<Self>,
        params: ChangesParams,
        mut results: ChangesResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let mut query = ChangesQuery::default();
        query.run = p.get_run()?.to_string()?;
        query.cursor = cursor_text(p.get_cursor()?)?;
        query.limit = p.get_limit();
        let page = Conn::changes(&self, query)
            .await
            .map_err(|r| refusal_error(&r))?;
        let mut out = results.get();
        let mut items = out
            .reborrow()
            .init_items(u32::try_from(page.changes.len()).unwrap_or(0));
        for (i, c) in (0..).zip(&page.changes) {
            write_change_ref(items.reborrow().get(i), c)?;
        }
        write_cursor(out.init_next(), &page.next);
        Ok(())
    }

    async fn introduce(
        self: Rc<Self>,
        params: IntroduceParams,
        _: IntroduceResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let (program, instance, device) = (
            p.get_program()?.to_str()?,
            p.get_instance()?.to_str()?,
            p.get_device()?.to_str()?,
        );
        if [program, instance, device]
            .iter()
            .any(|t| t.len() > CLAIM_MAX)
        {
            return Err(invalid_request("introduce: a text is too long"));
        }
        let claimed = Claimed::new(program, instance, device);
        let mut slot = self.cx.borrow_mut();
        // Only `claimed` changes (history and undo); the person and the actor stay what the connection said.
        let cx = slot
            .take()
            .ok_or_else(|| refusal_error(&Refusal::forbidden("no-person")))?;
        *slot = Some(cx.with_claimed(claimed));
        Ok(())
    }

    async fn watch(
        self: Rc<Self>,
        params: WatchParams,
        mut results: WatchResults,
    ) -> Result<(), capnp::Error> {
        let sink = params.get()?.get_sink()?;
        let cx = self.context().map_err(|r| refusal_error(&r))?;
        results.get().set_subscription(self.hub.subscribe(cx, sink));
        Ok(())
    }
}

fn invocation(
    command: capnp::text::Reader<'_>,
    record: capnp::text::Reader<'_>,
    args: capnp::struct_list::Reader<'_, fs_interfaces::formsource_capnp::value::Owned>,
) -> capnp::Result<Invocation> {
    let (command, record) = (command.to_string()?, record.to_string()?);
    let args = read_values(args)?;
    check_intake(
        args.iter()
            .map(|v| v.path.len() + v.text.len())
            .sum::<usize>()
            + command.len()
            + record.len(),
    )?;
    let mut inv = Invocation::default();
    inv.command = command;
    inv.record = (!record.is_empty()).then_some(record);
    inv.args = args;
    Ok(inv)
}
