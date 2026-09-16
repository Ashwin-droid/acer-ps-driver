//! Unprivileged wrapper for the kernel's dedicated timeout control.
use super::*;
pub(super) fn run(value: &str) -> Result<()> {
    sysfs::Controls::open()?;
    match value {
        "get" => println!(
            "Keyboard backlight timeout: {} seconds (0 means disabled)",
            sysfs::read("keyboard_timeout")?.trim()
        ),
        "on" | "off" => {
            sysfs::write("keyboard_timeout", value)?;
            let seconds: u16 = sysfs::read("keyboard_timeout")?.trim().parse()?;
            if seconds != if value == "on" { 30 } else { 0 } {
                return Err("Timeout readback did not match".into());
            }
            println!("30-second keyboard backlight timeout: {value} (verified by kernel); brightness preserved");
        }
        _ => return Err("Use keyboard timeout get|on|off".into()),
    }
    Ok(())
}
