#[path = "../src/control.rs"]
mod control;
#[path = "../src/interface.rs"]
mod interface;
#[path = "../src/sysfs_codec.rs"]
mod sysfs_codec;
use interface::*;

#[test]
fn all_defined_keyboard_values_round_trip() {
    for value in ["off", "static FF8C00", "static FFB000 FF8C00 FF4D00 B3001B brightness=100",
        "breathing brightness=80 speed=4", "wave brightness=100 speed=9 direction=left",
        "zones static FF0000 brightness=20 ; off ; neon brightness=30 speed=2 ; static 00FF00 brightness=90"] {
        let setting = Keyboard::parse(value.as_bytes()).unwrap();
        let canonical = setting.to_string();
        assert!(canonical.len() < MAX_TEXT);
        assert!(setting.writable());
        assert_eq!(Keyboard::parse(canonical.as_bytes()), Some(setting), "{canonical}");
    }
    for mode in [4, 5, 7, 8, 9, 10, 11] {
        for brightness in [0, 100] {
            for speed in [0, 9] {
                let setting = Keyboard::from_light(control::Light {
                    mode,
                    brightness,
                    speed,
                    direction: 1,
                    zones: 15,
                    rgb: [0; 3],
                });
                assert_eq!(
                    Keyboard::parse(setting.to_string().as_bytes()),
                    Some(setting)
                );
            }
        }
    }
}
#[test]
fn undefined_is_not_a_setting_and_partial_writes_do_not_invent_zones() {
    let empty = Keyboard::parse(b"undefined").unwrap();
    assert!(!empty.writable());
    assert_eq!(empty.to_string(), "undefined");
    let partial = Keyboard::parse(b"zones undefined ; static FF0000 ; undefined ; off").unwrap();
    assert!(partial.writable());
    assert!(partial.0[0].is_none() && partial.0[2].is_none());
    assert_eq!(partial.0[1].unwrap().zones, 2);
    assert_eq!(
        Keyboard::parse(partial.to_string().as_bytes()),
        Some(partial)
    );
}
#[test]
fn whole_request_must_validate_before_any_firmware_work() {
    for value in [
        "static",
        "static FF0000 00FF00",
        "static FF0000 00FF00 0000FF",
        "static FF0000 brightness=101",
        "static FF0000 brightness=50 brightness=60",
        "static FF0000 speed=2",
        "neon FF0000",
        "neon speed=10",
        "wave direction=native",
        "off brightness=1",
        "static FF0000 raw=a4",
        "zones off ; off ; off",
        "zones off ; off ; off ; off ; off",
        "zones off ; off ; off ; static nope",
        "zones static FF0000 00FF00 0000FF FFFFFF ; off ; off ; off",
        "static FF0000\0",
        "opmode",
    ] {
        assert!(Keyboard::parse(value.as_bytes()).is_none(), "{value}");
    }
    assert!(Keyboard::parse(&[b'x'; MAX_TEXT + 1]).is_none());
    assert!(Keyboard::parse(&[0xff]).is_none());
}
#[test]
fn logo_and_turbo_round_trip_and_turbo_preserves_manual_color() {
    for value in [
        "off",
        "static FF8C00 brightness=100",
        "breathing brightness=25 speed=5",
        "neon brightness=0 speed=1",
    ] {
        let setting = Cover::parse(value.as_bytes()).unwrap();
        assert_eq!(
            Cover::parse(Cover(setting).to_string().as_bytes()),
            Some(setting)
        );
    }
    for bad in [
        "undefined",
        "wave",
        "neon speed=0",
        "breathing FF0000",
        "static FF0000 00FF00 0000FF FFFFFF",
    ] {
        assert!(Cover::parse(bad.as_bytes()).is_none());
    }
    let remembered = control::TurboLed::Rgb([179, 0, 27]);
    let auto = Turbo::parse(b"auto", remembered).unwrap();
    assert_eq!(auto.to_string(), "auto manual=B3001B");
    assert_eq!(
        Turbo::parse(auto.to_string().as_bytes(), control::TurboLed::Off),
        Some(auto)
    );
    assert_eq!(
        Turbo::parse(b"manual", auto.manual).unwrap().setting,
        remembered
    );
    assert!(Turbo::parse(b"static FF0000 extra", remembered).is_none());
}
#[test]
fn old_disk_records_and_cli_packets_use_new_paths_only() {
    for (logical, actual) in [
        ("profile", "performance_mode"),
        ("keyboard_effect", "keyboard_lighting"),
        ("keyboard_state", "keyboard_lighting"),
        ("turbo_led", "turbo_lighting"),
        ("turbo_led_mode", "turbo_lighting"),
        ("turbo_led_manual", "turbo_lighting"),
        ("logo", "cover_lighting"),
        ("sleep_logo", "sleep_cover_lighting"),
        ("mux_mode", "gpu_mux_mode"),
    ] {
        assert_eq!(sysfs_codec::attribute(logical), actual);
    }
    let public = sysfs_codec::encode("keyboard_effect", "static 100 0 none 2 FF8C00").unwrap();
    assert_eq!(
        sysfs_codec::decode("keyboard_state", &public).unwrap(),
        "static 100 0 none 2 FF8C00"
    );
    assert_eq!(sysfs_codec::encode("keyboard_timeout", "on").unwrap(), "30");
    assert_eq!(sysfs_codec::decode("profile", "eco").unwrap(), "eco");
    assert_eq!(
        sysfs_codec::decode("fan_control", "manual 50 60").unwrap(),
        "3 3"
    );
    assert_eq!(
        sysfs_codec::encode("logo", "static 100 FF8C00").unwrap(),
        "static FF8C00 brightness=100"
    );
}
