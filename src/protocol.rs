// Protocol recovered from the supplied PredatorSense 5.1.514 binaries.
#![allow(dead_code)]

pub(crate) const GET_CPU_RPM: u32 = 0x8008_5001;
pub(crate) const GET_GPU_RPM: u32 = 0x8008_5002;
pub(crate) const GET_PROFILE: u32 = 0x8008_5003;
pub(crate) const SET_PROFILE: u32 = 0x4008_5004;
pub(crate) const GET_FANS: u32 = 0x8008_5005;
pub(crate) const SET_FANS: u32 = 0x4008_5006;
pub(crate) const SET_MANUAL: u32 = 0x4008_5007;

pub(crate) fn valid_profile(mode: u64) -> bool {
    matches!(mode, 0 | 1 | 4 | 5 | 6)
}

// WMI reports setter values after a mode is selected. A fresh boot can report
// raw 2 (observed on this BIOS with the user's Balanced boot default). This is
// NOT an allowed setter value. The kernel can select a documented mode from
// this state without replaying 2. ODV0/CAUM use a separate internal mapping.
pub(crate) fn profile_from_firmware(raw: u64) -> Option<u64> {
    valid_profile(raw).then_some(raw)
}

pub(crate) fn fan_behavior(mode: u8) -> Option<u64> {
    if !(1..=3).contains(&mode) {
        return None;
    }
    Some(9 | ((mode as u64) << 16) | ((mode as u64) << 22))
}

pub(crate) fn fan_speed(gpu: bool, percent: u8) -> Option<u64> {
    if !(40..=100).contains(&percent) {
        return None;
    }
    Some((if gpu { 4 } else { 1 }) | ((percent as u64) << 8))
}

pub(crate) fn keyboard_report(
    mode: u8,
    brightness: u8,
    speed: u8,
    rgb: [u8; 3],
    zones: u16,
) -> Option<[u8; 11]> {
    if !matches!(mode, 1 | 2 | 4 | 5 | 7..=11) || brightness > 100 || speed > 9 || zones == 0 {
        return None;
    }
    let [lo, hi] = zones.to_le_bytes();
    // A4, keyboard target 21, effect, brightness, speed, direction, RGB, zone mask LE.
    Some([
        0xa4, 0x21, mode, brightness, speed, 0, rgb[0], rgb[1], rgb[2], lo, hi,
    ])
}

pub(crate) fn logo_report(setting: crate::control::Logo) -> Option<[u8; 11]> {
    use crate::control::Logo;
    if !setting.valid() {
        return None;
    }
    let (mode, brightness, speed, direction, rgb) = match setting {
        Logo::Off => (1, 0, 0, 0, [0; 3]),
        Logo::Static { brightness, rgb } => (2, brightness, 0, 0, rgb),
        Logo::Breathing { brightness, speed } => (4, brightness, speed, 1, [0; 3]),
        Logo::Neon { brightness, speed } => (5, brightness, speed, 1, [0; 3]),
        Logo::OperatingMode { brightness } => (6, brightness, 0, 0, [0; 3]),
    };
    // Exact CoverLogo target and zero zone mask from AcerHIDLogoLEDController.
    // Native animations cycle colors; no fixed-color animation is promised.
    Some([
        0xa4, 0x83, mode, brightness, speed, direction, rgb[0], rgb[1], rgb[2], 0, 0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logo_packets_cannot_select_other_targets_or_zone_masks() {
        use crate::control::Logo;
        assert_eq!(
            logo_report(Logo::Static {
                brightness: 60,
                rgb: [255, 0, 255]
            })
            .unwrap(),
            [0xa4, 0x83, 2, 60, 0, 0, 255, 0, 255, 0, 0]
        );
        for text in [
            "off",
            "static 100 0088FF",
            "breathing 60 3",
            "neon 60 3",
            "opmode 60",
        ] {
            let packet = logo_report(crate::control::logo(text.as_bytes()).unwrap()).unwrap();
            assert_eq!(&packet[..2], &[0xa4, 0x83]);
            assert_eq!(&packet[9..], &[0, 0]);
            assert!(matches!(packet[2], 1 | 2 | 4 | 5 | 6));
        }
        assert!(logo_report(Logo::Static {
            brightness: 101,
            rgb: [0; 3]
        })
        .is_none());
        assert!(logo_report(Logo::Breathing {
            brightness: 60,
            speed: 0
        })
        .is_none());
        assert!(logo_report(Logo::Neon {
            brightness: 60,
            speed: 6
        })
        .is_none());
    }
    #[test]
    fn profile_readback_does_not_use_the_odv0_mapping() {
        for mode in [0, 1, 4, 5, 6] {
            assert_eq!(profile_from_firmware(mode), Some(mode));
        }
        assert_eq!(profile_from_firmware(2), None);
        assert_eq!(profile_from_firmware(u64::MAX), None);
    }
    #[test]
    fn vendor_fan_payloads() {
        assert_eq!(fan_behavior(1), Some(0x0041_0009));
        assert_eq!(fan_behavior(2), Some(0x0082_0009));
        assert_eq!(fan_behavior(3), Some(0x00c3_0009));
        assert_eq!(fan_speed(false, 50), Some(0x3201));
        assert_eq!(fan_speed(true, 75), Some(0x4b04));
    }
    #[test]
    fn rejects_unrelated_or_out_of_range_values() {
        for n in 0..=255 {
            assert_eq!(valid_profile(n), [0, 1, 4, 5, 6].contains(&n));
        }
        assert!(fan_behavior(0).is_none());
        assert!(fan_behavior(4).is_none());
        assert!(fan_speed(false, 0).is_none());
        assert!(fan_speed(false, 39).is_none());
        assert!(fan_speed(true, 101).is_none());
        assert!(keyboard_report(2, 101, 0, [0; 3], 15).is_none());
        assert!(keyboard_report(2, 50, 10, [0; 3], 15).is_none());
        assert!(keyboard_report(3, 50, 0, [0; 3], 15).is_none());
        assert!(keyboard_report(2, 50, 0, [0; 3], 0).is_none());
    }
    #[test]
    fn keyboard_layout_matches_descriptor_and_vendor_writer() {
        assert_eq!(
            keyboard_report(2, 50, 0, [0x12, 0x34, 0x56], 15),
            Some([0xa4, 0x21, 2, 50, 0, 0, 0x12, 0x34, 0x56, 15, 0])
        );
        assert_eq!(
            keyboard_report(4, 100, 9, [255, 0, 0], 0x8000).unwrap()[9..],
            [0, 128]
        );
    }
}
