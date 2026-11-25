//! The steps of a call: context, rules, module. Every method of the `FormSource` server goes through these.
//!
//! Pattern: Template Method as a wrapper - check the rules, then call the module; the order is written here once.
//! Deviation: none; a verdict is read only through `is_allowed()`.

#![allow(clippy::field_reassign_with_default)] // the values are `#[non_exhaustive]`: built by Default plus assignment
#![allow(clippy::result_large_err)] // `Refusal` is the contract's own value (the trait of the module returns it as it is)

use fs_interfaces::module::{
    Applied, Ask, Blob, ChangePage, ChangesQuery, Context, HiddenField, Invocation, ModuleCore,
    Page, PageQuery, Refusal, StepRef, Verdict,
};

use super::REVERT_COMMAND;
use super::conn::Conn;

impl<M: ModuleCore> Conn<M> {
    pub(super) fn context(&self) -> Result<Context, Refusal> {
        self.cx
            .borrow()
            .clone()
            .ok_or_else(|| Refusal::forbidden("no-person"))
    }

    /// Ask the rules. Only `is_allowed()` lets a call pass: a verdict this code does not know is a refusal.
    pub(super) fn permit(
        &self,
        cx: &Context,
        command: &str,
        record: Option<&str>,
    ) -> Result<(), Refusal> {
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

    pub(super) async fn write(
        &self,
        mut inv: Invocation,
        level_two: bool,
    ) -> Result<Applied, Refusal> {
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
    pub(super) fn screen(&self, cx: &Context, page: &mut Page) {
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

    pub(super) async fn read_page(&self, query: PageQuery) -> Result<Page, Refusal> {
        let cx = self.context()?;
        let mut page = self.core.page(&cx, query).await?;
        self.screen(&cx, &mut page);
        Ok(page)
    }

    pub(super) async fn read_blob(&self, record: &str, field: &str) -> Result<Blob, Refusal> {
        let cx = self.context()?;
        if self.core.rules().hidden(&cx, record, field).is_some() {
            return Err(Refusal::forbidden("hidden"));
        }
        self.core.blob(&cx, record, field).await
    }

    pub(super) async fn revert(&self, change: &str, step: StepRef) -> Result<Applied, Refusal> {
        let cx = self.context()?;
        self.permit(&cx, REVERT_COMMAND, Some(change))?;
        self.core.revert(&cx, change, step).await
    }

    pub(super) async fn changes(&self, query: ChangesQuery) -> Result<ChangePage, Refusal> {
        let cx = self.context()?;
        self.core.changes(&cx, query).await
    }
}
