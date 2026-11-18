//! The one mapping between a code with parameters and a Problem (K86 round trip): type = message key, parameters in order, text
//! only; a problem of another type is no layout code.
use fs_forms_layout::{Coded, ErrorCode, LayoutCode};
use fs_layouts_rpc::{from_problem, to_problem};

#[test]
fn every_code_survives_the_trip_with_its_parameters_in_order() {
    for code in LayoutCode::ALL {
        let c = Coded::new(*code)
            .with("b", "2")
            .with("a", "1")
            .with("b", "again");
        let p = to_problem(&c);
        assert_eq!(
            p.problem_type,
            format!("layout-error-{}", code.as_str()),
            "type is the message key"
        );
        assert_eq!(
            (p.title.as_str(), p.detail.as_str()),
            (code.as_str(), code.as_str()),
            "no sentence"
        );
        assert_eq!(from_problem(&p), Some(c), "{}", code.as_str());
    }
}

#[test]
fn a_foreign_problem_is_no_layout_code() {
    let p = fs_rpc::Problem::new("setting-error-locked", "x", "x", 409);
    assert_eq!(from_problem(&p), None);
    let p = fs_rpc::Problem::new("layout-error-not-a-code", "x", "x", 422);
    assert_eq!(from_problem(&p), None);
}

#[test]
fn the_status_tells_generic_clients_what_kind() {
    let status = |c: LayoutCode| to_problem(&Coded::new(c)).status;
    assert_eq!(status(LayoutCode::RevChanged), 409);
    assert_eq!(status(LayoutCode::Unknown), 404);
    assert_eq!(status(LayoutCode::Unsupported), 501);
    assert_eq!(status(LayoutCode::Unreachable), 503);
    assert_eq!(status(LayoutCode::KeyInvalid), 422);
}
