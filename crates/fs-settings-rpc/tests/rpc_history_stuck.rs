//! The livelock guard of the log: a read that is cut AND empty is `history-cut` on the wire; the client counts the same pointer and
//! answers `history-stuck` after `LIVELOCK_MAX`; `HistoryPage::cut` reaches the caller.

mod support;

use fs_settings::{InstanceStore, Raw};
use fs_settings_rpc::LIVELOCK_MAX;
use support::{HIDDEN, admin, code, key, nobody, text, world};

/// Cut + empty page + the same pointer: the wire turns it into `history-cut`, the client counts, the last one is `history-stuck`.
#[test]
fn a_cut_empty_page_for_the_same_pointer_ends_in_history_stuck() {
    let w = world(false);
    for i in 0..fs_settings::MAX_LINES_READ + 4 {
        w.store
            .write_instance(&key(HIDDEN), Some(&Raw::Bool(i % 2 == 1)), &nobody())
            .unwrap();
    }
    let person = admin(&w.person);
    // lines 0 and 1 are the two seeds (the second is the visible one); after it only hidden lines follow
    let lines = person.instance_log(None, 5);
    let (after_open, later) = (Some(lines[1].id), Some(lines[3].id));
    let bot = admin(&w.bot);
    assert!(
        bot.instance_log(after_open, 10).is_empty(),
        "the port form: an empty page, no loop"
    );
    let cut = |p| {
        let page = bot.instance_log_page(p, 10).unwrap();
        assert!(page.cut && page.lines.is_empty());
    };
    let stuck = |p| bot.instance_log_page(p, 10).unwrap_err();
    for _ in 1..LIVELOCK_MAX {
        cut(after_open);
    }
    let e = stuck(after_open);
    assert_eq!(
        (code(&e), text(&e, "cause")),
        ("store-broken", Some("history-stuck"))
    );
    // another pointer starts the count again
    for _ in 1..LIVELOCK_MAX {
        cut(later);
    }
    // so does an answer that is no cut (the visible seed line)
    assert_eq!(bot.instance_log_page(None, 10).unwrap().lines.len(), 1);
    for _ in 1..LIVELOCK_MAX {
        cut(later);
    }
    assert_eq!(text(&stuck(later), "cause"), Some("history-stuck"));
    let page = person.instance_log_page(None, 10).unwrap();
    assert!(
        !page.cut && page.lines.len() == 10,
        "a person reads the log whole"
    );
}

/// The texts of the code exist in German and English (`store-broken{cause}`, the code of the module).
#[test]
fn the_stuck_code_has_its_text_in_both_languages() {
    for texts in [fs_settings::TEXT_DE, fs_settings::TEXT_EN] {
        assert!(texts.contains("setting-error-store-broken"));
    }
}
