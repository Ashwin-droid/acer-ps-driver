//! Shared, allocation-free sysfs grammar. The kernel calls this before any hardware I/O.
#![allow(dead_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fans {
    Auto,
    Maximum,
    Manual(u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonMode {
    PowerModes,
    Hotkey,
}
pub(crate) fn button_mode(bytes: &[u8]) -> Option<ButtonMode> {
    match text(bytes)?.trim() {
        "power modes" | "power-modes" => Some(ButtonMode::PowerModes),
        "hotkey" => Some(ButtonMode::Hotkey),
        _ => None,
    }
}
pub(crate) fn next_profile(current: u64, on_ac: bool) -> Option<u8> {
    let order: &[u8] = if on_ac { &[0, 1, 4, 5, 6] } else { &[0, 1, 6] };
    if !matches!(current, 0 | 1 | 4 | 5 | 6) {
        return None;
    }
    let position = order.iter().position(|&mode| mode as u64 == current);
    match position {
        Some(index) => order.get((index + 1) % order.len()).copied(),
        None => Some(0),
    }
}
pub(crate) fn turbo_event(notification: u32, event_type: u8, function: u64, pressed: u64) -> bool {
    notification == 0xbc && event_type == 7 && function == 5 && pressed == 1
}
pub(crate) fn turbo_hid_report(report: &[u8]) -> bool {
    report == [0x04, 0x85, 0xff]
}
pub(crate) fn sense_hid_report(report: &[u8]) -> bool {
    report == [0x04, 0x81, 0xff]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TurboLed {
    Auto,
    Off,
    Rgb([u8; 3]),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Logo {
    Off,
    Static { brightness: u8, rgb: [u8; 3] },
    Breathing { brightness: u8, speed: u8 },
    Neon { brightness: u8, speed: u8 },
    OperatingMode { brightness: u8 },
}
impl Logo {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Off => true,
            Self::Static { brightness, .. } | Self::OperatingMode { brightness } => {
                brightness <= 100
            }
            Self::Breathing { brightness, speed } | Self::Neon { brightness, speed } => {
                brightness <= 100 && (1..=5).contains(&speed)
            }
        }
    }
}
impl core::fmt::Display for Logo {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Off => out.write_str("off"),
            Self::Static { brightness, rgb } => write!(
                out,
                "static {} {:02X}{:02X}{:02X}",
                brightness, rgb[0], rgb[1], rgb[2]
            ),
            Self::Breathing { brightness, speed } => {
                write!(out, "breathing {} {}", brightness, speed)
            }
            Self::Neon { brightness, speed } => write!(out, "neon {} {}", brightness, speed),
            Self::OperatingMode { brightness } => write!(out, "opmode {}", brightness),
        }
    }
}
pub(crate) fn logo(bytes: &[u8]) -> Option<Logo> {
    let mut parts = text(bytes)?.split_ascii_whitespace();
    let kind = parts.next()?;
    if kind == "off" {
        return parts.next().is_none().then_some(Logo::Off);
    }
    let brightness = number(parts.next()?)?;
    if brightness > 100 {
        return None;
    }
    let brightness = brightness as u8;
    let setting = match kind {
        "static" => match turbo_led(parts.next()?.as_bytes())? {
            TurboLed::Rgb(rgb) => Logo::Static { brightness, rgb },
            _ => return None,
        },
        "breathing" | "neon" => {
            let speed = number(parts.next()?)?;
            if !(1..=5).contains(&speed) {
                return None;
            }
            if kind == "breathing" {
                Logo::Breathing {
                    brightness,
                    speed: speed as u8,
                }
            } else {
                Logo::Neon {
                    brightness,
                    speed: speed as u8,
                }
            }
        }
        "opmode" => Logo::OperatingMode { brightness },
        _ => return None,
    };
    (parts.next().is_none() && setting.valid()).then_some(setting)
}

// Sleep has no operating-mode flashes, software animations, or raw commands.
pub(crate) fn sleep_logo(bytes: &[u8]) -> Option<Logo> {
    match logo(bytes)? {
        Logo::OperatingMode { .. } => None,
        setting => Some(setting),
    }
}

