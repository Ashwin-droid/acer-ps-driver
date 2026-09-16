//! Sysfs-only firmware MUX client. No PRIME, initramfs or reboot operations.
use crate::{mux::Mode, sysfs, Result};
use std::{os::unix::fs::MetadataExt, process::Command as Process};
pub(crate) const ROOT_REQUIRED: &str = "Changing the firmware MUX requires root privileges. Run aps gpu mux optimus|nvidia|auto for Polkit authorization, or use sudo. Status and lighting controls do not require root.";
extern "C" {
    fn geteuid() -> u32;
}

pub(crate) fn help() {
    println!("APS firmware GPU MUX\n\n  aps gpu mux status\n  aps gpu mux optimus\n  aps gpu mux nvidia\n  aps gpu mux auto\n\nMUX writes require root and request Polkit authorization automatically.\nUsing sudo skips the authorization dialog. Status and lighting do not elevate.\nOptimus: hybrid graphics, NVIDIA remains available for rendering.\nNvidia: NVIDIA GPU Only. Auto: firmware Automatic Select.\nA successful mode request requires a restart to take effect. APS never reboots.\nExternal display availability can change after switching modes.\nThis does not run prime-select or change Linux driver/initramfs configuration.\nStatus shows the firmware-reported selection, not proof of the active display route.\nThe last-request record lasts only until module unload or reboot.\nMUX settings are not saved or replayed by the APS preference service.");
}

const INSTALLED_APS: &str = "/usr/local/sbin/aps";
fn elevation_args(mode: Mode) -> [&'static str; 4] {
    [INSTALLED_APS, "gpu", "mux", mode.name()]
}
fn elevation_result(code: Option<i32>) -> Result<()> {
    match code {
        Some(0) => Ok(()),
        Some(126) => Err("MUX authorization cancelled. No elevated MUX command was run.".into()),
        Some(127) => Err("MUX authorization was denied or could not be obtained. No elevated MUX command was run.".into()),
        _ => Err("The elevated MUX command failed. See the message above and check aps gpu mux status before retrying.".into()),
    }
}
fn elevate(mode: Mode) -> Result<()> {
    // Never elevate argv[0], a workspace build, a PATH lookup or caller-supplied
    // arguments. Only the installed root-owned executable and canonical enum.
    for path in ["/usr", "/usr/local", "/usr/local/sbin", INSTALLED_APS] {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
            || if path == INSTALLED_APS {
                !metadata.is_file()
            } else {
                !metadata.is_dir()
            }
        {
            return Err("Refusing elevation: installed APS and its parent directories must be root-owned and not writable by other accounts. Reinstall the system package.".into());
        }
    }
    println!("MUX changes require administrator authorization; opening Polkit. A successful change will require a restart.");
    let status = Process::new("/usr/bin/pkexec")
        .args(elevation_args(mode)).status()
        .map_err(|error| format!("Could not request Polkit authorization: {error}. You can use sudo aps gpu mux {} instead.", mode.name()))?;
    elevation_result(status.code())
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Status,
    Set(Mode),
}
fn parse(args: &[String]) -> Result<Command> {
    if args.is_empty()
        || args
            .iter()
            .any(|s| matches!(s.as_str(), "help" | "--help" | "-h"))
    {
        return Ok(Command::Help);
    }
    match args {
        [mux] if mux == "mux" => Ok(Command::Status),
        [mux, status] if mux == "mux" && status == "status" => Ok(Command::Status),
        [mux, mode] if mux == "mux" => Mode::parse(mode.as_bytes())
            .map(Command::Set)
            .ok_or_else(|| "Choose aps gpu mux optimus|nvidia|auto, or aps gpu mux status".into()),
        _ => Err("Usage: aps gpu mux status|optimus|nvidia|auto".into()),
    }
}

fn mode() -> Result<Mode> {
    let value = sysfs::read("mux_mode")?;
    Mode::parse(value.as_bytes()).ok_or_else(|| "Unrecognized firmware MUX mode".into())
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    let command = parse(args)?;
    if command == Command::Help {
        help();
        return Ok(());
    }
    // SAFETY: libc geteuid takes no pointers and returns this process's euid.
    // This is a usability check only. The kernel independently enforces CAP_SYS_ADMIN.
    if let Command::Set(mode) = command {
        if unsafe { geteuid() } != 0 {
            return elevate(mode);
        }
    }
    sysfs::Controls::open()?;
    match command {
        Command::Help => unreachable!(),
        Command::Status => {
            println!("Firmware-reported MUX mode: {}", mode()?.name());
            println!(
                "Last APS MUX request since module load: {}",
                sysfs::read("mux_request")?.trim()
            );
            println!("A mode change requires a restart. Firmware selection is not a live display-route check.");
        }
        Command::Set(setting) => {
            // Parsing/model checks finish before any sysfs write. The kernel
            // independently validates the same three names and fixed selector.
            sysfs::write("mux_mode", setting.name())?;
            println!("Firmware accepted MUX mode: {}. Restart required; APS has not rebooted your laptop.", setting.name());
            println!("External display availability may change after restart.");
            match mode() {
                Ok(reported) => println!("Firmware currently reports: {}. The display route still needs post-restart verification.", reported.name()),
                Err(error) => eprintln!("Request was accepted, but readback failed: {error}. Do not assume it was rolled back."),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn elevation_uses_only_installed_executable_and_validated_mode() {
        for mode in [Mode::Optimus, Mode::Nvidia, Mode::Auto] {
            assert_eq!(
                elevation_args(mode),
                ["/usr/local/sbin/aps", "gpu", "mux", mode.name()]
            );
        }
        assert!(elevation_result(Some(0)).is_ok());
        assert!(elevation_result(Some(126))
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        assert!(elevation_result(Some(127))
            .unwrap_err()
            .to_string()
            .contains("denied"));
        assert!(elevation_result(Some(1)).is_err());
        assert!(elevation_result(None).is_err());
    }
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn only_explicit_modes_can_write() {
        assert_eq!(parse(&[]).unwrap(), Command::Help);
        assert_eq!(parse(&args(&["mux"])).unwrap(), Command::Status);
        assert_eq!(parse(&args(&["mux", "status"])).unwrap(), Command::Status);
        for mode in [Mode::Optimus, Mode::Nvidia, Mode::Auto] {
            assert_eq!(
                parse(&args(&["mux", mode.name()])).unwrap(),
                Command::Set(mode)
            );
            assert_eq!(
                parse(&args(&["mux", mode.name(), "--help"])).unwrap(),
                Command::Help
            );
        }
        for command in [
            &["mux", "intel"][..],
            &["mux", "auto", "now"],
            &["prime", "nvidia"],
            &["mux", "0x302"],
        ] {
            assert!(parse(&args(command)).is_err());
        }
    }
}
