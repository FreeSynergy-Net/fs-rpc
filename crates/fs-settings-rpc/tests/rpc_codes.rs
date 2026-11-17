//! KB9: code <-> problem, the one mapping.

use fs_rpc::Problem;
use fs_settings::SettingCode;
use fs_settings::coded_stub::{Coded, ErrorCode, Param};
use fs_settings_rpc::{from_problem, to_problem};

/// KB9: every code, with every kind of parameter (text, a text that looks like a number, numbers), survives the trip; a problem of
/// another type is no setting code.
#[test]
fn kb9_every_code_roundtrips_with_all_parameters() {
    for &code in SettingCode::ALL {
        let c = Coded::new(code)
            .with_text("key", "fs.look.style")
            .with_text("reason", "house style: calm")
            .with_text("name", "123")
            .with_number("line", 7)
            .with_number("found", 6)
            .with_number("known", 5)
            .with_text("socket", "/run/user/1000/freesynergy/settings.sock");
        let p = to_problem(&c);
        assert_eq!(p.problem_type, format!("setting-error-{}", code.as_str()));
        assert_eq!(
            p.extensions.len(),
            7,
            "{code:?}: parameters must all travel"
        );
        let back = from_problem(&p).unwrap();
        assert_eq!(back, c, "{code:?}");
        let names: Vec<&str> = back.params().map(|(k, _)| k).collect();
        assert_eq!(
            names,
            ["key", "reason", "name", "line", "found", "known", "socket"],
            "order"
        );
        assert!(
            matches!(back.params().find(|(k, _)| *k == "name").unwrap().1, Param::Text(t) if t == "123")
        );
    }
}

#[test]
fn kb9_foreign_problem_is_no_setting_code() {
    assert!(
        from_problem(&Problem::new(
            "https://freesynergy.net/problems/too-large",
            "x",
            "y",
            413
        ))
        .is_none()
    );
    assert!(from_problem(&Problem::new("setting-error-no-such-code", "x", "y", 422)).is_none());
}
