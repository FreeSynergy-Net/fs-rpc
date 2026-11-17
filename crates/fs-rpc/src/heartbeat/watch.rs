//! When is a program hanging? A pure rule over what the pings said, with made-up times.
//!
//! Pattern: State machine over time (the clock is an argument, so every case is a test without waiting). Hanging means: no heartbeat
//! for `no_beat`, or no progress for `no_progress` (a busy program whose number stands still; a program that beats but does
//! not answer its contract for that long). A program that is busy and moving is never hanging.

use std::time::{Duration, Instant};

use super::{Beat, Pulse};

/// The limits (they can be set; the defaults are the plan).
#[derive(Clone, Copy, Debug)]
pub struct HangTimes {
    /// How often to ask.
    pub ping: Duration,
    /// Hanging after this long without any answer.
    pub no_beat: Duration,
    /// Hanging after this long without progress (`Busy` with a number that stands still, or `Alive` while the contract is silent).
    /// 90 s: a program may stand still for a load of 60 s during a self-update.
    pub no_progress: Duration,
    /// Hanging when the work thread's tick is older than this while the program does not say `Busy` (the heartbeat thread
    /// answers, the work thread is blocked).
    pub no_tick: Duration,
}

impl Default for HangTimes {
    fn default() -> Self {
        Self {
            ping: Duration::from_secs(2),
            no_beat: Duration::from_secs(10),
            no_progress: Duration::from_secs(90),
            no_tick: Duration::from_secs(10),
        }
    }
}

/// Why a program counts as hanging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hung {
    /// No heartbeat in `no_beat`.
    NoBeat,
    /// No progress in `no_progress`.
    NoProgress,
    /// The heartbeat answers but the work thread's tick stands still for `no_tick`, and the program does not say busy.
    NoTick,
}

/// What the pings say so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// No answer yet, but not long enough to say anything.
    Waiting,
    /// It beats, and is not busy.
    Alive,
    /// It is busy and moving: never to be ended; the person is told what it does.
    Busy {
        /// What it does (text key).
        what: String,
        /// The last progress number.
        progress: u64,
    },
    /// It hangs.
    Hung(Hung),
}

/// The record of the pings of one silent program.
#[derive(Debug)]
pub struct Watch {
    times: HangTimes,
    began: Instant,
    last_beat: Instant,
    progress: Option<(u64, Instant)>,
}

impl Watch {
    /// A record that starts at `now`, when the program was first found silent.
    #[must_use]
    pub fn new(times: HangTimes, now: Instant) -> Self {
        Self {
            times,
            began: now,
            last_beat: now,
            progress: None,
        }
    }

    /// Take one ping: `pulse` is the answer, or `None` if there was none in time.
    pub fn observe(&mut self, now: Instant, pulse: Option<&Pulse>) -> Verdict {
        let since = |t: Instant| now.saturating_duration_since(t);
        match pulse.map(|p| (&p.beat, p.tick_age)) {
            None => {
                if since(self.last_beat) >= self.times.no_beat {
                    Verdict::Hung(Hung::NoBeat)
                } else {
                    Verdict::Waiting
                }
            }
            Some((Beat::Alive, tick_age)) => {
                self.last_beat = now;
                self.progress = None;
                // The tick must have stood still for the whole watch: the watch itself lasted `no_tick` and the last tick is older than that.
                // So the contract falling silent for 2 s never ends anything, however old the program is.
                if since(self.began) >= self.times.no_tick
                    && tick_age.is_some_and(|age| age >= self.times.no_tick)
                {
                    Verdict::Hung(Hung::NoTick)
                } else if since(self.began) >= self.times.no_progress {
                    Verdict::Hung(Hung::NoProgress)
                } else {
                    Verdict::Alive
                }
            }
            Some((Beat::Busy { what, progress }, _)) => {
                self.last_beat = now;
                let moved_at = match self.progress {
                    Some((p, at)) if p == *progress => at,
                    _ => now,
                };
                self.progress = Some((*progress, moved_at));
                if since(moved_at) >= self.times.no_progress {
                    Verdict::Hung(Hung::NoProgress)
                } else {
                    Verdict::Busy {
                        what: what.clone(),
                        progress: *progress,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
