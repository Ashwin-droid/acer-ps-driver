#[path = "../src/control.rs"]
mod control;
#[path = "../src/event_policy.rs"]
mod event_policy;
#[path = "../src/lid_state.rs"]
mod lid_state;
#[path = "../src/logo_recovery.rs"]
#[allow(dead_code)]
mod logo_recovery;
#[path = "../src/sleep_state.rs"]
#[allow(dead_code)]
mod sleep_state;
use event_policy::deadline;

#[test]
fn idle_and_sleep_do_not_arm_timers() {
    assert_eq!(deadline(false, None, None, true, None, true), None);
    assert_eq!(deadline(false, None, None, false, None, false), None);
    assert_eq!(deadline(true, Some(1), Some(2), false, Some(3), true), None);
}
#[test]
fn earliest_permitted_deadline_wins_without_postponing_existing_work() {
    assert_eq!(
        deadline(false, None, Some(5), false, Some(2), true),
        Some(2)
    );
    assert_eq!(deadline(false, None, Some(1), true, Some(2), true), Some(1));
    assert_eq!(
        deadline(false, Some(8), Some(2), false, Some(1), true),
        Some(2)
    );
}
#[test]
fn blocked_restore_does_not_create_a_busy_loop() {
    assert_eq!(
        deadline(false, Some(8), Some(0), true, Some(0), true),
        Some(8)
    );
    assert_eq!(deadline(false, None, None, true, Some(0), false), None);
    assert_eq!(deadline(false, None, Some(0), true, Some(0), true), Some(0));
}
#[test]
fn lid_event_retries_stop_and_the_next_event_can_start_again() {
    let mut lid = lid_state::LidState::default();
    lid.observe(0, 0);
    for now in [0, 2_000_000_000, 4_000_000_000] {
        assert_eq!(
            deadline(false, None, lid.deadline(), lid.allowed(), None, false),
            Some(now)
        );
        lid.completed(now, -5);
    }
    assert_eq!(
        deadline(false, None, lid.deadline(), lid.allowed(), None, false),
        None
    );
    lid.observe(1, 10_000_000_000);
    assert_eq!(lid.deadline(), Some(10_000_000_000));
    lid.completed(10_000_000_000, 0);
    assert_eq!(lid.deadline(), None);
}
#[test]
fn suspend_wake_and_eco_share_one_bounded_deadline() {
    let mut sleep = sleep_state::SleepState::default();
    let mut eco = logo_recovery::LogoRecovery::default();
    eco.profile_changed(Some(6), 0);
    assert_eq!(
        deadline(false, None, None, true, eco.deadline(), true),
        Some(2_000_000_000)
    );
    sleep.prepare();
    eco.cancel();
    assert_eq!(
        deadline(
            sleep.sleeping,
            sleep.deadline(),
            Some(0),
            false,
            eco.deadline(),
            true
        ),
        None
    );
    sleep.resume(10_000_000_000);
    assert_eq!(
        deadline(false, sleep.deadline(), Some(0), true, None, true),
        Some(12_000_000_000)
    );
    sleep.completed(12_000_000_000, 0);
    assert_eq!(
        deadline(false, sleep.deadline(), None, true, None, true),
        None
    );
}
#[test]
fn driver_source_has_no_periodic_key_or_lid_worker() {
    let driver = include_str!("../kernel/acer_phn16s.rs");
    for forbidden in [
        "kthread_create",
        "kthread_should_stop",
        "msleep(250)",
        "polls_after",
    ] {
        assert!(
            !driver.contains(forbidden),
            "reintroduced polling: {forbidden}"
        );
    }
    let events = include_str!("../kernel/events.rs");
    assert!(events.contains("queue_work_on"));
    assert!(events.contains("system_unbound_wq"));
    assert!(!events.contains("system_percpu_wq"));
    assert!(events.contains("cancel_delayed_work_sync"));
    assert!(events.contains("cancel_work_sync"));
    assert!(include_str!("../kernel/turbo_button.rs").contains("crate::events::queue()"));
    assert!(include_str!("../kernel/sense_key.rs").contains("return false; // Preserve delivery"));
}
