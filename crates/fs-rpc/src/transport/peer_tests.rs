//! The caller of each connection, with made-up streams (the other user cannot be run in a test without root).
//! Pattern: Test double (a fake `CallerSource`) + Mutant-proof (each rule has a test that fails without it).
use std::io;

use capnp_rpc::new_client;
use fs_interfaces::tasks_capnp::tasks;

use super::{Caller, CallerError, CallerSource, PerConnection, bootstrap_for};

struct Fake(Result<Caller, io::ErrorKind>);

impl CallerSource for Fake {
    fn caller(&self) -> Result<Caller, CallerError> {
        self.0
            .clone()
            .map_err(|k| CallerError::Credentials(io::Error::from(k)))
    }
}

fn user(id: &str) -> Fake {
    Fake(Ok(Caller::new(id, Some(format!("user{id}")))))
}

struct Nothing;
impl tasks::Server for Nothing {}

#[derive(Default)]
struct Log {
    callers: Vec<Caller>,
    refused: Vec<&'static str>,
}

impl PerConnection for Log {
    fn bootstrap(&mut self, caller: &Caller) -> capnp::capability::Client {
        self.callers.push(caller.clone());
        let c: tasks::Client = new_client(Nothing);
        c.client
    }
    fn refused(&mut self, why: &CallerError) {
        self.refused.push(why.code());
    }
}

#[test]
fn each_connection_is_asked_anew_and_gets_its_own_caller() {
    let mut log = Log::default();
    for id in ["1000", "2000", "1000"] {
        assert!(bootstrap_for(&user(id), &mut log).is_some());
    }
    let ids: Vec<&str> = log.callers.iter().map(Caller::id).collect();
    assert_eq!(ids, ["1000", "2000", "1000"]);
    assert_eq!(log.callers[1].name(), Some("user2000"));
}

#[test]
fn a_caller_the_system_does_not_tell_is_refused_with_a_code_and_gets_nothing() {
    let mut log = Log::default();
    let bad = Fake(Err(io::ErrorKind::PermissionDenied));
    assert!(bootstrap_for(&bad, &mut log).is_none());
    assert_eq!(log.callers.len(), 0);
    assert_eq!(log.refused, ["caller-credentials"]);
    // The next connection is not poisoned by it.
    assert!(bootstrap_for(&user("1000"), &mut log).is_some());
    assert_eq!(log.callers.len(), 1);
}

#[cfg(unix)]
#[test]
fn the_name_comes_from_the_entry_with_that_user_id_and_the_first_wins() {
    let passwd = "# c\nroot:x:0:0:r:/root:/bin/sh\nann:x:1000:1000::/h/ann:/bin/sh\nbob:x:1000:1000::/h/bob:/bin/sh\n:x:7:7:::\n";
    let name = |uid| super::passwd_name(passwd, uid);
    assert_eq!(name(0).as_deref(), Some("root"));
    assert_eq!(name(1000).as_deref(), Some("ann"));
    assert_eq!(name(7), None);
    assert_eq!(name(5), None);
    assert_eq!(super::passwd_name("", 0), None);
    // The user id is compared as a whole number: 100 is not 1000.
    assert_eq!(name(100), None);
}

#[test]
fn the_error_says_its_code_and_no_text_of_the_system() {
    let e = CallerError::Credentials(io::Error::from(io::ErrorKind::PermissionDenied));
    assert_eq!(e.code(), "caller-credentials");
    assert!(e.to_string().starts_with("caller-credentials"));
}

/// A stream the kernel cannot give credentials for (a pipe, not a socket): the answer is the error, not a made-up user.
#[cfg(unix)]
#[test]
fn a_real_stream_without_credentials_is_an_error_not_a_default_user() {
    let (reader, _writer) = std::io::pipe().unwrap();
    let std_stream = std::os::unix::net::UnixStream::from(std::os::fd::OwnedFd::from(reader));
    std_stream.set_nonblocking(true).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _guard = rt.enter();
    let stream = tokio::net::UnixStream::from_std(std_stream).unwrap();
    let err = stream.caller().unwrap_err();
    assert_eq!(err.code(), "caller-credentials");
    // And a real socket pair says who we are: this user.
    let (a, _b) = tokio::net::UnixStream::pair().unwrap();
    let me = rustix::process::geteuid().as_raw().to_string();
    assert_eq!(a.caller().unwrap().id(), me);
}
