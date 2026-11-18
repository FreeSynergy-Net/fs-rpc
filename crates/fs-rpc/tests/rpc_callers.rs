//! The caller comes from the connection, never from the client: a real server on a real socket (in a temp folder of the test),
//! a capability built for each connection with its `Caller`, and a client that claims to be somebody else.
#![cfg(unix)]

mod callers_support;

use std::sync::atomic::Ordering;

use callers_support::{Server, call_as, my_name};

#[test]
fn every_connection_of_this_user_has_the_same_caller_whatever_the_client_claims() {
    let server = Server::start();
    for claim in ["root", "", "mallory", "1"] {
        call_as(&server.path, claim);
    }
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 4);
    let uid = rustix::process::geteuid().as_raw().to_string();
    for (caller, claim) in &seen {
        assert_eq!(caller.id(), uid, "claim {claim:?}");
        assert_eq!(caller.name(), Some(my_name().as_str()), "claim {claim:?}");
    }
    assert_eq!(
        seen.iter().map(|(_, c)| c.as_str()).collect::<Vec<_>>(),
        ["root", "", "mallory", "1"]
    );
    // One capability per connection: the factory ran for each, not once for the socket.
    assert_eq!(server.built.load(Ordering::SeqCst), 4);
}
