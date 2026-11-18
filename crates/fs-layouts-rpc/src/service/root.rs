//! The root capability `Layouts`: every method of the contract in ordinal order, each handed to its handler. What the person
//! service cannot do answers `unsupported{what}` (never an empty answer): the file methods and `repair` (parsing and the database
//! file belong to the program's own crates, which this adapter does not link) and `bind` (the host's interface).
//!
//! Pattern: Facade (one list, no logic) + Null-object-free provider rule (ADR 0045).

use std::rc::Rc;

use fs_forms_layout::{Coded, LayoutCode, LayoutStore};
use fs_interfaces::Capabilities;

use super::{Shared, admin, overlay, read, write};
use crate::layouts_capnp::layouts as l;

pub(crate) struct Root<S: LayoutStore + ?Sized> {
    pub(crate) shared: Rc<Shared<S>>,
}

/// The contract version this adapter speaks (draft 0.1, probe).
pub(crate) const CONTRACT_VERSION: &str = "0.1";

fn unsupported(what: &str) -> capnp::Error {
    crate::problem::err(&Coded::new(LayoutCode::Unsupported).with("what", what))
}

/// What this service offers: one entry per group of methods; the unsupported ones with the reason as a token.
fn capabilities<S: LayoutStore + ?Sized>(sh: &Shared<S>) -> Capabilities {
    let mut c = Capabilities::new("fs-layouts-rpc");
    c.supported("layouts.read")
        .supported("layouts.write")
        .supported("layouts.package")
        .supported("layouts.overlay");
    c.supported("layouts.watch");
    if sh.svc.admin.is_some() {
        c.supported("layouts.admin");
    } else {
        c.unsupported("layouts.admin", "person-service");
    }
    c.unsupported("layouts.files", "not-linked")
        .unsupported("layouts.repair", "not-linked")
        .unsupported("layouts.bind", "not-linked");
    c
}

/// A method that forwards to a handler of the same shape.
macro_rules! forward {
    ($($m:ident, $p:ident, $r:ident => $f:path;)*) => {$(
        async fn $m(self: Rc<Self>, p: l::$p, r: l::$r) -> Result<(), capnp::Error> {
            $f(&self.shared, p, r).await.map_err(Into::into)
        }
    )*};
}

#[allow(clippy::unused_async_trait_impl)] // the generated server trait is async
impl<S: LayoutStore + ?Sized + 'static> l::Server for Root<S> {
    async fn capabilities(
        self: Rc<Self>,
        _: l::CapabilitiesParams,
        mut r: l::CapabilitiesResults,
    ) -> Result<(), capnp::Error> {
        capabilities(&self.shared).write_list(CONTRACT_VERSION, r.get().init_capabilities());
        Ok(())
    }

    async fn support_report(
        self: Rc<Self>,
        _: l::SupportReportParams,
        mut r: l::SupportReportResults,
    ) -> Result<(), capnp::Error> {
        capabilities(&self.shared).write(CONTRACT_VERSION, r.get().init_report());
        Ok(())
    }

    forward! {
        stamp, StampParams, StampResults => read::stamp;
        health, HealthParams, HealthResults => read::health;
        list, ListParams, ListResults => read::list;
        original, OriginalParams, OriginalResults => read::original;
        resolve, ResolveParams, ResolveResults => read::resolve;
        create, CreateParams, CreateResults => write::create;
        import, ImportParams, ImportResults => write::import;
        save, SaveParams, SaveResults => write::save;
        delete, DeleteParams, DeleteResults => write::delete;
        install, InstallParams, InstallResults => write::install;
        uninstall, UninstallParams, UninstallResults => write::uninstall;
        overlays, OverlaysParams, OverlaysResults => overlay::overlays;
        overlay, OverlayParams, OverlayResults => overlay::overlay;
        create_overlay, CreateOverlayParams, CreateOverlayResults => overlay::create_overlay;
        replace_ops, ReplaceOpsParams, ReplaceOpsResults => overlay::replace_ops;
        set_overlay_state, SetOverlayStateParams, SetOverlayStateResults => overlay::set_overlay_state;
        delete_overlay, DeleteOverlayParams, DeleteOverlayResults => overlay::delete_overlay;
        import_overlay, ImportOverlayParams, ImportOverlayResults => overlay::import_overlay;
        conflicts, ConflictsParams, ConflictsResults => read::conflicts;
        resolve_conflict, ResolveConflictParams, ResolveConflictResults => overlay::resolve_conflict;
        log, LogParams, LogResults => read::log;
        admin, AdminParams, AdminResults => admin::admin;
    }

    async fn watch(
        self: Rc<Self>,
        p: l::WatchParams,
        mut r: l::WatchResults,
    ) -> Result<(), capnp::Error> {
        let sh = &self.shared;
        if sh.listeners.len() >= sh.svc.limits.watchers_max as usize {
            let c = Coded::new(LayoutCode::TooMany)
                .with("what", "watchers")
                .with("max", sh.svc.limits.watchers_max)
                .with("by", sh.svc.limits.by.as_str());
            return Err(crate::problem::err(&c));
        }
        let who = sh.caller.id();
        if sh.listeners.count_of(who) >= sh.svc.limits.watchers_per_actor_max as usize {
            let c = Coded::new(LayoutCode::TooMany)
                .with("what", "watchers-of-caller")
                .with("max", sh.svc.limits.watchers_per_actor_max)
                .with("by", sh.svc.limits.by.as_str());
            return Err(crate::problem::err(&c));
        }
        let id = sh.listeners.add(who, p.get()?.get_listener()?);
        let sub = super::listeners::Subscription {
            id,
            listeners: sh.listeners.clone(),
        };
        r.get().set_subscription(capnp_rpc::new_client::<
            fs_interfaces::common_capnp::subscription::Client,
            _,
        >(sub));
        Ok(())
    }

    async fn install_file(
        self: Rc<Self>,
        _: l::InstallFileParams,
        _: l::InstallFileResults,
    ) -> Result<(), capnp::Error> {
        Err(unsupported("installFile"))
    }

    async fn import_file(
        self: Rc<Self>,
        _: l::ImportFileParams,
        _: l::ImportFileResults,
    ) -> Result<(), capnp::Error> {
        Err(unsupported("importFile"))
    }

    async fn export_file(
        self: Rc<Self>,
        _: l::ExportFileParams,
        _: l::ExportFileResults,
    ) -> Result<(), capnp::Error> {
        Err(unsupported("exportFile"))
    }

    async fn import_overlay_file(
        self: Rc<Self>,
        _: l::ImportOverlayFileParams,
        _: l::ImportOverlayFileResults,
    ) -> Result<(), capnp::Error> {
        Err(unsupported("importOverlayFile"))
    }

    async fn export_overlay_file(
        self: Rc<Self>,
        _: l::ExportOverlayFileParams,
        _: l::ExportOverlayFileResults,
    ) -> Result<(), capnp::Error> {
        Err(unsupported("exportOverlayFile"))
    }

    async fn repair(
        self: Rc<Self>,
        _: l::RepairParams,
        _: l::RepairResults,
    ) -> Result<(), capnp::Error> {
        Err(unsupported("repair"))
    }

    async fn bind(self: Rc<Self>, _: l::BindParams, _: l::BindResults) -> Result<(), capnp::Error> {
        Err(unsupported("bind"))
    }
}