// A stateful command, deliberately NOT a serializable Logo preference.
pub(crate) fn logo_reapply(bytes: &[u8]) -> bool {
    text(bytes).is_some_and(|value| value.trim() == "reapply")
}
pub(crate) fn turbo_led_mode(bytes: &[u8]) -> Option<bool> {
    match text(bytes)?.trim() {
        "auto" => Some(true),
        "manual" => Some(false),
        _ => None,
    }
}
pub(crate) fn turbo_led(bytes: &[u8]) -> Option<TurboLed> {
    let value = text(bytes)?.trim();
    if value == "off" {
        return Some(TurboLed::Off);
    }
    let value = value.strip_prefix('#').unwrap_or(value);
    if value.len() != 6 {
        return None;
    }
    let nibble = |b| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    let mut rgb = [0; 3];
    for (out, pair) in rgb.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        *out = nibble(*pair.first()?)? << 4 | nibble(*pair.get(1)?)?;
    }
    Some(TurboLed::Rgb(rgb))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Light {
    pub(crate) mode: u8,
    pub(crate) brightness: u8,
    pub(crate) speed: u8,
    pub(crate) direction: u8,
    pub(crate) zones: u16,
    pub(crate) rgb: [u8; 3],
}
pub(crate) fn valid_light(s: Light) -> bool {
    effect(s.mode).is_some()
        && s.brightness <= 100
        && s.speed <= 9
        && (1..=15).contains(&s.zones)
        && (!matches!(s.mode, 1 | 2) || s.speed == 0)
        && light_parameter(s.mode, s.direction).is_some()
}

// Captured Windows animations use byte 5 = 1. It is not a fixed-color selector.
// Preserve legacy none/0 records while new callers use the captured native value.
pub(crate) fn light_parameter(mode: u8, direction: u8) -> Option<&'static str> {
    match (mode, direction) {
        (4 | 5 | 8..=11, 1) => Some("native"),
        (7, 1) => Some("right"),
        (7, 2) => Some("left"),
        (1 | 2 | 4 | 5 | 8..=11, 0) => Some("none"),
        _ => None,
    }
}

fn text(bytes: &[u8]) -> Option<&str> {
    if bytes.is_empty()
        || bytes.len() > 128
        || !bytes
            .iter()
            .all(|&b| b.is_ascii_graphic() || matches!(b, b' ' | b'\t' | b'\n'))
    {
        return None;
    }
    core::str::from_utf8(bytes).ok()
}

