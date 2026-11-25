//! The bytes of a binary field behind the `Blob` capability.
//!
//! Pattern: Adapter - a `module::Blob` (media type + bytes) as the `Blob` interface of `common.capnp`.

use std::rc::Rc;

use fs_interfaces::common_capnp::blob;
use fs_interfaces::module::Blob;

use crate::page::clamp_read;

pub(super) struct Bytes(Blob);

impl Bytes {
    pub(super) fn new(blob: Blob) -> Self {
        Self(blob)
    }
}

impl blob::Server for Bytes {
    #[allow(clippy::unused_async_trait_impl)] // the generated server trait is async; nothing to await here
    async fn size(
        self: Rc<Self>,
        _: blob::SizeParams,
        mut results: blob::SizeResults,
    ) -> Result<(), capnp::Error> {
        let mut out = results.get();
        out.set_bytes(self.0.bytes.len() as u64);
        out.set_media_type(self.0.media_type.as_str());
        Ok(())
    }

    #[allow(clippy::unused_async_trait_impl)] // the generated server trait is async; nothing to await here
    async fn read(
        self: Rc<Self>,
        params: blob::ReadParams,
        mut results: blob::ReadResults,
    ) -> Result<(), capnp::Error> {
        let p = params.get()?;
        let bytes = &self.0.bytes;
        let start = usize::try_from(p.get_offset())
            .unwrap_or(usize::MAX)
            .min(bytes.len());
        let end = start
            .saturating_add(usize::try_from(clamp_read(p.get_length())).unwrap_or(usize::MAX))
            .min(bytes.len());
        results.get().set_data(&bytes[start..end]);
        Ok(())
    }
}
