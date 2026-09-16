//! Pure tests of the policy used by the kernel PM callback and worker.
#[path = "../src/control.rs"]
mod control;
#[path = "../src/sleep_state.rs"]
mod sleep_state;
use sleep_state::{keyboard_reports, SleepState, SETTLE_NS};

#[test]
fn sleep_blocks_restore_and_post_waits_two_seconds() {
    let mut state = SleepState::default();
    assert_eq!(state.phase(), "awake");
    assert!(!state.resume(0));
    assert!(!state.ready(u64::MAX));
    assert!(state.prepare());
    assert!(!state.prepare());
    assert_eq!(state.cycles, 1);
    assert_eq!(state.phase(), "sleeping");
    assert!(!state.ready(u64::MAX));
    assert!(state.resume(100));
    assert!(!state.resume(200));
    assert_eq!(state.phase(), "restoring");
    assert!(!state.ready(100 + SETTLE_NS - 1));
    assert!(state.ready(100 + SETTLE_NS));
    state.completed(100 + SETTLE_NS, 0);
    assert_eq!(state.phase(), "awake");
    assert!(!state.ready(u64::MAX));
    assert_eq!(state.attempts, 1);
}

#[test]
fn errors_retry_three_times_then_stop_without_losing_error() {
    let mut state = SleepState::default();
    state.prepare();
    state.resume(0);
    let mut time = SETTLE_NS;
    for delay in [2_000_000_000, 5_000_000_000] {
        assert!(state.ready(time));
        state.completed(time, -5);
        assert!(!state.ready(time + delay - 1));
        time += delay;
    }
    assert!(state.ready(time));
    state.completed(time, -19);
    assert_eq!(state.attempts, 3);
    assert_eq!(state.error, -19);
    assert_eq!(state.phase(), "error");
    assert!(!state.ready(u64::MAX));
    assert!(state.prepare());
    assert_eq!(state.error, 0);
    assert_eq!(state.attempts, 0);
    assert!(state.resume(time));
    assert!(state.ready(time + SETTLE_NS));
}

#[test]
fn new_suspend_cancels_pending_restore_and_abort_uses_same_post() {
    let mut state = SleepState::default();
    state.prepare();
    state.resume(0);
    state.prepare();
    assert!(!state.ready(u64::MAX));
    assert_eq!(state.cycles, 2);
    state.resume(10);
    assert!(!state.ready(SETTLE_NS));
    assert!(state.ready(SETTLE_NS + 10));
    state.completed(SETTLE_NS + 10, 0);
    assert_eq!(state.phase(), "awake");
}

#[test]
fn native_animation_restores_in_one_report_without_modifying_cache() {
    let setting = control::light(b"breathing 60 3 native 15 000000").unwrap();
    let zones = core::array::from_fn(|zone| {
        Some(control::Light {
            zones: 1 << zone,
            ..setting
        })
    });
    let original = zones;
    assert_eq!(keyboard_reports(zones), [Some(setting), None, None, None]);
    assert_eq!(zones, original);
}

#[test]
fn four_static_colors_and_unknown_zones_are_preserved() {
    let zones = [
        Some(control::light(b"static 100 0 none 1 FFB000").unwrap()),
        Some(control::light(b"static 100 0 none 2 FF8C00").unwrap()),
        Some(control::light(b"static 100 0 none 4 FF4D00").unwrap()),
        Some(control::light(b"static 100 0 none 8 B3001B").unwrap()),
    ];
    assert_eq!(keyboard_reports(zones), zones);
    assert_eq!(keyboard_reports([None; 4]), [None; 4]);
    assert_eq!(
        keyboard_reports([None, zones[1], None, zones[3]]),
        [zones[1], zones[3], None, None]
    );
}

#[test]
fn latest_frame_and_settings_are_used_after_resume_not_a_sleep_snapshot() {
    let old = control::light(b"static 60 0 none 15 0000FF").unwrap();
    let new = control::light(b"static 40 0 none 15 FF0000").unwrap();
    let mut zones = [Some(old); 4];
    let mut state = SleepState::default();
    state.prepare();
    state.resume(0);
    zones[1] = Some(new);
    let reports = keyboard_reports(zones);
    assert_eq!(reports[0], Some(control::Light { zones: 13, ..old }));
    assert_eq!(reports[1], Some(control::Light { zones: 2, ..new }));
}
