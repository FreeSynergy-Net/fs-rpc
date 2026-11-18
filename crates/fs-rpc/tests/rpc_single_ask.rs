//! `ensure_single_with` against a real server that holds the socket and has no heartbeat channel (`no-control`): does the failure ask
//! the person? One test covers every path with the same child: with a surface and the setting on it is a question (`can-ask` and the
//! choices `wait`, `replace`, `abort` as parameters), without a surface it is not, with the setting off it is not.
//!
//! Pattern: Table of options over one hanging child (no signal is sent for `no-control`, so the child serves every row).
#![cfg(target_os = "linux")]

use std::time::Duration;

use fs_interfaces::common_capnp::blob;
use fs_rpc::{
    AskPolicy, Choices, RpcError, RpcErrorCode, SingleError, SingleOptions, Stuck,
    ensure_single_with,
};

mod handshake_support;
mod single_support;
use handshake_support::*;
use single_support::*;

#[test]
fn helper_hanging_server() {
    hanging_server_main();
}

fn failure(socket: &std::path::Path, options: SingleOptions) -> Stuck {
    let starter = starter(socket);
    let rt = runtime();
    let result = tokio::task::LocalSet::new().block_on(&rt, async {
        ensure_single_with::<blob::Client>(socket, &expected(), &starter, &times(), None, &options)
            .await
            .map(|_| ())
    });
    let Err(SingleError::Unresponsive(stuck)) = result else {
        panic!("expected a silent server, got {:?}", result.err());
    };
    let coded = RpcError::from(&stuck);
    assert_eq!(coded.code(), RpcErrorCode::ServerUnresponsive);
    Stuck::from_error(&coded).expect("reads back")
}

#[test]
fn a_person_is_asked_with_a_surface_and_the_setting_on_never_without_one_or_with_it_off() {
    let _one = ONE_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let _limit = watchdog(90);
    let (_dir, socket, mut child, _) = setup("silent", false);

    // Surface and setting on (the default): a question, with the three choices as parameters.
    let asked = failure(&socket, SingleOptions::new(AskPolicy::Ask, true));
    assert_eq!(asked.why, "no-control");
    assert!(asked.can_ask);
    assert_eq!(asked.choices.tokens(), ["wait", "replace", "abort"]);

    // Headless (no surface): the program decides.
    let headless = failure(&socket, SingleOptions::new(AskPolicy::Ask, false));
    assert_eq!(headless.why, "no-control");
    assert!(!headless.can_ask && headless.choices == Choices::None);

    // The setting off, with a surface: the program decides.
    let off = failure(&socket, SingleOptions::new(AskPolicy::from(false), true));
    assert_eq!(off.why, "no-control");
    assert!(!off.can_ask && off.choices == Choices::None);

    // The default options ask where there is a surface; the rest of the failure is the same in every row.
    assert_eq!((asked.pid, asked.limit), (headless.pid, headless.limit));
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        alive(&mut child),
        "no signal for no-control, whoever is asked"
    );
}
