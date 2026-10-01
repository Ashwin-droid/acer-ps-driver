//! Translate APS's existing internal commands and disk records to the public ABI.
//! These logical names are not sysfs aliases. Only the new paths are opened.
use crate::{control, interface};
use std::io;
pub(crate) fn attribute(name: &str) -> &str {
    match name {
        "profile" => "performance_mode",
        "keyboard_effect" | "keyboard_state" => "keyboard_lighting",
        "keyboard_timeout" => "keyboard_backlight_timeout_seconds",
        "keyboard_capabilities" => "keyboard_lighting_capabilities",
        "logo" => "cover_lighting",
        "sleep_logo" => "sleep_cover_lighting",
        "logo_capabilities" => "cover_lighting_capabilities",
        "turbo_led" | "turbo_led_mode" | "turbo_led_manual" => "turbo_lighting",
        "turbo_led_capabilities" => "turbo_lighting_capabilities",
        "button_mode" => "turbo_button_mode",
        "button_presses" => "turbo_button_presses",
        "button_error" => "turbo_button_error",
        "sense_presses" => "predatorsense_button_presses",
        "mux_mode" => "gpu_mux_mode",
        "mux_request" => "gpu_mux_request",
        _ => name,
    }
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid APS control value")
}
pub(crate) fn encode(name: &str, value: &str) -> io::Result<String> {
    Ok(match name {
        "keyboard_effect" => {
            interface::Keyboard::from_light(control::light(value.as_bytes()).ok_or_else(invalid)?)
                .to_string()
        }
        "logo" if control::logo_reapply(value.as_bytes()) => "reapply".into(),
        "logo" | "sleep_logo" => {
            interface::Cover(control::logo(value.as_bytes()).ok_or_else(invalid)?).to_string()
        }
        "keyboard_timeout" => if control::timeout(value.as_bytes()).ok_or_else(invalid)? {
            "30"
        } else {
            "0"
        }
        .into(),
        "turbo_led" => {
            let setting = control::turbo_led(value.as_bytes()).ok_or_else(invalid)?;
            interface::Turbo {
                setting,
                manual: setting,
            }
            .to_string()
        }
        _ => value.to_owned(),
    })
}
pub(crate) fn legacy_light(s: control::Light) -> String {
    format!(
        "{} {} {} {} {} {:02X}{:02X}{:02X}",
        control::effect(s.mode).unwrap(),
        s.brightness,
        s.speed,
        control::light_parameter(s.mode, s.direction).unwrap(),
        s.zones,
        s.rgb[0],
        s.rgb[1],
        s.rgb[2]
    )
}
pub(crate) fn decode(name: &str, value: &str) -> io::Result<String> {
    if value.trim() == "undefined" {
        return Ok("unknown".into());
    }
    Ok(match name {
        "keyboard_state" | "keyboard_effect" => {
            let state = interface::Keyboard::parse(value.as_bytes()).ok_or_else(invalid)?;
            state
                .0
                .into_iter()
                .flatten()
                .map(legacy_light)
                .collect::<Vec<_>>()
                .join("\n")
        }
        "logo" | "sleep_logo" => interface::Cover::parse(value.as_bytes())
            .ok_or_else(invalid)?
            .to_string(),
        "turbo_led" | "turbo_led_mode" | "turbo_led_manual" => {
            let state = interface::Turbo::parse(value.as_bytes(), control::TurboLed::Off)
                .ok_or_else(invalid)?;
            if name == "turbo_led_mode" {
                if state.setting == control::TurboLed::Auto {
                    "auto"
                } else {
                    "manual"
                }
                .into()
            } else {
                match if name == "turbo_led_manual" {
                    state.manual
                } else {
                    state.setting
                } {
                    control::TurboLed::Auto => "auto".into(),
                    control::TurboLed::Off => "off".into(),
                    control::TurboLed::Rgb(rgb) => {
                        format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
                    }
                }
            }
        }
        "fan_control" => match control::fans(value.as_bytes()).ok_or_else(invalid)? {
            control::Fans::Auto => "1 1",
            control::Fans::Maximum => "2 2",
            control::Fans::Manual(_, _) => "3 3",
        }
        .into(),
        _ => value.to_owned(),
    })
}
