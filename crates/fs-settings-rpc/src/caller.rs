//! Who the caller is, as the OS says: the port of the instance (`InstanceStore`) has no place for it, so the service puts it where a
//! port that logs can read it for the duration of ONE call: [`current_os_user`].
//!
//! Pattern: Context (a thread-local that lives exactly as long as one port call on the blocking pool).

use std::cell::RefCell;

thread_local! {
    static CALLER: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The OS user of the caller of the port call that is running on this thread: the kernel's answer for the connection (fs-rpc lets
/// in the same OS user only), never an environment variable and never a parameter of the caller. `None` outside a call of the
/// service or where the system gives none.
#[must_use]
pub fn current_os_user() -> Option<String> {
    CALLER.with(|c| c.borrow().clone())
}

/// Runs `f` with `user` as the caller; the previous value comes back afterwards.
pub(crate) fn scoped<T>(user: Option<String>, f: impl FnOnce() -> T) -> T {
    let before = CALLER.with(|c| c.replace(user));
    let out = f();
    CALLER.with(|c| *c.borrow_mut() = before);
    out
}
