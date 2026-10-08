// SPDX-License-Identifier: GPL-2.0-only
//! Lid policy; no userspace-configurable sensor or firmware target.
#![allow(dead_code)]
#[derive(Default)]
pub(crate) struct LidState {
    pub(crate) open: Option<bool>,
    pub(crate) changes: u64,
    pub(crate) attempts: u8,
    pub(crate) error: i32,
    pending: Option<u64>,
    initialized: bool,
}

impl LidState {
    pub(crate) fn observe(&mut self, result: i32, now: u64) -> bool {
        let open = match result {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        let changed = !self.initialized || self.open != open;
        if changed {
            self.initialized = true;
            self.open = open;
            self.changes = self.changes.saturating_add(1);
            self.request(now);
        }
        changed
    }

    pub(crate) fn allowed(&self) -> bool {
        self.open == Some(true)
    }
    pub(crate) fn name(&self) -> &'static str {
        match self.open {
            Some(true) => "open",
            Some(false) => "closed",
            None => "unknown",
        }
    }
    pub(crate) fn request(&mut self, now: u64) {
        self.attempts = 0;
        self.error = 0;
        self.pending = Some(now);
    }
    pub(crate) fn ready(&self, now: u64) -> bool {
        self.pending.is_some_and(|due| now >= due)
    }
    pub(crate) fn deadline(&self) -> Option<u64> {
        self.pending
    }
    pub(crate) fn completed(&mut self, now: u64, error: i32) {
        self.attempts = self.attempts.saturating_add(1);
        self.error = error;
        self.pending = if error != 0 && self.attempts < 3 {
            Some(now.saturating_add(2_000_000_000))
        } else {
            None
        };
    }
}

pub(crate) fn controlled(name: &str) -> bool {
    matches!(
        name,
        "keyboard_effect" | "keyboard_timeout" | "turbo_led" | "turbo_led_mode"
    )
}
