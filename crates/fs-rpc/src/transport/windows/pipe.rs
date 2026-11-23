//! The rights of the pipe: this user alone. Pattern: Factory (a pipe instance with its descriptor). The descriptor and the data
//! folder rights live in `fs-place`.

use std::io;

use fs_place::windows::acl::Descriptor;
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

/// One instance of the pipe `name`, open to this user alone (not to the system, not to administrators: the standard rights
/// would let others in). `first` is the rule that decides who owns the name.
pub(super) fn create_pipe(name: &str, first: bool) -> io::Result<NamedPipeServer> {
    let descriptor = Descriptor::only_me("")?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).unwrap_or(0),
        lpSecurityDescriptor: descriptor.as_ptr(),
        bInheritHandle: 0,
    };
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true);
    // The descriptor outlives the call; the system copies it.
    unsafe { options.create_with_security_attributes_raw(name, (&raw mut attributes).cast()) }
}
