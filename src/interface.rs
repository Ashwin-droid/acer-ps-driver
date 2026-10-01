// SPDX-License-Identifier: GPL-2.0-only
//! Public sysfs values: bounded parsing and matching display, shared with userspace.
#![allow(dead_code)]
use crate::control::{self, Light, Logo, TurboLed};
use core::fmt::{self, Write};
pub(crate) const MAX_TEXT: usize = 512;
fn text(bytes: &[u8]) -> Option<&str> {
    if bytes.is_empty()
        || bytes.len() > MAX_TEXT
        || !bytes
            .iter()
            .all(|b| b.is_ascii_graphic() || matches!(b, b' ' | b'\t' | b'\n'))
    {
        return None;
    }
    Some(core::str::from_utf8(bytes).ok()?.trim())
}
fn color(value: &str) -> Option<[u8; 3]> {
    match control::turbo_led(value.as_bytes())? {
        TurboLed::Rgb(rgb) => Some(rgb),
        _ => None,
    }
}
fn number(value: &str, maximum: u8) -> Option<u8> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse::<u8>().ok().filter(|v| *v <= maximum)
}
fn effect(name: &str) -> Option<u8> {
    (1..=11).find(|mode| control::effect(*mode) == Some(name))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Keyboard(pub(crate) [Option<Light>; 4]);
impl Keyboard {
    pub(crate) fn from_light(light: Light) -> Self {
        Self(core::array::from_fn(|i| {
            (light.zones & (1 << i) != 0).then_some(Light {
                zones: 1 << i,
                ..light
            })
        }))
    }
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        let value = text(bytes)?;
        if value == "undefined" {
            return Some(Self([None; 4]));
        }
        if let Some(zones) = value.strip_prefix("zones ") {
            let mut result = [None; 4];
            let mut parts = zones.split(';');
            for (i, slot) in result.iter_mut().enumerate() {
                let part = parts.next()?.trim();
                if part != "undefined" {
                    let parsed = single(part)?;
                    if !parsed
                        .0
                        .iter()
                        .all(|v| v.is_some_and(|s| same(s, parsed.0[0].unwrap())))
                    {
                        return None;
                    }
                    *slot = parsed.0[0].map(|s| Light { zones: 1 << i, ..s });
                }
            }
            return parts.next().is_none().then_some(Self(result));
        }
        single(value)
    }
    pub(crate) fn writable(self) -> bool {
        self.0.iter().any(Option::is_some)
    }
}
fn same(a: Light, b: Light) -> bool {
    Light {
        zones: b.zones,
        ..a
    } == b
}
fn single(value: &str) -> Option<Keyboard> {
    let mut parts = value.split_ascii_whitespace();
    let mode = effect(parts.next()?)?;
    if mode == 1 {
        return parts
            .next()
            .is_none()
            .then_some(Keyboard::from_light(Light {
                mode: 1,
                brightness: 0,
                speed: 0,
                direction: 0,
                zones: 15,
                rgb: [0; 3],
            }));
    }
    let mut colors = [[0; 3]; 4];
    let mut count = 0;
    let mut brightness = 60;
    let mut speed = if mode == 2 { 0 } else { 3 };
    let mut direction = if mode == 2 { 0 } else { 1 };
    let mut seen = 0;
    for part in parts {
        if let Some(v) = part.strip_prefix("brightness=") {
            if seen & 1 != 0 {
                return None;
            }
            seen |= 1;
            brightness = number(v, 100)?;
        } else if let Some(v) = part.strip_prefix("speed=") {
            if seen & 2 != 0 || mode == 2 {
                return None;
            }
            seen |= 2;
            speed = number(v, 9)?;
        } else if let Some(v) = part.strip_prefix("direction=") {
            if seen & 4 != 0 || mode == 2 {
                return None;
            }
            seen |= 4;
            direction = match v {
                "none" => 0,
                "native" if mode != 7 => 1,
                "right" if mode == 7 => 1,
                "left" if mode == 7 => 2,
                _ => return None,
            };
        } else {
            if mode != 2 || count >= 4 || seen != 0 {
                return None;
            }
            colors[count] = color(part)?;
            count += 1;
        }
    }
    if mode == 2 && count != 1 && count != 4 {
        return None;
    }
    let light = Light {
        mode,
        brightness,
        speed,
        direction,
        zones: 15,
        rgb: colors[0],
    };
    if !control::valid_light(light) {
        return None;
    }
    Some(Keyboard(core::array::from_fn(|i| {
        Some(Light {
            zones: 1 << i,
            rgb: if count == 4 { colors[i] } else { colors[0] },
            ..light
        })
    })))
}
fn rgb(out: &mut fmt::Formatter<'_>, value: [u8; 3]) -> fmt::Result {
    write!(out, "{:02X}{:02X}{:02X}", value[0], value[1], value[2])
}
fn light(out: &mut fmt::Formatter<'_>, value: Light) -> fmt::Result {
    out.write_str(control::effect(value.mode).ok_or(fmt::Error)?)?;
    if value.mode == 1 {
        return Ok(());
    }
    if value.mode == 2 {
        out.write_char(' ')?;
        rgb(out, value.rgb)?;
    }
    write!(out, " brightness={}", value.brightness)?;
    if value.mode != 2 {
        write!(
            out,
            " speed={} direction={}",
            value.speed,
            control::light_parameter(value.mode, value.direction).ok_or(fmt::Error)?
        )?;
    }
    Ok(())
}
impl fmt::Display for Keyboard {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.iter().all(Option::is_none) {
            return out.write_str("undefined");
        }
        if let Some(first) = self.0[0] {
            if self.0.iter().all(|v| v.is_some_and(|s| same(s, first))) {
                return light(out, first);
            }
            if first.mode == 2
                && self.0.iter().all(|v| {
                    v.is_some_and(|s| {
                        same(
                            Light {
                                rgb: first.rgb,
                                ..s
                            },
                            first,
                        )
                    })
                })
            {
                out.write_str("static")?;
                for value in self.0.iter().flatten() {
                    out.write_char(' ')?;
                    rgb(out, value.rgb)?;
                }
                return write!(out, " brightness={}", first.brightness);
            }
        }
        out.write_str("zones ")?;
        for (i, value) in self.0.iter().enumerate() {
            if i != 0 {
                out.write_str(" ; ")?;
            }
            match value {
                Some(value) => light(out, *value)?,
                None => out.write_str("undefined")?,
            }
        }
        Ok(())
    }
}

