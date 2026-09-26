//! Live tests bypass the CLI and exercise the kernel's own input boundary.
use std::{fs, io::Write, path::Path, thread, time::Duration};
const ROOT: &str = "/sys/class/misc/acer-phn16s";

#[test]
#[ignore = "requires physically closed lid and installed APS driver"]
fn closed_lid_rejects_rgb_without_changing_cached_intent() {
    assert_eq!(read("lid_state").trim(), "closed");
    let generation = read("preferences_generation");
    let before = [
        read("keyboard_state"),
        read("turbo_led"),
        read("turbo_led_mode"),
        read("logo"),
    ];
    for (name, value) in [
        ("keyboard_effect", "static 60 0 none 15 FF00FF"),
        ("turbo_led", "FF00FF"),
        ("turbo_led_mode", "auto"),
        ("keyboard_timeout", "on"),
    ] {
        let error = write(name, value.as_bytes()).expect_err("lid gate accepted lighting write");
        assert_eq!(error.raw_os_error(), Some(112), "{name}: {error}");
    }
    assert_eq!(read("preferences_generation"), generation);
    assert_eq!(
        [
            read("keyboard_state"),
            read("turbo_led"),
            read("turbo_led_mode"),
            read("logo")
        ],
        before
    );
    assert!(write("lid_state", b"open").is_err());
    assert!(write("lid_status", b"open").is_err());
}

fn read(name: &str) -> String {
    fs::read_to_string(Path::new(ROOT).join(name)).unwrap()
}
fn write(name: &str, data: &[u8]) -> std::io::Result<()> {
    fs::OpenOptions::new()
        .write(true)
        .open(Path::new(ROOT).join(name))?
        .write_all(data)
}

#[test]
#[ignore = "requires installed APS sysfs driver and normal-user access"]
fn kernel_rejects_invalid_requests_without_changing_control_state() {
    let cases: [(&str, &[&[u8]]); 8] = [
        (
            "logo",
            &[
                b"raw a4 83",
                b"shifting 60 3",
                b"static 101 FF0000",
                b"static 60 GGGGGG",
                b"static 60 FF0000 21",
                b"neon 60 6",
                b"breathing 60 0",
                b"off extra",
                b"opmode 60 1",
                b"opmode 0",
                b"opmode 60",
                b"auto",
                b"static 60 FF0000\0",
                b"reapply extra",
                b"reapply\0",
            ],
        ),
        (
            "button_mode",
            &[b"hotkey F15", b"F14", b"power modes extra", b"hotkey\0"],
        ),
        (
            "turbo_led",
            &[
                b"FFFFFF extra",
                b"GGGGGG",
                b"FFFFFF00",
                b"0xFFFFFF",
                b"FFFFFF\0",
            ],
        ),
        (
            "turbo_led_mode",
            &[b"auto extra", b"rainbow", b"manual FF00FF", b"auto\0"],
        ),
        (
            "profile",
            &[
                b"balanced extra",
                b"turbo; reboot",
                b"0x16 0x0b",
                b"raw",
                b"2",
                b"balanced\0",
                b"LCD_OVERDRIVE",
            ],
        ),
        (
            "fan_control",
            &[
                b"manual -1 50",
                b"manual 39 100",
                b"manual 100 101",
                b"maximum extra",
                b"manual 50 50\0",
            ],
        ),
        (
            "keyboard_effect",
            &[
                b"raw a4 21",
                b"opmode 50 0 none 15 FFFFFF",
                b"static 101 0 none 15 FF0000",
                b"static 50 0 none 16 FF0000",
                b"static 50 0 none 15 FF0000 extra",
                b"static 50 0 none 15 GGGGGG",
                b"breathing 60 3 255 15 FFFFFF",
                b"breathing 60 3 right 15 FFFFFF",
                b"shifting 60 3 left 15 FF0000",
                b"static 60 0 native 15 FF0000",
                b"breathing 60 3 fixed 16 FFFFFF",
                b"breathing 60 3 fixed 15 FFFFFF",
                b"static 60 0 fixed 15 FFFFFF",
                b"wave 60 3 cycle 15 FFFFFF",
            ],
        ),
        (
            "keyboard_timeout",
            &[b"30", b"on extra", b"off\0", b"-1", b"\xff\xfe"],
        ),
    ];
    for (name, requests) in cases {
        let before = read(name);
        for request in requests {
            let error = write(name, request).expect_err("kernel accepted malformed input");
            assert_eq!(
                error.raw_os_error(),
                Some(22),
                "{name}: {request:?}: {error}"
            );
        }
        let error = write(name, &[b'1'; 129]).expect_err("kernel accepted oversized input");
        assert_eq!(error.raw_os_error(), Some(22));
        assert_eq!(read(name), before, "malformed input changed {name}");
    }
    for name in [
        "raw",
        "acpi",
        "ioctl",
        "memory",
        "port",
        "hid_report",
        "auth_key",
        "emit_key",
        "button_press",
        "preferences_save",
        "preferences_status",
        "sleep_event",
        "suspend",
        "pm_trigger",
    ] {
        assert!(
            !Path::new(ROOT).join(name).exists(),
            "unexpected generic endpoint {name}"
        );
    }
    assert_eq!(
        read("mux_request").trim(),
        "none",
        "this test must not request a GPU switch"
    );
    assert!(write("mux_request", b"auto accepted").is_err());
    assert_eq!(
        write("mux_mode", b"not-a-mode").unwrap_err().raw_os_error(),
        Some(13)
    );
    let sleep = read("sleep_status");
    assert!(sleep.starts_with("awake "), "unexpected PM state: {sleep}");
    assert!(sleep.contains("cycles=") && sleep.contains("attempts=") && sleep.contains("error=0"));
    assert!(
        write("sleep_status", b"sleeping").is_err(),
        "PM status must be read-only"
    );
}

