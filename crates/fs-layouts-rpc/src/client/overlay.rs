//! `OverlayWrite` over the contract. The op list goes whole (position = seq); who and time of an op are not sent, the service and
//! the store set them (D13: `expect` is the store's, whatever the caller puts in it is ignored).
//!
//! Pattern: Adapter (port -> contract).

use fs_forms_layout::{
    LayoutId, Op, OpKind, OverlayDoc, OverlayId, OverlayWrite, Pending, Report, Resolution, Rev,
    Utc, Who,
};

use super::RpcLayouts;
use crate::wire::answers::read_report;
use crate::wire::conflict::write_overlay_doc;
use crate::wire::ids::{read_id, write_id};
use crate::wire::overlay::write_ops;
use crate::wire::text::n;

impl OverlayWrite for RpcLayouts {
    fn create_overlay<'a>(
        &'a self,
        layout: &'a LayoutId,
        lang: Option<&'a str>,
        title_key: Option<&'a str>,
        who: &'a Who,
    ) -> Pending<'a, OverlayId> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.create_overlay_request();
            write_id(req.get().init_layout(), &layout.0)?;
            req.get().set_lang(lang.unwrap_or(""));
            req.get().set_title_key(title_key.unwrap_or(""));
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(OverlayId(read_id(r.get()?.get_id()?)))
        }))
    }

    fn replace_ops<'a>(
        &'a self,
        id: &'a OverlayId,
        expected: Rev,
        ops: Vec<OpKind>,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.replace_ops_request();
            write_id(req.get().init_id(), &id.0)?;
            req.get().set_expected_rev(expected.0);
            req.get().set_author(&who.author.0);
            let ops: Vec<Op> = (0..)
                .zip(ops)
                .map(|(seq, kind)| Op {
                    seq,
                    kind,
                    who: who.clone(),
                    at: Utc(0),
                })
                .collect();
            write_ops(req.get().init_ops(n(&ops)), &ops);
            let r = req.send().promise.await?;
            Ok(read_report(r.get()?.get_report()?)?)
        }))
    }

    fn set_state<'a>(
        &'a self,
        id: &'a OverlayId,
        expected: Rev,
        on: bool,
        who: &'a Who,
    ) -> Pending<'a, Rev> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.set_overlay_state_request();
            write_id(req.get().init_id(), &id.0)?;
            req.get().set_expected_rev(expected.0);
            req.get().set_on(on);
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(Rev(r.get()?.get_rev()))
        }))
    }

    fn delete_overlay<'a>(
        &'a self,
        id: &'a OverlayId,
        expected: Rev,
        who: &'a Who,
    ) -> Pending<'a, ()> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.delete_overlay_request();
            write_id(req.get().init_id(), &id.0)?;
            req.get().set_expected_rev(expected.0);
            req.get().set_author(&who.author.0);
            req.send().promise.await?;
            Ok(())
        }))
    }

    fn import_overlay<'a>(
        &'a self,
        doc: OverlayDoc,
        mode: fs_forms_layout::Mode,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.import_overlay_request();
            write_overlay_doc(req.get().init_doc(), &doc, who, Utc(0));
            req.get()
                .set_dry_run(matches!(mode, fs_forms_layout::Mode::DryRun));
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(read_report(r.get()?.get_report()?)?)
        }))
    }

    fn resolve_conflict<'a>(
        &'a self,
        id: &'a OverlayId,
        seq: u32,
        base_version: &'a str,
        how: Resolution,
        who: &'a Who,
    ) -> Pending<'a, Rev> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.resolve_conflict_request();
            write_id(req.get().init_overlay(), &id.0)?;
            req.get().set_seq(seq);
            req.get().set_base_version(base_version);
            match &how {
                Resolution::Keep => req.get().set_how("keep"),
                Resolution::Drop => req.get().set_how("drop"),
                Resolution::Retarget(e) => {
                    req.get().set_how("retarget");
                    req.get().set_retarget(e.as_str());
                }
            }
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(Rev(r.get()?.get_rev()))
        }))
    }
}
