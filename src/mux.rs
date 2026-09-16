// SPDX-License-Identifier: GPL-2.0-only
//! Fixed PredatorSense GPU-mode values, shared by kernel and CLI.
#![allow(dead_code)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Optimus,
    Nvidia,
    Auto,
}

impl Mode {
    pub(crate) fn parse(input: &[u8]) -> Option<Self> {
        if input.len() > 128 || !input.is_ascii() {
            return None;
        }
        match core::str::from_utf8(input).ok()?.trim() {
            "optimus" => Some(Self::Optimus),
            "nvidia" => Some(Self::Nvidia),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Optimus => "optimus",
            Self::Nvidia => "nvidia",
            Self::Auto => "auto",
        }
    }
    pub(crate) fn from_firmware(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Optimus),
            2 => Some(Self::Nvidia),
            3 => Some(Self::Auto),
            _ => None,
        }
    }
    pub(crate) fn firmware(self) -> u8 {
        match self {
            Self::Optimus => 1,
            Self::Nvidia => 2,
            Self::Auto => 3,
        }
    }
    pub(crate) fn payload(self) -> u64 {
        2 | ((self.firmware() as u64) << 8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vendor_values_and_selector_are_exact() {
        for (mode, value, payload) in [
            (Mode::Optimus, 1, 0x102),
            (Mode::Nvidia, 2, 0x202),
            (Mode::Auto, 3, 0x302),
        ] {
            assert_eq!(Mode::parse(mode.name().as_bytes()), Some(mode));
            assert_eq!(Mode::from_firmware(value), Some(mode));
            assert_eq!(mode.payload(), payload);
            assert_eq!(mode.payload() & 255, 2);
            assert_eq!(mode.payload() >> 16, 0);
        }
        for value in 0..=255 {
            assert_eq!(
                Mode::from_firmware(value).is_some(),
                (1..=3).contains(&value)
            );
        }
    }
    #[test]
    fn invalid_modes_never_produce_a_firmware_command() {
        for value in [
            "intel",
            "hybrid",
            "3",
            "0x302",
            "nvidia extra",
            "auto; reboot",
            "AUTO",
            "auto\0",
            "overdrive",
            "optimus\nraw",
            "",
        ] {
            assert_eq!(Mode::parse(value.as_bytes()), None, "{value}");
        }
        assert_eq!(Mode::parse(b"  optimus\n"), Some(Mode::Optimus));
        assert_eq!(Mode::parse(&[b' '; 129]), None);
        for byte in 0..=255 {
            assert!(Mode::parse(&[byte]).is_none());
        }
    }
}
