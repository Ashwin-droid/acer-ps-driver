// SPDX-License-Identifier: GPL-2.0-only
//! Allocation-free suspend policy, shared with host-side tests.
use crate::control::Light;

pub(crate) const SETTLE_NS: u64 = 2_000_000_000;
const RETRY_NS: [u64; 2] = [2_000_000_000, 5_000_000_000];

#[derive(Default)]
pub(crate) struct SleepState {
    pub(crate) sleeping: bool,
    pub(crate) cycles: u64,
    pub(crate) attempts: u8,
    pub(crate) error: i32,
    due: Option<u64>,
}

impl SleepState {
    pub(crate) fn prepare(&mut self) -> bool {
        if self.sleeping {
            return false;
        }
        self.sleeping = true;
        self.cycles = self.cycles.saturating_add(1);
        self.attempts = 0;
        self.error = 0;
        self.due = None;
        true
    }

    // Also used after a failed/aborted suspend. Unmatched POST events do nothing.
    pub(crate) fn resume(&mut self, now: u64) -> bool {
        if !self.sleeping {
            return false;
        }
        self.sleeping = false;
        self.due = Some(now.saturating_add(SETTLE_NS));
        true
    }

    pub(crate) fn ready(&self, now: u64) -> bool {
        !self.sleeping && self.due.is_some_and(|due| now >= due)
    }

    pub(crate) fn completed(&mut self, now: u64, error: i32) {
        self.attempts = self.attempts.saturating_add(1);
        self.error = error;
        self.due = if error == 0 {
            None
        } else {
            RETRY_NS
                .get(usize::from(self.attempts - 1))
                .map(|delay| now.saturating_add(*delay))
        };
    }

    pub(crate) fn phase(&self) -> &'static str {
        if self.sleeping {
            "sleeping"
        } else if self.due.is_some() {
            "restoring"
        } else if self.error != 0 {
            "error"
        } else {
            "awake"
        }
    }
}

// Merge identical zone settings into one report so a whole-keyboard native
// animation restarts in sync. Distinct static zone colors remain distinct.
pub(crate) fn keyboard_reports(zones: [Option<Light>; 4]) -> [Option<Light>; 4] {
    let mut reports: [Option<Light>; 4] = [None; 4];
    for (zone, setting) in zones.into_iter().enumerate() {
        if let Some(mut setting) = setting {
            setting.zones = 1 << zone;
            for entry in &mut reports {
                match entry {
                    Some(existing)
                        if Light {
                            zones: setting.zones,
                            ..*existing
                        } == setting =>
                    {
                        existing.zones |= setting.zones;
                        break;
                    }
                    None => {
                        *entry = Some(setting);
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    reports
}
