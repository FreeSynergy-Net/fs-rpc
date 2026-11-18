//! The start time a failure of `ensure_single` tells (`start` in ticks, `since` as UTC text).

/// The failure says when the silent process started (`start` in ticks, `since` as UTC text): a process of this test, a moment ago.
pub fn assert_started_just_now(coded: &fs_rpc::RpcError) {
    assert!(
        coded
            .param("start")
            .and_then(fs_rpc::Param::as_number)
            .is_some()
    );
    let since = coded
        .param("since")
        .and_then(fs_rpc::Param::as_text)
        .expect("since");
    let at = chrono::DateTime::parse_from_rfc3339(since)
        .expect("RFC 3339")
        .timestamp();
    let now = chrono::Utc::now().timestamp();
    assert!(
        (now - 600..=now + 2).contains(&at),
        "{since} is not a start within the last minutes"
    );
}
