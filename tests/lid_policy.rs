#[path = "../src/lid_state.rs"]
mod lid_state;
use lid_state::LidState;

#[test]
fn closed_at_boot_is_detected_without_an_input_event() {
    let mut lid = LidState::default();
    assert!(!lid.allowed());
    assert!(lid.observe(0, 1));
    assert_eq!(lid.name(), "closed");
    assert!(lid.ready(1));
    lid.completed(1, 0);
    assert!(!lid.observe(0, 2));
    assert!(!lid.ready(u64::MAX));
    assert_eq!(lid.polls_after, 500_000_002);
}

#[test]
fn open_close_open_queues_only_transitions_and_restores_on_open() {
    let mut lid = LidState::default();
    assert!(lid.observe(1, 0));
    assert!(lid.allowed());
    lid.completed(0, 0);
    assert!(!lid.observe(1, 1));
    assert!(!lid.ready(1));
    assert!(lid.observe(0, 2));
    assert!(!lid.allowed());
    assert!(lid.ready(2));
    lid.completed(2, 0);
    assert!(lid.observe(1, 3));
    assert!(lid.allowed());
    assert!(lid.ready(3));
    assert_eq!(lid.changes, 3);
}

#[test]
fn sensor_failure_is_not_interpreted_as_open() {
    let mut lid = LidState::default();
    lid.observe(1, 0);
    for raw in [-19, -5, 2, i32::MAX, i32::MIN] {
        lid.observe(raw, 1);
        assert!(!lid.allowed());
        assert_eq!(lid.name(), "unknown");
    }
    assert!(lid.observe(1, 2));
    assert!(lid.allowed());
}

#[test]
fn retries_are_bounded_and_new_lid_state_replaces_stale_work() {
    let mut lid = LidState::default();
    lid.observe(0, 0);
    for now in [0, 2_000_000_000, 4_000_000_000] {
        assert!(lid.ready(now));
        lid.completed(now, -5);
        assert!(!lid.ready(now));
    }
    assert!(!lid.ready(u64::MAX));
    assert_eq!(lid.error, -5);
    lid.observe(1, 5_000_000_000);
    assert_eq!(lid.attempts, 0);
    assert!(lid.ready(5_000_000_000));
    lid.completed(5_000_000_000, 0);
    assert_eq!(lid.error, 0);
}

#[test]
fn profile_overlay_schedules_delayed_off_without_new_lid_event() {
    let mut lid = LidState::default();
    lid.observe(0, 0);
    lid.completed(0, 0);
    lid.request(2_000_000_000);
    assert!(!lid.observe(0, 1));
    assert!(!lid.ready(1_999_999_999));
    assert!(lid.ready(2_000_000_000));
}

#[test]
fn only_internal_lighting_is_gated() {
    for name in [
        "keyboard_effect",
        "keyboard_timeout",
        "turbo_led",
        "turbo_led_mode",
    ] {
        assert!(lid_state::controlled(name));
    }
    for name in [
        "logo",
        "sleep_logo",
        "profile",
        "fan_control",
        "button_mode",
        "lid_state",
    ] {
        assert!(!lid_state::controlled(name));
    }
}
