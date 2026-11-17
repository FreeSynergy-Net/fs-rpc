//! Error codes and parameters: stable keys, typed parameters, a `Display` that is no sentence, and one test per code.

use std::collections::HashSet;
use std::io;
use std::time::Duration;

use fs_rpc::client::Fault;
use fs_rpc::{
    Expected, HandshakeError, Param, Problem, RestartCause, RpcError, RpcErrorCode, ServerInfo,
    too_large_by,
};

fn info(program: &str, version: &str, protocol: u32) -> ServerInfo {
    ServerInfo {
        program: program.into(),
        version: version.into(),
        protocol,
    }
}

fn text<'a>(e: &'a RpcError, key: &str) -> Option<&'a str> {
    e.param(key).and_then(Param::as_text)
}

#[test]
fn every_code_has_a_unique_key_and_a_unique_name() {
    let mut keys = HashSet::new();
    let mut names = HashSet::new();
    for code in RpcErrorCode::ALL {
        assert_eq!(code.message_key(), format!("rpc-error-{}", code.as_str()));
        assert!(
            code.as_str()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '-')
        );
        assert!(keys.insert(code.message_key()), "{code}");
        assert!(names.insert(code.as_str()), "{code}");
    }
    assert_eq!(keys.len(), RpcErrorCode::ALL.len());
}

#[test]
fn the_released_names_are_fixed() {
    let names: Vec<&str> = RpcErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(
        names,
        [
            "no-server",
            "socket-unreachable",
            "already-running",
            "contract-mismatch",
            "cannot-restart",
            "other-program",
            "too-old-to-stop",
            "timeout",
            "connection-lost",
            "interrupted",
            "unimplemented",
            "invalid-request",
            "too-large",
            "protocol",
            "io",
            "data-dir-not-private",
            "server-unresponsive",
        ]
    );
}

#[test]
fn io_kinds_become_codes() {
    use io::ErrorKind as K;
    let code = |k: K| RpcError::from(&io::Error::from(k)).code();
    assert_eq!(code(K::NotFound), RpcErrorCode::NoServer);
    assert_eq!(code(K::ConnectionRefused), RpcErrorCode::NoServer);
    assert_eq!(code(K::PermissionDenied), RpcErrorCode::SocketUnreachable);
    assert_eq!(code(K::AddrInUse), RpcErrorCode::AlreadyRunning);
    assert_eq!(code(K::TimedOut), RpcErrorCode::Timeout);
    assert_eq!(code(K::BrokenPipe), RpcErrorCode::ConnectionLost);
    assert_eq!(code(K::Unsupported), RpcErrorCode::Unimplemented);
    let other = RpcError::from(&io::Error::from(K::InvalidInput));
    assert_eq!(other.code(), RpcErrorCode::Io);
    assert_eq!(text(&other, "kind"), Some("invalid-input"));
}

#[test]
fn a_mismatch_carries_both_sides_typed() {
    let e = RpcError::from(&HandshakeError::ServerMismatch {
        running: info("fs-people", "0.0.2", 3),
        expected: Expected::new("0.0.3").with_program("fs-people"),
    });
    assert_eq!(e.message_key(), "rpc-error-contract-mismatch");
    assert_eq!(text(&e, "running-version"), Some("0.0.2"));
    assert_eq!(
        e.param("running-protocol").and_then(Param::as_number),
        Some(3)
    );
    assert_eq!(text(&e, "expected-version"), Some("0.0.3"));
    assert_eq!(text(&e, "expected-program"), Some("fs-people"));
    assert!(e.param("expected-protocol").is_some());
}

#[test]
fn a_replacement_that_fails_names_its_step() {
    let step = |cause: RestartCause| {
        let e = RpcError::from(&HandshakeError::CannotRestart {
            running: None,
            expected: Expected::new(""),
            cause: Box::new(cause),
        });
        (e.code(), text(&e, "step").map(str::to_owned), e)
    };
    let (code, s, _) = step(RestartCause::NotStarted(io::Error::other("x")));
    assert_eq!(
        (code, s.as_deref()),
        (RpcErrorCode::CannotRestart, Some("start"))
    );
    let (_, s, e) = step(RestartCause::CannotTerminate {
        pid: 77,
        source: io::Error::other("x"),
    });
    assert_eq!(s.as_deref(), Some("terminate"));
    assert_eq!(e.param("pid").and_then(Param::as_number), Some(77));
    assert_eq!(step(RestartCause::NotEnded).1.as_deref(), Some("not-ended"));
    assert_eq!(
        step(RestartCause::StopRefused(io::Error::other("x")))
            .1
            .as_deref(),
        Some("stop")
    );
    assert_eq!(
        step(RestartCause::CannotAsk(io::Error::other("x")))
            .1
            .as_deref(),
        Some("ask")
    );
    assert_eq!(
        step(RestartCause::NewUnreachable(io::Error::other("x")))
            .1
            .as_deref(),
        Some("reach")
    );
    let (_, _, e) = step(RestartCause::NotStarted(io::Error::from(
        io::ErrorKind::PermissionDenied,
    )));
    assert_eq!(text(&e, "kind"), Some("permission-denied"));
    let (code, s, _) = step(RestartCause::TooOldToStop);
    assert_eq!((code, s), (RpcErrorCode::TooOldToStop, None));
    let (code, s, e) = step(RestartCause::OtherProgram {
        found: "a".into(),
        wanted: "b".into(),
    });
    assert_eq!((code, s), (RpcErrorCode::OtherProgram, None));
    assert_eq!(e.message_key(), "rpc-error-other-program");
}

