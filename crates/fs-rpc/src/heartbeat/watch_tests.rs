//! When a program counts as hanging, on made-up times: one test per rule.
//!
//! Pattern: Table tests with a clock that is an argument. Mutants (each turns its test red): M1 `Busy` is judged by the answer
//! alone, never by the progress (a busy program that stands still is never hanging); M2 `Busy` with moving progress is hanging;
//! M3 no answer is never hanging; M4 no answer is hanging at once (no `no_beat` limit); M5 `Alive` while the contract is silent
//! is never hanging; M7 the tick age is ignored (a blocked work thread with a live heartbeat counts as alive); M8 `Busy` is judged by the tick; M6 the progress time is reset by every busy answer, also when the number is the same.

use std::time::{Duration, Instant};

use super::{Beat, HangTimes, Hung, Pulse, Verdict, Watch};

fn times() -> HangTimes {
    HangTimes {
        ping: Duration::from_secs(2),
        no_beat: Duration::from_secs(10),
        no_progress: Duration::from_secs(90),
        no_tick: Duration::from_secs(10),
    }
}

fn busy(progress: u64) -> Pulse {
    Pulse {
        beat: Beat::Busy {
            what: "importing".into(),
            progress,
        },
        tick_age: None,
    }
}

fn alive(tick_s: Option<u64>) -> Pulse {
    Pulse {
        beat: Beat::Alive,
        tick_age: tick_s.map(Duration::from_secs),
    }
}

fn at(t0: Instant, s: u64) -> Instant {
    t0 + Duration::from_secs(s)
}

#[test]
fn no_answer_is_waiting_and_then_hanging_after_the_limit() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    assert_eq!(w.observe(at(t0, 2), None), Verdict::Waiting);
    assert_eq!(w.observe(at(t0, 9), None), Verdict::Waiting);
    assert_eq!(w.observe(at(t0, 10), None), Verdict::Hung(Hung::NoBeat));
}

#[test]
fn a_busy_program_that_moves_is_never_hanging_however_long() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    for s in (0..600).step_by(2) {
        let v = w.observe(at(t0, s), Some(&busy(s)));
        assert!(matches!(v, Verdict::Busy { .. }), "{s}: {v:?}");
    }
}

#[test]
fn a_busy_program_whose_number_stands_still_hangs_after_the_progress_limit() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    assert!(matches!(
        w.observe(t0, Some(&busy(5))),
        Verdict::Busy { .. }
    ));
    assert!(matches!(
        w.observe(at(t0, 89), Some(&busy(5))),
        Verdict::Busy { .. }
    ));
    assert_eq!(
        w.observe(at(t0, 90), Some(&busy(5))),
        Verdict::Hung(Hung::NoProgress)
    );
    // A move after a long standstill starts the count again.
    let mut w = Watch::new(times(), t0);
    let _ = w.observe(t0, Some(&busy(5)));
    assert!(matches!(
        w.observe(at(t0, 80), Some(&busy(6))),
        Verdict::Busy { .. }
    ));
    assert!(matches!(
        w.observe(at(t0, 160), Some(&busy(6))),
        Verdict::Busy { .. }
    ));
}

#[test]
fn a_program_that_beats_but_never_answers_its_contract_hangs_after_the_progress_limit() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    assert_eq!(w.observe(at(t0, 10), Some(&alive(None))), Verdict::Alive);
    assert_eq!(w.observe(at(t0, 89), Some(&alive(None))), Verdict::Alive);
    assert_eq!(
        w.observe(at(t0, 90), Some(&alive(None))),
        Verdict::Hung(Hung::NoProgress)
    );
}

#[test]
fn an_answer_resets_the_beat_limit() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    assert_eq!(w.observe(at(t0, 8), Some(&alive(None))), Verdict::Alive);
    assert_eq!(
        w.observe(at(t0, 16), None),
        Verdict::Waiting,
        "8 s since the last answer"
    );
    assert_eq!(w.observe(at(t0, 18), None), Verdict::Hung(Hung::NoBeat));
}

#[test]
fn a_heartbeat_that_answers_while_the_work_thread_stands_still_is_hanging_not_alive() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    assert_eq!(w.observe(at(t0, 2), Some(&alive(Some(1)))), Verdict::Alive);
    assert_eq!(w.observe(at(t0, 4), Some(&alive(Some(9)))), Verdict::Alive);
    assert_eq!(
        w.observe(at(t0, 6), Some(&alive(Some(10)))),
        Verdict::Hung(Hung::NoTick)
    );
}

#[test]
fn a_busy_program_is_not_judged_by_its_tick() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    let slow = Pulse {
        tick_age: Some(Duration::from_secs(60)),
        ..busy(1)
    };
    assert!(matches!(
        w.observe(at(t0, 2), Some(&slow)),
        Verdict::Busy { .. }
    ));
}

#[test]
fn a_program_that_never_ticks_is_not_judged_by_a_tick() {
    let t0 = Instant::now();
    let mut w = Watch::new(times(), t0);
    assert_eq!(w.observe(at(t0, 20), Some(&alive(None))), Verdict::Alive);
}
