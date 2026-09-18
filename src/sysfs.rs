//! Unprivileged sysfs client. No ioctl, ACPI, HID, socket, or module-loading operations.
use super::*;
pub(super) const ROOT: &str = "/sys/class/misc/acer-phn16s";
fn path(name: &str) -> Result<PathBuf> {
    if !matches!(
        name,
        "cpu_fan_rpm"
            | "gpu_fan_rpm"
            | "profile"
            | "fan_control"
            | "keyboard_capabilities"
            | "keyboard_effect"
            | "keyboard_timeout"
            | "fan_lease_ms"
            | "button_mode"
            | "button_presses"
            | "button_error"
            | "turbo_led"
            | "turbo_led_mode"
            | "turbo_led_capabilities"
            | "keyboard_state"
            | "turbo_led_manual"
            | "sense_presses"
            | "preferences_generation"
            | "logo"
            | "logo_capabilities"
            | "sleep_logo"
            | "sleep_status"
            | "lid_state"
            | "lid_status"
            | "mux_mode"
            | "mux_request"
    ) {
        return Err("Unknown sysfs control".into());
    }
    Ok(PathBuf::from(ROOT).join(name))
}
pub(super) fn read(name: &str) -> Result<String> {
    Ok(fs::read_to_string(path(name)?)?)
}
pub(super) fn write(name: &str, text: &str) -> Result<()> {
    if text.len() > 128 {
        return Err("Control request too long".into());
    }
    fs::write(path(name)?, text).map_err(|error| write_error(name, &error))?;
    Ok(())
}

fn write_error(name: &str, error: &std::io::Error) -> String {
    if name == "mux_mode" {
        if matches!(error.raw_os_error(), Some(1 | 13)) {
            return crate::gpu::ROOT_REQUIRED.into();
        }
        return format!("Cannot request MUX mode: {error}. No automatic reboot or rollback was attempted. Check aps gpu mux status before retrying.");
    }
    let advice = if error.raw_os_error() == Some(112) && crate::lid_state::controlled(name) {
        " The lid is closed (docked/headless mode). Open the lid first to change keyboard or Turbo LED lighting."
    } else if error.raw_os_error() == Some(11) && crate::lid_state::controlled(name) {
        " The lid sensor could not be read. Lighting changes are blocked until the driver can confirm the lid is open."
    } else if error.kind() == std::io::ErrorKind::PermissionDenied {
        " Check the kernel package's sysfs access rule for your account."
    } else if error.raw_os_error() == Some(5) {
        " The driver reported a firmware I/O or readback failure; this is not a file permission error."
    } else {
        ""
    };
    format!("Cannot write {name}: {error}.{advice}")
}

pub(super) struct Controls;
impl Controls {
    pub(super) fn open() -> Result<Self> {
        model_check()?;
        if !path("profile")?.exists() {
            return Err(
                "APS sysfs driver is not loaded. Install the separate kernel package once.".into(),
            );
        }
        Ok(Self)
    }
    pub(super) fn call(&self, request: u32, value: u64) -> Result<u64> {
        match request {
            GET_CPU_RPM => Ok(read("cpu_fan_rpm")?.trim().parse()?),
            GET_GPU_RPM => Ok(read("gpu_fan_rpm")?.trim().parse()?),
            GET_PROFILE => profile_value(read("profile")?.trim()),
            GET_FANS => {
                let text = read("fan_control")?;
                let mut parts = text.split_ascii_whitespace();
                let cpu: u64 = parts.next().ok_or("Missing CPU fan mode")?.parse()?;
                let gpu: u64 = parts.next().ok_or("Missing GPU fan mode")?.parse()?;
                if parts.next().is_some() || !(1..=3).contains(&cpu) || !(1..=3).contains(&gpu) {
                    return Err("Invalid fan mode readback".into());
                }
                Ok(cpu | (gpu << 6))
            }
            SET_PROFILE => {
                if !valid_profile(value) {
                    return Err("Invalid profile".into());
                }
                write("profile", profile_name(value))?;
                Ok(value)
            }
            SET_FANS => {
                write(
                    "fan_control",
                    match value {
                        1 => "auto",
                        2 => "maximum",
                        _ => return Err("Invalid fan mode".into()),
                    },
                )?;
                Ok(value)
            }
            SET_MANUAL => {
                if value > 0xffff {
                    return Err("Invalid manual fan setting".into());
                }
                let text = format!("manual {} {}", value & 255, value >> 8);
                if control::fans(text.as_bytes()).is_none() {
                    return Err("Invalid manual fan percentages".into());
                }
                write("fan_control", &text)?;
                Ok(value)
            }
            _ => Err("Unknown control operation".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::write_error;
    #[test]
    fn mux_permission_errors_request_root_without_changing_lighting_advice() {
        for errno in [1, 13] {
            let mux = write_error("mux_mode", &std::io::Error::from_raw_os_error(errno));
            assert!(mux.contains("requires root privileges"));
            assert!(mux.contains("Polkit authorization"));
            assert!(!mux.contains("sysfs access rule"));
        }
        assert!(
            write_error("keyboard_effect", &std::io::Error::from_raw_os_error(13))
                .contains("sysfs access rule")
        );
    }
    #[test]
    fn closed_lid_errors_explain_docked_mode_without_permission_advice() {
        for name in [
            "keyboard_effect",
            "keyboard_timeout",
            "turbo_led",
            "turbo_led_mode",
        ] {
            let error = write_error(name, &std::io::Error::from_raw_os_error(112));
            assert!(error.contains("docked/headless"));
            assert!(error.contains("Open the lid first"));
            assert!(!error.contains("sysfs access rule"));
            let unknown = write_error(name, &std::io::Error::from_raw_os_error(11));
            assert!(unknown.contains("lid sensor could not be read"));
        }
        assert!(
            !write_error("logo", &std::io::Error::from_raw_os_error(112)).contains("Open the lid")
        );
    }
    #[test]
    fn access_advice_only_for_permission_errors() {
        let permission = write_error("profile", &std::io::Error::from_raw_os_error(13));
        assert!(permission.contains("sysfs access rule"));
        let io = write_error("profile", &std::io::Error::from_raw_os_error(5));
        assert!(io.contains("firmware I/O"));
        assert!(!io.contains("sysfs access rule"));
        let invalid = write_error("profile", &std::io::Error::from_raw_os_error(22));
        assert!(!invalid.contains("sysfs access rule"));
        assert!(!invalid.contains("firmware I/O"));
    }
}
