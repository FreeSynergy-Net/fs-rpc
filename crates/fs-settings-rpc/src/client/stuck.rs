//! The guard against a read of the log that never gets on: the service answers `history-cut` (read limit reached, nothing to show) and the
//! same pointer comes back again. [`Stuck`] counts those answers in a row for the same pointer; the [`LIVELOCK_MAX`]-th is `history-stuck`.
//!
//! Pattern: State (the last cut pointer and how many times in a row).

use std::sync::{Mutex, PoisonError};

use fs_settings::{Coded, SettingCode};
use fs_types::ObjectId;

use crate::problem::{HISTORY_STUCK, broken_with};

/// How many `history-cut` answers in a row for the SAME pointer a reader takes; the last of them is `history-stuck` (a named limit, not a
/// setting: a run of hidden lines does not get shorter by asking again).
pub const LIVELOCK_MAX: u32 = 3;

#[derive(Default)]
pub(crate) struct Stuck(Mutex<Option<(Option<ObjectId>, u32)>>);

impl Stuck {
    /// A cut, empty page came for `after`.
    ///
    /// # Errors
    /// `store-broken{cause=history-stuck}` when it is the [`LIVELOCK_MAX`]-th in a row for this pointer.
    pub(crate) fn cut(&self, after: Option<ObjectId>) -> Result<(), Coded<SettingCode>> {
        let mut last = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let n = match *last {
            Some((p, n)) if p == after => n.saturating_add(1),
            _ => 1,
        };
        *last = Some((after, n));
        if n >= LIVELOCK_MAX {
            Err(broken_with(HISTORY_STUCK))
        } else {
            Ok(())
        }
    }

    /// An answer that is no cut: the read moves, the count starts again.
    pub(crate) fn moved(&self) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}