fn number(value: &str) -> Option<u16> {
    if value.is_empty() || value.len() > 5 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

pub(crate) fn profile(bytes: &[u8]) -> Option<u8> {
    match text(bytes)?.trim() {
        "quiet" => Some(0),
        "balanced" => Some(1),
        "performance" => Some(4),
        "turbo" => Some(5),
        "eco" => Some(6),
        _ => None,
    }
}

pub(crate) fn fans(bytes: &[u8]) -> Option<Fans> {
    let mut parts = text(bytes)?.split_ascii_whitespace();
    let setting = match parts.next()? {
        "auto" => Fans::Auto,
        "maximum" => Fans::Maximum,
        "manual" => {
            let cpu = number(parts.next()?)?;
            let gpu = number(parts.next()?)?;
            if !(40..=100).contains(&cpu) || !(40..=100).contains(&gpu) {
                return None;
            }
            Fans::Manual(cpu as u8, gpu as u8)
        }
        _ => return None,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(setting)
}

pub(crate) fn timeout(bytes: &[u8]) -> Option<bool> {
    match text(bytes)?.trim() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

pub(crate) fn effect(mode: u8) -> Option<&'static str> {
    match mode {
        1 => Some("off"),
        2 => Some("static"),
        4 => Some("breathing"),
        5 => Some("neon"),
        7 => Some("wave"),
        8 => Some("shifting"),
        9 => Some("zoom"),
        10 => Some("meteor"),
        11 => Some("twinkling"),
        _ => None,
    }
}

pub(crate) fn light(bytes: &[u8]) -> Option<Light> {
    let mut parts = text(bytes)?.split_ascii_whitespace();
    let mode = match parts.next()? {
        "off" => 1,
        "static" => 2,
        "breathing" => 4,
        "neon" => 5,
        "wave" => 7,
        "shifting" => 8,
        "zoom" => 9,
        "meteor" => 10,
        "twinkling" => 11,
        _ => return None,
    };
    let brightness = number(parts.next()?)?;
    let speed = number(parts.next()?)?;
    let direction = match parts.next()? {
        "none" => 0,
        "right" if mode == 7 => 1,
        "left" if mode == 7 => 2,
        "cycle" if mode == 4 => 0,
        "native" if matches!(mode, 4 | 5 | 8..=11) => 1,
        _ => return None,
    };
    let zones = number(parts.next()?)?;
    let color = parts.next()?;
    if parts.next().is_some()
        || brightness > 100
        || speed > 9
        || !(1..=15).contains(&zones)
        || color.len() != 6
        || !color.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    if matches!(mode, 1 | 2) && speed != 0 {
        return None;
    }
    light_parameter(mode, direction)?;
    let nibble = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    let mut rgb = [0u8; 3];
    for (output, pair) in rgb.iter_mut().zip(color.as_bytes().chunks_exact(2)) {
        *output = (nibble(*pair.first()?)? << 4) | nibble(*pair.get(1)?)?;
    }
    Some(Light {
        mode,
        brightness: brightness as u8,
        speed: speed as u8,
        direction,
        zones,
        rgb,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn button_controls_are_fixed_and_skip_ac_modes_on_battery() {
        assert!(turbo_hid_report(&[4, 0x85, 0xff]));
        assert!(sense_hid_report(&[4, 0x81, 0xff]));
        assert!(!sense_hid_report(&[4, 0x85, 0xff]));
        for report in [
            &[4, 0x81, 0xff][..],
            &[4, 0x89, 0xff],
            &[4, 0x8a, 0xff],
            &[4, 0x85, 0xff, 0],
            &[0x32, 0x85, 0xff],
            &[],
        ] {
            assert!(!turbo_hid_report(report));
        }
        assert_eq!(turbo_led(b"#FF0088\n"), Some(TurboLed::Rgb([255, 0, 136])));
        assert_eq!(turbo_led(b"off"), Some(TurboLed::Off));
        for invalid in [
            "",
            "0xFF0088",
            "FFFFFF extra",
            "FFFFFG",
            "FFFFFF00",
            "FFFFFF\0",
        ] {
            assert!(turbo_led(invalid.as_bytes()).is_none());
        }
        assert_eq!(button_mode(b"power modes\n"), Some(ButtonMode::PowerModes));
        assert_eq!(button_mode(b"hotkey"), Some(ButtonMode::Hotkey));
        for invalid in [
            "hotkey F15",
            "F14",
            "shell",
            "power modes extra",
            "hotkey\0",
        ] {
            assert!(button_mode(invalid.as_bytes()).is_none());
        }
        assert_eq!(
            [0, 1, 4, 5, 6].map(|p| next_profile(p, true)),
            [Some(1), Some(4), Some(5), Some(6), Some(0)]
        );
        assert_eq!(
            [0, 1, 6, 4, 5].map(|p| next_profile(p, false)),
            [Some(1), Some(6), Some(0), Some(0), Some(0)]
        );
        assert_eq!(next_profile(2, true), None);
        assert!(turbo_event(0xbc, 7, 5, 1));
        for event in [
            (0xbd, 7, 5, 1),
            (0xbc, 8, 5, 1),
            (0xbc, 7, 4, 1),
            (0xbc, 7, 5, 0),
        ] {
            assert!(!turbo_event(event.0, event.1, event.2, event.3));
        }
    }
    #[test]
    fn only_exact_named_operations_are_accepted() {
        for invalid in [
            "",
            "raw",
            "0x16 0x0b",
            "balanced; reboot",
            "balanced\0",
            "balanced extra",
            "LCD_OVERDRIVE",
        ] {
            assert!(profile(invalid.as_bytes()).is_none());
        }
        assert_eq!(profile(b"balanced\n"), Some(1));
        assert_eq!(timeout(b"on\n"), Some(true));
        assert!(timeout(b"30").is_none());
        assert!(timeout(b"on extra").is_none());
    }
    #[test]
    fn fans_reject_bad_lengths_signs_ranges_and_trailing_data() {
        for invalid in [
            "manual -1 50",
            "manual +50 50",
            "manual 39 50",
            "manual 50 101",
            "manual 65536 50",
            "manual 50",
            "manual 50 50 extra",
            "maximum 99",
            "auto\0",
        ] {
            assert!(fans(invalid.as_bytes()).is_none());
        }
        assert!(fans(&[b'1'; 129]).is_none());
        assert_eq!(fans(b"manual 40 100\n"), Some(Fans::Manual(40, 100)));
    }
    #[test]
    fn lighting_cannot_smuggle_reports_targets_or_unsupported_modes() {
        for invalid in [
            "a4 21 2 100 0",
            "opmode 50 0 none 15 000000",
            "static 101 0 none 15 FF0000",
            "static 50 0 none 16 FF0000",
            "static 50 0 none 0 FF0000",
            "static 50 0 none 15 FFFFFF extra",
            "static 50 0 none 15 GGGGGG",
            "static 50 1 none 15 FFFFFF",
            "wave 50 10 left 15 FFFFFF",
            "wave 50 3 none 15 FFFFFF",
            "neon 50 3 left 15 FFFFFF",
        ] {
            assert!(light(invalid.as_bytes()).is_none(), "accepted {invalid}");
        }
        assert_eq!(
            light(b"static 50 0 none 15 FF0080").unwrap().rgb,
            [255, 0, 128]
        );
        assert_eq!(light(b"wave 80 3 left 15 FFFFFF").unwrap().direction, 2);
    }
    #[test]
    fn arbitrary_bytes_never_panic_or_get_accepted_as_commands() {
        for n in 0..=255u8 {
            let bytes = [n; 128];
            assert!(profile(&bytes).is_none());
            assert!(fans(&bytes).is_none());
            assert!(light(&bytes).is_none());
            assert!(logo(&bytes).is_none());
        }
    }
    #[test]
    fn logo_grammar_is_bounded_and_canonical() {
        for text in [
            "off",
            "static 60 FF00FF",
            "static 0 000000",
            "breathing 100 1",
            "neon 60 5",
            "opmode 60",
        ] {
            let setting = logo(text.as_bytes()).unwrap();
            assert!(setting.valid());
            assert_eq!(logo(setting.to_string().as_bytes()), Some(setting));
        }
        for text in [
            "shifting 60 3",
            "static 101 FF0000",
            "static -1 FF0000",
            "static 60 off",
            "static 60 GGGGGG",
            "static 60 FF0000 extra",
            "breathing 60 0",
            "neon 60 6",
            "opmode 60 1",
            "off now",
            "a4 83 2",
            "breathing 60 3 FF0000",
        ] {
            assert!(logo(text.as_bytes()).is_none(), "{text}");
        }
        assert!(logo(&[b'1'; 129]).is_none());
    }
    #[test]
    fn breathing_parameters_are_bounded_and_round_trip() {
        for (name, value) in [("cycle", 0), ("none", 0), ("native", 1)] {
            let text = std::format!("breathing 60 3 {name} 15 0088FF");
            let parsed = light(text.as_bytes()).unwrap();
            assert!(valid_light(parsed));
            assert_eq!(parsed.direction, value);
            let canonical = light_parameter(parsed.mode, parsed.direction).unwrap();
            assert_eq!(
                light(std::format!("breathing 60 3 {canonical} 15 0088FF").as_bytes()),
                Some(parsed)
            );
        }
        for value in 0..=255 {
            assert_eq!(light_parameter(4, value).is_some(), matches!(value, 0 | 1));
        }
        for invalid in [
            "breathing 60 3 right 15 FFFFFF",
            "breathing 60 3 left 15 FFFFFF",
            "breathing 60 3 fixed 15 FFFFFF",
            "breathing 60 3 255 15 FFFFFF",
            "static 60 0 fixed 15 FFFFFF",
            "wave 60 3 cycle 15 FFFFFF",
        ] {
            assert!(light(invalid.as_bytes()).is_none());
        }
    }
    #[test]
    fn reapply_is_a_command_not_a_persistable_logo_value() {
        assert!(logo_reapply(b"reapply\n"));
        assert!(logo(b"reapply").is_none());
        for invalid in [
            b"reapply extra".as_slice(),
            b"reapply\0",
            b"REAPPLY",
            b"reapply FF0000",
        ] {
            assert!(!logo_reapply(invalid));
        }
    }
}