#[test]
#[ignore = "requires the updated live module; changes sleep preferences only, never suspends"]
fn sleep_preferences_are_bounded_and_do_not_change_awake_lighting() {
    struct Restore(String);
    impl Drop for Restore {
        fn drop(&mut self) {
            write("sleep_logo", self.0.as_bytes()).unwrap();
        }
    }
    let _restore = Restore(read("sleep_logo"));
    let awake_logo = read("logo");
    let keyboard = read("keyboard_state");
    let turbo = read("turbo_led");
    let pm = read("sleep_status");
    let lid = read("lid_state");
    for value in ["off", "static 25 0000FF", "breathing 25 3", "neon 25 5"] {
        write("sleep_logo", value.as_bytes()).unwrap();
        assert_eq!(read("sleep_logo").trim(), value);
        assert_eq!(read("logo"), awake_logo);
        assert_eq!(read("keyboard_state"), keyboard);
        assert_eq!(read("turbo_led"), turbo);
        assert_eq!(read("sleep_status"), pm);
        assert_eq!(read("lid_state"), lid);
    }
    let before = read("sleep_logo");
    let generation = read("preferences_generation");
    for value in [
        "opmode 25",
        "reapply",
        "raw a4 83",
        "static 101 0000FF",
        "static 25 0000FF extra",
        "static 25 0000FF\0",
        "neon 25 0",
        "off extra",
    ] {
        assert_eq!(
            write("sleep_logo", value.as_bytes())
                .unwrap_err()
                .raw_os_error(),
            Some(22)
        );
    }
    assert_eq!(
        write("sleep_logo", &[b'x'; 129])
            .unwrap_err()
            .raw_os_error(),
        Some(22)
    );
    assert_eq!(read("sleep_logo"), before);
    assert_eq!(read("preferences_generation"), generation);
}

#[test]
#[ignore = "briefly enables Maximum, then tests kernel lease expiry without any CLI process"]
fn kernel_restores_auto_when_userspace_stops_renewing() {
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = write("fan_control", b"auto");
        }
    }
    let _restore = Restore;
    write("fan_control", b"maximum").unwrap();
    assert_eq!(read("fan_control").trim(), "2 2");
    let lease: u64 = read("fan_lease_ms").trim().parse().unwrap();
    assert!(lease > 0 && lease <= 5000);
    thread::sleep(Duration::from_secs(6));
    assert_eq!(read("fan_control").trim(), "1 1");
    assert_eq!(read("fan_lease_ms").trim(), "0");
}
