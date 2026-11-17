//! The waits of reconnecting: a short pause that grows to a ceiling, and a deadline for the whole attempt.
//!
//! Pattern: Value objects. Nothing here waits for ever: every step has its limit, and the whole try ends at `give_up`.

use std::time::Duration;

use crate::RestartTimes;

/// The pause between two tries: the steps in order, the last one repeated (the ceiling).
#[derive(Clone, Debug)]
pub struct Backoff {
    steps: Vec<Duration>,
    next: usize,
}

impl Backoff {
    /// A backoff over `steps` (an empty list means one second, always).
    #[must_use]
    pub fn new(steps: &[Duration]) -> Self {
        Self {
            steps: steps.to_vec(),
            next: 0,
        }
    }

    /// The next pause; after the last step it stays at the last one.
    pub fn next_pause(&mut self) -> Duration {
        let pause = self
            .steps
            .get(self.next)
            .or(self.steps.last())
            .copied()
            .unwrap_or(Duration::from_secs(1));
        self.next = self.next.saturating_add(1);
        pause
    }

    /// Start again with the first step (after a success).
    pub fn reset(&mut self) {
        self.next = 0;
    }
}

/// The time limits of [`keep_connected`](super::keep_connected). The defaults are for a program: tries after 1 s, 2 s, then every 5 s,
/// for 30 s; then the program is told.
#[derive(Clone, Debug)]
pub struct ReconnectTimes {
    /// How long one try (connect, handshake, subscribe) may take at most.
    pub attempt: Duration,
    /// The pauses between tries (the last one repeats: the ceiling).
    pub backoff: Vec<Duration>,
    /// After this long without success the program is told and the loop ends (it may start it again).
    pub give_up: Duration,
    /// How often a live connection is asked whether the server still answers.
    pub ping: Duration,
    /// How long the server gets to answer a ping.
    pub ping_limit: Duration,
    /// The waits of the handshake with the server (and its restart through the program's starter).
    pub handshake: RestartTimes,
}

impl Default for ReconnectTimes {
    fn default() -> Self {
        Self {
            attempt: Duration::from_secs(10),
            backoff: vec![
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(5),
            ],
            give_up: Duration::from_secs(30),
            ping: Duration::from_secs(2),
            ping_limit: Duration::from_secs(2),
            handshake: RestartTimes {
                call: Duration::from_secs(2),
                stop: Duration::from_secs(5),
                start: Duration::from_secs(5),
            },
        }
    }
}
