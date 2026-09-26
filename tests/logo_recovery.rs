//! The allocation-free Eco policy used by the kernel worker.
#[path = "../src/control.rs"]
mod control;
#[path = "../src/logo_recovery.rs"]
mod logo_recovery;
use control::Logo;
use logo_recovery::{LogoRecovery, SETTLE_NS};
const ORANGE: Logo = Logo::Static {
    brightness: 100,
    rgb: [255, 140, 0],
};

#[test]
fn eco_waits_and_uses_latest_setting_including_off_and_animations() {
    let mut state = LogoRecovery::default();
    assert_eq!(state.setting(u64::MAX, Some(ORANGE), false), None);
    state.profile_changed(Some(6), 100);
    assert_eq!(
        state.setting(100 + SETTLE_NS - 1, Some(ORANGE), false),
        None
    );
    assert_eq!(state.setting(100 + SETTLE_NS, None, false), None);
    for latest in [
        ORANGE,
        Logo::Off,
        Logo::Breathing {
            brightness: 40,
            speed: 2,
        },
        Logo::Neon {
            brightness: 80,
            speed: 5,
        },
    ] {
        assert_eq!(
            state.setting(100 + SETTLE_NS, Some(latest), false),
            Some(latest)
        );
    }
    state.completed(100 + SETTLE_NS, true);
    assert_eq!(state.attempts, 1);
    assert_eq!(state.setting(u64::MAX, Some(ORANGE), false), None);
}

#[test]
fn mode_changes_cancel_or_restart_the_settling_window() {
    let mut state = LogoRecovery::default();
    for profile in [None, Some(0), Some(1), Some(4), Some(5), Some(255)] {
        state.profile_changed(Some(6), 0);
        state.profile_changed(profile, 1);
        assert_eq!(state.setting(u64::MAX, Some(ORANGE), false), None);
    }
    state.profile_changed(Some(6), 0);
    state.profile_changed(Some(6), SETTLE_NS);
    assert_eq!(state.setting(SETTLE_NS, Some(ORANGE), false), None);
    assert_eq!(
        state.setting(2 * SETTLE_NS, Some(ORANGE), false),
        Some(ORANGE)
    );
}

#[test]
fn sleep_and_wake_restore_own_rgb_and_suspend_cancels_recovery() {
    let mut state = LogoRecovery::default();
    state.profile_changed(Some(6), 0);
    assert_eq!(state.setting(u64::MAX, Some(ORANGE), true), None);
    state.cancel();
    assert_eq!(state.setting(u64::MAX, Some(ORANGE), false), None);
    assert_eq!(state.attempts, 0);
}

#[test]
fn errors_retry_twice_then_stop_and_new_events_reset_attempts() {
    let mut state = LogoRecovery::default();
    state.profile_changed(Some(6), 0);
    let first = SETTLE_NS;
    assert_eq!(state.setting(first, Some(ORANGE), false), Some(ORANGE));
    state.completed(first, false);
    assert_eq!(
        state.setting(first + SETTLE_NS - 1, Some(ORANGE), false),
        None
    );
    let second = first + SETTLE_NS;
    assert_eq!(state.setting(second, Some(ORANGE), false), Some(ORANGE));
    state.completed(second, false);
    let third = second + 5_000_000_000;
    assert_eq!(state.setting(third - 1, Some(ORANGE), false), None);
    assert_eq!(state.setting(third, Some(ORANGE), false), Some(ORANGE));
    state.completed(third, false);
    assert_eq!(state.attempts, 3);
    assert_eq!(state.setting(u64::MAX, Some(ORANGE), false), None);
    state.profile_changed(Some(6), third);
    assert_eq!(state.attempts, 0);
    assert_eq!(
        state.setting(third + SETTLE_NS, Some(ORANGE), false),
        Some(ORANGE)
    );
}

#[test]
fn deadline_saturates_instead_of_overflowing() {
    let mut state = LogoRecovery::default();
    state.profile_changed(Some(6), u64::MAX - 1);
    assert_eq!(state.setting(u64::MAX - 1, Some(ORANGE), false), None);
    assert_eq!(state.setting(u64::MAX, Some(ORANGE), false), Some(ORANGE));
    state.completed(u64::MAX, false);
    assert_eq!(state.setting(u64::MAX, Some(ORANGE), false), Some(ORANGE));
}
