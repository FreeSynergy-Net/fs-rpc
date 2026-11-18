//! `LayoutWrite` and `PackageLayouts` over the contract. The author on the wire is the author of `Who`; the actor is the
//! connection (the transport's identity), so `Who.actor` is not sent. A refusal arrives in the report, as the service wrote it.
//!
//! Pattern: Adapter (port -> contract).

use fs_forms_layout::{
    LayoutDoc, LayoutId, LayoutWrite, Mode, PackageLayouts, PackageRef, Pending, Report, Rev, Who,
};

use super::RpcLayouts;
use crate::wire::answers::read_report;
use crate::wire::doc_out::write_doc;
use crate::wire::ids::write_id;

fn dry(m: Mode) -> bool {
    matches!(m, Mode::DryRun)
}

impl LayoutWrite for RpcLayouts {
    fn create<'a>(&'a self, doc: LayoutDoc, who: &'a Who) -> Pending<'a, Report> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.create_request();
            write_doc(req.get().init_doc(), &doc);
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(read_report(r.get()?.get_report()?)?)
        }))
    }

    fn import<'a>(&'a self, doc: LayoutDoc, mode: Mode, who: &'a Who) -> Pending<'a, Report> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.import_request();
            write_doc(req.get().init_doc(), &doc);
            req.get().set_dry_run(dry(mode));
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(read_report(r.get()?.get_report()?)?)
        }))
    }

    fn save<'a>(
        &'a self,
        id: &'a LayoutId,
        expected: Rev,
        doc: LayoutDoc,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.save_request();
            write_id(req.get().init_id(), &id.0)?;
            req.get().set_expected_rev(expected.0);
            write_doc(req.get().init_doc(), &doc);
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(read_report(r.get()?.get_report()?)?)
        }))
    }

    fn delete<'a>(&'a self, id: &'a LayoutId, expected: Rev, who: &'a Who) -> Pending<'a, ()> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.delete_request();
            write_id(req.get().init_id(), &id.0)?;
            req.get().set_expected_rev(expected.0);
            req.get().set_author(&who.author.0);
            req.send().promise.await?;
            Ok(())
        }))
    }
}

impl PackageLayouts for RpcLayouts {
    fn install<'a>(
        &'a self,
        doc: LayoutDoc,
        package: &'a PackageRef,
        mode: Mode,
        who: &'a Who,
    ) -> Pending<'a, Report> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.install_request();
            write_doc(req.get().init_doc(), &doc);
            req.get().set_package(&package.package);
            req.get().set_version(&package.version);
            req.get().set_dry_run(dry(mode));
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(read_report(r.get()?.get_report()?)?)
        }))
    }

    fn uninstall<'a>(&'a self, package: &'a str, who: &'a Who) -> Pending<'a, Vec<Report>> {
        Box::pin(self.ctx.call(async move |c| {
            let mut req = c.uninstall_request();
            req.get().set_package(package);
            req.get().set_author(&who.author.0);
            let r = req.send().promise.await?;
            Ok(r.get()?
                .get_reports()?
                .iter()
                .map(read_report)
                .collect::<Result<Vec<_>, _>>()?)
        }))
    }
}
