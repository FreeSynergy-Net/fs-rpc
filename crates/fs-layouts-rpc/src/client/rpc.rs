//! `RpcLayouts`: the type that holds the link and the check, and its constructors. The ports are implemented in `read`, `write`,
//! `overlay`; `admin` and `watch` are next to them.
//!
//! Pattern: Adapter (port -> contract), Proxy (the link is made on the first call and mended after a loss). Runs on a `LocalSet`
//! (capnp-rpc; the ports are `?Send`). No service: every call answers `unreachable{socket}`, never an empty window without reason.

use std::path::Path;
use std::rc::Rc;

use fs_forms_layout::{CatalogRules, Coded, Layout, LayoutCode, LayoutDoc, Limits, OpenCatalog};

use super::RpcLayoutAdmin;
use super::ctx::ClientTimes;
use crate::wire::text::R;

/// A check of the description the service sent. The service already checked and decided the limits (rule 57), so by default the
/// client takes the tree as it is: open catalog, no limits of its own. A host that wants its own check passes one.
type Check = Rc<dyn Fn(LayoutDoc) -> R<Layout>>;

/// The ports over the contract.
pub struct RpcLayouts {
    pub(crate) ctx: Rc<super::ctx::Ctx>,
    pub(crate) check: Check,
}

fn first(problems: Vec<Coded<LayoutCode>>) -> Coded<LayoutCode> {
    problems
        .into_iter()
        .next()
        .unwrap_or_else(|| crate::wire::text::bad("layout"))
}

/// The limits of "the service decides": nothing is refused by size here.
fn unlimited() -> Limits {
    let mut l = Limits::declared();
    (
        l.elements_max,
        l.menu_depth_max,
        l.text_bytes_max,
        l.ops_max,
        l.overlays_max,
    ) = (u32::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX);
    l
}

impl RpcLayouts {
    /// A client for the service on `socket`; the link is made on the first call (no service yet: calls say `unreachable`).
    #[must_use]
    pub fn new(socket: &Path, times: ClientTimes) -> Self {
        let check: Check =
            Rc::new(|doc: LayoutDoc| doc.check(&OpenCatalog, &unlimited()).map_err(first));
        Self {
            ctx: Rc::new(super::ctx::Ctx::new(socket.to_path_buf(), times)),
            check,
        }
    }

    /// Like [`new`](Self::new) and the link is made now.
    ///
    /// # Errors
    /// `unreachable{socket}` when nothing answers.
    pub async fn connect(socket: &Path, times: ClientTimes) -> Result<Self, Coded<LayoutCode>> {
        let me = Self::new(socket, times);
        me.ctx.link().await?;
        Ok(me)
    }

    /// Checks every description the service sends with your catalog and limits instead of taking it as it is.
    #[must_use]
    pub fn with_check(mut self, catalog: Rc<dyn CatalogRules>, limits: Limits) -> Self {
        self.check =
            Rc::new(move |doc: LayoutDoc| doc.check(catalog.as_ref(), &limits).map_err(first));
        self
    }

    /// The administrator's port on the same socket; `by` is the person who acts (the instance service only).
    #[must_use]
    pub fn admin(&self) -> RpcLayoutAdmin {
        RpcLayoutAdmin::new(self.ctx.clone())
    }
}