#[test]
fn a_handshake_io_error_goes_by_its_kind() {
    let e = RpcError::from(&HandshakeError::Io(io::Error::from(
        io::ErrorKind::PermissionDenied,
    )));
    assert_eq!(e.message_key(), "rpc-error-socket-unreachable");
}

#[test]
fn faults_become_codes_with_server_and_limit() {
    let gone = RpcError::from(&Fault::Gone { server: "people" });
    assert_eq!(gone.message_key(), "rpc-error-connection-lost");
    assert_eq!(text(&gone, "server"), Some("people"));
    let cut = RpcError::from(&Fault::Interrupted { server: "people" });
    assert_eq!(cut.message_key(), "rpc-error-interrupted");
    let late = RpcError::from(&Fault::TimedOut {
        server: "people",
        limit: Duration::from_millis(1500),
    });
    assert_eq!(late.message_key(), "rpc-error-timeout");
    assert_eq!(text(&late, "server"), Some("people"));
    assert_eq!(
        late.param("limit-ms").and_then(Param::as_number),
        Some(1500)
    );
}

#[test]
fn problems_become_codes_and_their_extensions_become_parameters() {
    let big = RpcError::of(&too_large_by("x", 9000, 4096));
    assert_eq!(big.message_key(), "rpc-error-too-large");
    assert_eq!(big.param("size").and_then(Param::as_number), Some(9000));
    assert_eq!(big.param("limit").and_then(Param::as_number), Some(4096));
    let bad = RpcError::of(&fs_rpc::invalid_request("no id"));
    assert_eq!(bad.message_key(), "rpc-error-invalid-request");
    let no = RpcError::of(&fs_rpc::unsupported(&fs_rpc_unsupported(
        "board.open",
        "later",
    )));
    assert_eq!(no.message_key(), "rpc-error-unimplemented");
    assert_eq!(text(&no, "capability"), Some("board.open"));
    // An older server sends no extension: its sentence is never read for the capability.
    let old = RpcError::of(
        &Problem::new(
            "https://freesynergy.net/problems/unsupported",
            "Not possible",
            "board.open: later",
            501,
        )
        .into_error(),
    );
    assert_eq!(old.code(), RpcErrorCode::Unimplemented);
    assert!(old.param("capability").is_none());
    let other = RpcError::of(
        &Problem::new(
            "https://freesynergy.net/problems/people/not-found",
            "T",
            "d",
            404,
        )
        .with_ext("id", "0190-abc")
        .into_error(),
    );
    assert_eq!(other.message_key(), "rpc-error-protocol");
    assert_eq!(text(&other, "type"), Some("people/not-found"));
    assert_eq!(text(&other, "id"), Some("0190-abc"));
}

fn fs_rpc_unsupported(capability: &str, reason: &str) -> fs_interfaces::provider::Unsupported {
    fs_interfaces::provider::Unsupported::new(capability, reason)
}

#[test]
fn plain_rpc_errors_go_by_their_kind() {
    let lost = RpcError::of(&capnp::Error::disconnected("x".into()));
    assert_eq!(lost.code(), RpcErrorCode::ConnectionLost);
    let old = RpcError::of(&capnp::Error::unimplemented("m".into()));
    assert_eq!(old.code(), RpcErrorCode::Unimplemented);
    let odd = RpcError::of(&capnp::Error::failed("whatever the peer said".into()));
    assert_eq!(odd.code(), RpcErrorCode::Protocol);
    assert!(
        odd.params().next().is_none(),
        "the peer's text is never a parameter"
    );
}

#[test]
fn display_is_a_code_text_and_no_sentence() {
    let e = RpcError::new(RpcErrorCode::NoServer)
        .with_text("socket", "/run/x.sock")
        .with_text("server", "people");
    assert_eq!(
        e.to_string(),
        "rpc.error: no-server (socket=/run/x.sock, server=people)"
    );
    assert_eq!(
        RpcError::new(RpcErrorCode::Timeout).to_string(),
        "rpc.error: timeout"
    );
    let spaced = RpcError::new(RpcErrorCode::Io)
        .with_text("x", "a b")
        .with_number("n", 3);
    assert_eq!(spaced.to_string(), "rpc.error: io (x=\"a b\", n=3)");
    for code in RpcErrorCode::ALL {
        let t = RpcError::new(code).to_string();
        // Exactly one word after the prefix, the name of the code: no sentence.
        assert_eq!(t, format!("rpc.error: {}", code.as_str()));
    }
}

#[test]
fn retryable_codes() {
    assert!(RpcError::new(RpcErrorCode::NoServer).is_retryable());
    assert!(RpcError::new(RpcErrorCode::Timeout).is_retryable());
    assert!(!RpcError::new(RpcErrorCode::ContractMismatch).is_retryable());
    assert!(!RpcError::new(RpcErrorCode::InvalidRequest).is_retryable());
}