pub(crate) struct Cover(pub(crate) Logo);
impl Cover {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Logo> {
        let value = text(bytes)?;
        if value.starts_with("zones ") || value == "undefined" {
            return None;
        }
        let keyboard = single(value)?;
        let first = keyboard.0[0]?;
        if !keyboard.0.iter().all(|s| s.is_some_and(|s| same(s, first))) {
            return None;
        }
        let setting = match first.mode {
            1 => Logo::Off,
            2 => Logo::Static {
                brightness: first.brightness,
                rgb: first.rgb,
            },
            4 if first.direction == 1 => Logo::Breathing {
                brightness: first.brightness,
                speed: first.speed,
            },
            5 if first.direction == 1 => Logo::Neon {
                brightness: first.brightness,
                speed: first.speed,
            },
            _ => return None,
        };
        setting.valid().then_some(setting)
    }
}
impl fmt::Display for Cover {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Logo::Off => out.write_str("off"),
            Logo::Static {
                brightness,
                rgb: value,
            } => {
                out.write_str("static ")?;
                rgb(out, value)?;
                write!(out, " brightness={brightness}")
            }
            Logo::Breathing { brightness, speed } => {
                write!(out, "breathing brightness={brightness} speed={speed}")
            }
            Logo::Neon { brightness, speed } => {
                write!(out, "neon brightness={brightness} speed={speed}")
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Turbo {
    pub(crate) setting: TurboLed,
    pub(crate) manual: TurboLed,
}
impl Turbo {
    pub(crate) fn parse(bytes: &[u8], remembered: TurboLed) -> Option<Self> {
        let value = text(bytes)?;
        let mut parts = value.split_ascii_whitespace();
        let (setting, manual) = match parts.next()? {
            "auto" => {
                let manual = match parts.next() {
                    None => remembered,
                    Some(v) => control::turbo_led(v.strip_prefix("manual=")?.as_bytes())?,
                };
                (TurboLed::Auto, manual)
            }
            "manual" => (remembered, remembered),
            "off" => (TurboLed::Off, TurboLed::Off),
            "static" => {
                let value = TurboLed::Rgb(color(parts.next()?)?);
                (value, value)
            }
            _ => return None,
        };
        parts.next().is_none().then_some(Self { setting, manual })
    }
}
impl fmt::Display for Turbo {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.setting {
            TurboLed::Auto => {
                out.write_str("auto manual=")?;
                match self.manual {
                    TurboLed::Rgb(value) => rgb(out, value),
                    _ => out.write_str("off"),
                }
            }
            TurboLed::Off => out.write_str("off"),
            TurboLed::Rgb(value) => {
                out.write_str("static ")?;
                rgb(out, value)
            }
        }
    }
}
