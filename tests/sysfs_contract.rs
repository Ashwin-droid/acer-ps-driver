//! Live tests bypass the CLI and exercise the kernel's own input boundary.
use std::{fs, io::Write, path::Path, thread, time::Duration};
const ROOT: &str = "/sys/class/misc/acer-phn16s";

#[test]
#[ignore = "requires physically closed lid and installed APS driver"]
fn closed_lid_rejects_rgb_without_changing_cached_intent() {
    assert_eq!(read("lid_state").trim(), "closed");
    let generation = read("preferences_generation");
    let before = [
        read("keyboard_lighting"),
        read("turbo_lighting"),
        read("turbo_lighting"),
        read("cover_lighting"),
    ];
    for (name, value) in [
        ("keyboard_lighting", "static FF00FF brightness=60"),
        ("turbo_lighting", "static FF00FF"),
        ("turbo_lighting", "auto"),
        ("keyboard_backlight_timeout_seconds", "30"),
    ] {
        let error = write(name, value.as_bytes()).expect_err("lid gate accepted lighting write");
        assert_eq!(error.raw_os_error(), Some(112), "{name}: {error}");
    }
    assert_eq!(read("preferences_generation"), generation);
    assert_eq!(
        [
            read("keyboard_lighting"),
            read("turbo_lighting"),
            read("turbo_lighting"),
            read("cover_lighting")
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
#[ignore = "requires open lid and known lighting; reapplies only the current values"]
fn lighting_reads_can_be_written_back_to_the_same_files() {
    assert_eq!(read("lid_state").trim(), "open");
    let before = [
        read("keyboard_lighting"),
        read("turbo_lighting"),
        read("cover_lighting"),
    ];
    assert!(before.iter().all(|v| !v.contains("undefined")));
    let sleep = read("sleep_cover_lighting");
    let profile = read("performance_mode");
    let fans = read("fan_control");
    for (name, value) in ["keyboard_lighting", "turbo_lighting", "cover_lighting"]
        .iter()
        .zip(&before)
    {
        assert_eq!(read(name), *value, "setting changed while preparing test");
        write(name, value.as_bytes()).unwrap();
        assert_eq!(read(name), *value);
    }
    assert_eq!(
        before,
        [
            read("keyboard_lighting"),
            read("turbo_lighting"),
            read("cover_lighting")
        ]
    );
    assert_eq!(read("sleep_cover_lighting"), sleep);
    assert_eq!(read("performance_mode"), profile);
    assert_eq!(read("fan_control"), fans);
}

#[test]
#[ignore = "requires installed APS sysfs driver and normal-user access"]
fn kernel_rejects_invalid_requests_without_changing_control_state() {
    let cases: [(&str, &[&[u8]]); 8] = [
        (
            "cover_lighting",
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
                b"static FF0000 brightness=101",
                b"breathing brightness=60 speed=0",
            ],
        ),
        (
            "turbo_button_mode",
            &[b"hotkey F15", b"F14", b"power modes extra", b"hotkey\0"],
        ),
        (
            "turbo_lighting",
            &[
                b"FFFFFF extra",
                b"GGGGGG",
                b"FFFFFF00",
                b"0xFFFFFF",
                b"FFFFFF\0",
            ],
        ),
        (
            "turbo_lighting",
            &[b"auto extra", b"rainbow", b"manual FF00FF", b"auto\0"],
        ),
        (
            "performance_mode",
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
            "keyboard_lighting",
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
                b"undefined",
                b"static FF0000 brightness=101",
                b"static FF0000 00FF00",
                b"static FF0000 brightness=30 brightness=40",
                b"zones off ; off ; off ; raw a4 21",
                b"zones off ; off ; off ; off ; off",
            ],
        ),
        (
            "keyboard_backlight_timeout_seconds",
            &[b"31", b"on", b"off\0", b"-1", b"\xff\xfe"],
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
        "profile",
        "keyboard_effect",
        "keyboard_state",
        "keyboard_timeout",
        "keyboard_capabilities",
        "turbo_led",
        "turbo_led_mode",
        "turbo_led_manual",
        "logo",
        "sleep_logo",
        "fan_lease_ms",
        "button_mode",
        "button_presses",
        "button_error",
        "sense_presses",
        "mux_mode",
        "mux_request",
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
        read("gpu_mux_request").trim(),
        "none",
        "this test must not request a GPU switch"
    );
    assert!(write("gpu_mux_request", b"auto accepted").is_err());
    assert_eq!(
        write("gpu_mux_mode", b"not-a-mode")
            .unwrap_err()
            .raw_os_error(),
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
            write("sleep_cover_lighting", self.0.as_bytes()).unwrap();
        }
    }
    let _restore = Restore(read("sleep_cover_lighting"));
    let awake_logo = read("cover_lighting");
    let keyboard = read("keyboard_lighting");
    let turbo = read("turbo_lighting");
    let pm = read("sleep_status");
    let lid = read("lid_state");
    for value in [
        "off",
        "static 0000FF brightness=25",
        "breathing brightness=25 speed=3",
        "neon brightness=25 speed=5",
    ] {
        write("sleep_cover_lighting", value.as_bytes()).unwrap();
        assert_eq!(read("sleep_cover_lighting").trim(), value);
        assert_eq!(read("cover_lighting"), awake_logo);
        assert_eq!(read("keyboard_lighting"), keyboard);
        assert_eq!(read("turbo_lighting"), turbo);
        assert_eq!(read("sleep_status"), pm);
        assert_eq!(read("lid_state"), lid);
    }
    let before = read("sleep_cover_lighting");
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
            write("sleep_cover_lighting", value.as_bytes())
                .unwrap_err()
                .raw_os_error(),
            Some(22)
        );
    }
    assert_eq!(
        write("sleep_cover_lighting", &[b'x'; 129])
            .unwrap_err()
            .raw_os_error(),
        Some(22)
    );
    assert_eq!(read("sleep_cover_lighting"), before);
    assert_eq!(read("preferences_generation"), generation);
}

#[test]
#[ignore = "tests Maximum and manual for six seconds each after CLI exit, then restores the original setting"]
fn fans_persist_after_cli_exit_without_a_lease() {
    struct Restore(String);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = write("fan_control", self.0.as_bytes());
        }
    }
    let _restore = Restore(read("fan_control"));
    for (args, expected) in [
        (&["fans", "maximum"][..], "maximum"),
        (&["fans", "manual", "55", "65"][..], "manual 55 65"),
    ] {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_aps"))
            .args(args)
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("fan CLI did not exit");
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(read("fan_control").trim(), expected);
        assert!(!Path::new(ROOT).join("fan_lease_ms").exists());
        thread::sleep(Duration::from_secs(6));
        assert_eq!(read("fan_control").trim(), expected);
    }
}
