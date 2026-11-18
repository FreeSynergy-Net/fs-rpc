//! The administrator port of the tests that checks nothing (a store without the on-behalf rule).
use fs_forms_layout::*;

/// An administrator port that checks nothing (a store without the on-behalf rule): what refuses a bot is then the service alone.
pub struct Lax;

impl LayoutAdmin for Lax {
    fn lock<'a>(
        &'a self,
        _: &'a LayoutId,
        _: Option<&'a ElemRef>,
        _: &'a str,
        _: &'a Who,
    ) -> Pending<'a, bool> {
        Box::pin(std::future::ready(Ok(true)))
    }
    fn unlock<'a>(
        &'a self,
        _: &'a LayoutId,
        _: Option<&'a ElemRef>,
        _: &'a str,
        _: &'a Who,
    ) -> Pending<'a, bool> {
        Box::pin(std::future::ready(Ok(true)))
    }
    fn create_instance_overlay<'a>(
        &'a self,
        _: &'a LayoutId,
        _: Option<&'a str>,
        _: Option<&'a str>,
        _: &'a Who,
    ) -> Pending<'a, OverlayId> {
        Box::pin(std::future::ready(Err(
            Coded::new(LayoutCode::Unsupported).with("what", "test")
        )))
    }
}
