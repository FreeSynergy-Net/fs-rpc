//! A caller is not read from the environment: even with `USER`, `LOGNAME` and `HOME` forged, the caller is what the system says.
//! Alone in its file: the environment belongs to the whole process.
#![cfg(unix)]

mod callers_support;

use callers_support::{Server, call_as, my_name};

#[test]
fn a_forged_environment_does_not_change_the_caller() {
    let real = my_name();
    // SAFETY: the only test of this process, nothing else reads the environment at this moment.
    unsafe {
        std::env::set_var("USER", "mallory");
        std::env::set_var("LOGNAME", "mallory");
        std::env::set_var("UID", "0");
    }
    let server = Server::start();
    call_as(&server.path, "root");
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0.name(), Some(real.as_str()));
    assert_eq!(
        seen[0].0.id(),
        rustix::process::geteuid().as_raw().to_string()
    );
}
