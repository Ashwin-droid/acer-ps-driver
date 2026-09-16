// SPDX-License-Identifier: GPL-2.0-only
#![allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effects {
    Off,
    On,
}
impl Effects {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > 128 || !bytes.is_ascii() {
            return None;
        }
        match core::str::from_utf8(bytes).ok()?.trim() {
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }
    pub(crate) fn payload(self) -> u64 {
        match self {
            Self::Off => 6,
            Self::On => 0x106,
        }
    }
    pub(crate) fn decode(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Off),
            1 => Some(Self::On),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_verified_boot_selector_and_values() {
        assert_eq!(Effects::On.payload(), 0x106);
        assert_eq!(Effects::Off.payload(), 6);
        for invalid in [
            "1",
            "animation",
            "sound",
            "on extra",
            "off\0",
            "on;reboot",
            "",
        ] {
            assert_eq!(Effects::parse(invalid.as_bytes()), None);
        }
        for value in 0..=255 {
            assert_eq!(Effects::decode(value).is_some(), value <= 1);
        }
    }
}
