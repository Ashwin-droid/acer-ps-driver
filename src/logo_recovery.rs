// SPDX-License-Identifier: GPL-2.0-only
//! Bounded recovery of the awake cover logo after Eco's firmware indication.
use crate::control::Logo;

pub(crate) const SETTLE_NS: u64 = 2_000_000_000;
const RETRY_NS: [u64; 2] = [2_000_000_000, 5_000_000_000];

#[derive(Default)]
pub(crate) struct LogoRecovery {
    due: Option<u64>,
    pub(crate) attempts: u8,
}

impl LogoRecovery {
    pub(crate) fn cancel(&mut self) {
        self.due = None;
        self.attempts = 0;
    }

    // Also called after a failed setter if firmware readback shows Eco, since
    // rollback can produce another indication. Unknown/other modes cancel.
    pub(crate) fn profile_changed(&mut self, actual: Option<u64>, now: u64) {
        self.cancel();
        if actual == Some(6) {
            self.due = Some(now.saturating_add(SETTLE_NS));
        }
    }

    // Never retain a color snapshot: the caller supplies the latest cache while
    // holding the same mutex as sysfs, PM and profile writes. Sleep owns RGB
    // until its wake restore has completed, including any bounded retries.
    pub(crate) fn setting(&self, now: u64, latest: Option<Logo>, sleep_busy: bool) -> Option<Logo> {
        if !sleep_busy && self.due.is_some_and(|due| now >= due) {
            latest
        } else {
            None
        }
    }

    pub(crate) fn completed(&mut self, now: u64, success: bool) {
        self.attempts = self.attempts.saturating_add(1);
        self.due = if success {
            None
        } else {
            RETRY_NS
                .get(usize::from(self.attempts.saturating_sub(1)))
                .map(|delay| now.saturating_add(*delay))
        };
    }
}
