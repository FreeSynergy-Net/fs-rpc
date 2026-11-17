//! What the client knows between two stamps. Pattern: Value object with the one rule "a new stamp drops everything".

use std::collections::HashMap;

use fs_settings::{Coded, Instance, ProfileSet, Scope, SettingCode, Snapshot, Stamp};

/// Where the service stands, as far as this client saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Reach {
    /// Nothing asked yet.
    #[default]
    Unknown,
    /// The last call was answered.
    Up,
    /// The last call was not answered.
    Down,
}

#[derive(Default)]
pub(crate) struct Cache {
    /// Counts every change of [`Reach`], so a program that polls `stamp` sees the health change even when the service's
    /// own stamp is equal to the last known one.
    epoch: u64,
    link: Reach,
    server: u64,
    pub(crate) snapshots: HashMap<(Scope, bool, u64), Snapshot>,
    pub(crate) index: Option<(ProfileSet<u64>, Vec<Coded<SettingCode>>)>,
    pub(crate) instance: Option<Instance>,
}

impl Cache {
    /// The stamp this client shows: the service's, with the epoch in the top byte.
    pub(crate) fn stamp(&self) -> Stamp {
        Stamp(self.server | ((self.epoch & 0xff) << 56))
    }

    /// The service answered with `stamp`: a different one drops the cache.
    pub(crate) fn seen(&mut self, stamp: u64) {
        self.set_link(Reach::Up);
        if stamp != self.server {
            self.server = stamp;
            self.forget();
        }
    }

    /// The service answered something (an error of its own is an answer too); the stamp stays.
    pub(crate) fn seen_up(&mut self) {
        self.set_link(Reach::Up);
    }

    /// The service did not answer: what was read stays (last known), the stamp moves once.
    pub(crate) fn lost(&mut self) {
        self.set_link(Reach::Down);
    }

    pub(crate) fn reachable(&self) -> bool {
        self.link != Reach::Down
    }

    fn set_link(&mut self, to: Reach) {
        if self.link != to {
            self.link = to;
            self.epoch += 1;
        }
    }

    fn forget(&mut self) {
        self.snapshots.clear();
        self.index = None;
        self.instance = None;
    }
}
