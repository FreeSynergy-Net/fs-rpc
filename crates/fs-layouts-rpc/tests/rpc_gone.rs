//! No service: every call says `unreachable{socket}` (never an empty answer), and the link is made again when the service comes.
#![allow(clippy::many_single_char_names, clippy::too_many_lines)]
mod support;

use std::time::Duration;

use fs_forms_layout::*;
use support::*;

#[test]
fn no_service_says_unreachable_with_the_socket_and_a_later_service_is_found() {
    let d = dir();
    let socket = d.path().join("l.sock");
    run(async {
        let c = client(&socket);
        let e = c.stamp().await.unwrap_err();
        assert_eq!(
            (code(&e), param(&e, "socket")),
            ("unreachable", Some(socket.to_str().unwrap()))
        );
        let e = c.list(&LayoutFilter::default(), None, 5).await.unwrap_err();
        assert_eq!(code(&e), "unreachable");
        // inside the retry pause the answer comes without a new try; after it the service is found
        let s = person(&socket);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(c.stamp().await.is_ok());
        // the service goes away: the next call says so, the one after mends itself
        drop(s);
        let e = c.stamp().await.unwrap_err();
        assert_eq!(code(&e), "unreachable");
        let _again = person(&socket);
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(c.stamp().await.is_ok());
    });
}

#[test]
fn connect_fails_with_the_code_when_nothing_listens() {
    let d = dir();
    run(async {
        let e = fs_layouts_rpc::RpcLayouts::connect(&d.path().join("none.sock"), times())
            .await
            .map(drop)
            .unwrap_err();
        assert_eq!(code(&e), "unreachable");
    });
}
