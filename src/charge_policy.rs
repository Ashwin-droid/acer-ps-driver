// SPDX-License-Identifier: GPL-2.0-only
//! BatteryControl's fixed battery-1 operations. No arbitrary thresholds/registers.
#![allow(dead_code)]
pub(crate) const GET: u64 = 0x14;
pub(crate) const SET: u64 = 0x15;
pub(crate) const QUERY: [u8; 10] = [1, 1, 0, 0, 0, 0, 0, 0, 0, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Status {
    pub capabilities: u8,
    pub limited: bool,
    pub calibrating: bool,
    pub adaptive: bool,
}
impl Status {
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 8
            || bytes[1..3] != [0, 0]
            || bytes[0] & !7 != 0
            || bytes[3..6].iter().any(|b| *b > 1)
            || bytes[6..].iter().any(|b| *b != 0)
        {
            return None;
        }
        let active = bytes[3] | (bytes[4] << 1) | (bytes[5] << 2);
        if active & !bytes[0] != 0 {
            return None;
        }
        Some(Self {
            capabilities: bytes[0],
            limited: bytes[3] != 0,
            calibrating: bytes[4] != 0,
            adaptive: bytes[5] != 0,
        })
    }
    pub(crate) fn supported(self, op: Operation) -> bool {
        self.capabilities & op.mask() != 0
    }
    pub(crate) fn value(self, op: Operation) -> bool {
        match op {
            Operation::Limit(_) => self.limited,
            Operation::Calibration(_) => self.calibrating,
        }
    }
    pub(crate) fn conflict(self, op: Operation, ac: bool) -> bool {
        match op {
            Operation::Limit(_) => self.calibrating || self.adaptive,
            Operation::Calibration(true) => !ac || self.limited || self.adaptive,
            Operation::Calibration(false) => false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    Limit(bool),
    Calibration(bool),
}
impl Operation {
    pub(crate) fn parse(attribute: &str, input: &[u8]) -> Option<Self> {
        if input.len() > 128
            || !input
                .iter()
                .all(|b| b.is_ascii_graphic() || matches!(b, b' ' | b'\n' | b'\t'))
        {
            return None;
        }
        match (attribute, core::str::from_utf8(input).ok()?.trim()) {
            ("battery_charge_limit", "80") => Some(Self::Limit(true)),
            ("battery_charge_limit", "100") => Some(Self::Limit(false)),
            ("battery_calibration", "on") => Some(Self::Calibration(true)),
            ("battery_calibration", "off") => Some(Self::Calibration(false)),
            _ => None,
        }
    }
    pub(crate) fn mask(self) -> u8 {
        match self {
            Self::Limit(_) => 1,
            Self::Calibration(_) => 2,
        }
    }
    pub(crate) fn enabled(self) -> bool {
        match self {
            Self::Limit(v) | Self::Calibration(v) => v,
        }
    }
    pub(crate) fn packet(self) -> [u8; 23] {
        let mut packet = [0; 23];
        packet[0] = 1;
        packet[1] = self.mask();
        packet[2] = u8::from(self.enabled());
        packet
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_fixed_commands_and_fields_are_writable() {
        for (field, value, mask, enabled) in [
            ("battery_charge_limit", "80", 1, 1),
            ("battery_charge_limit", "100", 1, 0),
            ("battery_calibration", "on", 2, 1),
            ("battery_calibration", "off", 2, 0),
        ] {
            let op = Operation::parse(field, value.as_bytes()).unwrap();
            assert_eq!(&op.packet()[..3], &[1, mask, enabled]);
            assert_eq!(&op.packet()[3..], &[0; 20]);
        }
        for value in [
            "0",
            "79",
            "81",
            "255",
            "80 100",
            "80\0",
            "80; reboot",
            "on",
            "adaptive",
        ] {
            assert!(Operation::parse("battery_charge_limit", value.as_bytes()).is_none());
        }
        assert!(Operation::parse("battery_calibration", b"on extra").is_none());
        assert!(Operation::parse("raw", b"80").is_none());
        assert!(Operation::parse("battery_charge_limit", &[b'8'; 129]).is_none());
    }
    #[test]
    fn readback_checks_capabilities_and_reply_shape() {
        let status = Status::decode(&[3, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert!(status.supported(Operation::Limit(true)));
        assert!(status.supported(Operation::Calibration(true)));
        assert_eq!(status.capabilities & 4, 0);
        // Live before/after fixtures: WMI uReturn is TWO bytes, followed by
        // the feature-status array. Do not mistake its high byte for a flag.
        let limited = Status::decode(&[3, 0, 0, 1, 0, 0, 0, 0]).unwrap();
        assert!(limited.limited && !limited.calibrating);
        assert!(
            Status::decode(&[3, 0, 0, 0, 1, 0, 0, 0])
                .unwrap()
                .calibrating
        );
        for data in [
            &[3, 1, 0, 0, 0, 0, 0, 0][..],
            &[3, 0, 2, 0, 0, 0, 0, 0],
            &[3, 0, 0, 2, 0, 0, 0, 0],
            &[3, 0, 0, 0, 0, 1, 0, 0],
            &[3, 0, 0, 0, 0, 0, 0, 1],
            &[0, 0, 1, 0, 0, 0, 0, 0],
            &[255; 8],
            &[0; 7],
            &[0; 9],
        ] {
            assert!(Status::decode(data).is_none());
        }
    }
    #[test]
    fn inactive_and_unsupported_features_cannot_be_confused() {
        let none = Status::decode(&[0; 8]).unwrap();
        assert!(!none.supported(Operation::Limit(false)));
        assert!(!none.supported(Operation::Calibration(false)));
        for capabilities in 0..8u8 {
            for active in 0..8u8 {
                let bytes = [
                    capabilities,
                    0,
                    0,
                    active & 1,
                    (active >> 1) & 1,
                    (active >> 2) & 1,
                    0,
                    0,
                ];
                assert_eq!(
                    Status::decode(&bytes).is_some(),
                    active & !capabilities == 0
                );
            }
        }
    }
    #[test]
    fn calibration_cannot_start_on_battery_or_with_a_charge_limit() {
        let mut status = Status::decode(&[3, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert!(status.conflict(Operation::Calibration(true), false));
        assert!(!status.conflict(Operation::Calibration(true), true));
        status.limited = true;
        assert!(status.conflict(Operation::Calibration(true), true));
        status.calibrating = true;
        assert!(status.conflict(Operation::Limit(false), true));
        assert!(!status.conflict(Operation::Calibration(false), false));
    }
}
