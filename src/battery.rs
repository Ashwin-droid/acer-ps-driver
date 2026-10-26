//! Battery UI: typed sysfs only. Calibration policy/inhibition stays in userspace.
use crate::{charge_policy::Operation, sysfs, Result};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    process::Command as Process,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::Duration,
};
const APS: &str = "/usr/local/sbin/aps";
const BAT: &str = "/sys/class/power_supply/BAT1";
static CANCEL: AtomicBool = AtomicBool::new(false);
extern "C" fn cancel(_: libc::c_int) {
    CANCEL.store(true, Ordering::Relaxed);
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Status,
    Capabilities,
    Limit(bool),
    CalibrationStatus,
    Stop,
    Start,
    Run,
}
fn parse(args: &[String]) -> Result<Command> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    if words.iter().any(|v| matches!(*v, "--help" | "-h" | "help")) {
        return Ok(Command::Help);
    }
    Ok(match words.as_slice() {
        [] | ["status"] => Command::Status,
        ["capabilities"] => Command::Capabilities,
        ["limit", value] => match Operation::parse("battery_charge_limit",value.as_bytes()) {
            Some(Operation::Limit(v)) => Command::Limit(v), _ => return Err("This model supports charge limits 80 or 100 only".into()),
        },
        ["calibration"] | ["calibration","status"] => Command::CalibrationStatus,
        ["calibration","stop"] => Command::Stop,
        ["calibration","start","--confirm"] => Command::Start,
        ["calibration","run","--confirm"] => Command::Run,
        ["calibration","start"] => return Err("Calibration deliberately cycles the battery and can take hours. Use aps battery calibration start --confirm with AC connected and charge limit 100.".into()),
        _ => return Err("Use aps battery --help".into()),
    })
}
pub(crate) fn help() {
    println!("Battery charging\n\n  aps battery status\n  aps battery capabilities\n  aps battery limit 80|100\n  aps battery calibration status\n  aps battery calibration start --confirm\n  aps battery calibration stop\n\nCharge limit 80 enables Acer Optimized Battery Charging; 100 disables it.\nIt does not discharge a battery that is already above the limit. These are\nfirmware settings, not a software charging loop. Writes require root/Polkit.\n\nCalibration deliberately discharges/recharges and can take hours. Connect AC\nand select limit 100 first. The foreground CLI inhibits system sleep, monitors\nAC and firmware active state, and requests cancellation on Ctrl-C or AC loss.\nNo calibration is started automatically or restored from disk. A killed/crashed\nclient may leave firmware calibration active; use the stop command to cancel.\nAn inactive flag alone does not prove successful calibration.\n\nThis laptop advertises the 80% limiter and calibration. Adaptive charging is\nnot advertised; the programmable HID boundary query returned unusable values.\nNo arbitrary thresholds, fast-charge, voltage/current or USB charging controls\nare exposed without a verified model-specific contract.");
}
fn protected_binary() -> Result<()> {
    for path in ["/usr", "/usr/local", "/usr/local/sbin", APS] {
        let m = fs::symlink_metadata(path)?;
        if m.uid() != 0
            || m.mode() & 0o022 != 0
            || m.file_type().is_symlink()
            || (path == APS && !m.is_file())
            || (path != APS && !m.is_dir())
        {
            return Err(
                "Installed APS and parent directories must be root-owned and protected".into(),
            );
        }
    }
    Ok(())
}
fn elevated_args(command: &Command) -> Option<Vec<&'static str>> {
    Some(match command {
        Command::Limit(true) => vec![APS, "battery", "limit", "80"],
        Command::Limit(false) => vec![APS, "battery", "limit", "100"],
        Command::Stop => vec![APS, "battery", "calibration", "stop"],
        Command::Start => vec![APS, "battery", "calibration", "start", "--confirm"],
        _ => return None,
    })
}
fn firmware_value(name: &str) -> Result<String> {
    Ok(sysfs::read(name)?.trim().to_owned())
}
fn active() -> Result<bool> {
    match firmware_value("battery_calibration")?.as_str() {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err("Invalid firmware calibration state".into()),
    }
}
fn ac_online() -> bool {
    fs::read_to_string("/sys/class/power_supply/ACAD/online").is_ok_and(|v| v.trim() == "1")
}
fn telemetry() {
    for (name, label) in [
        ("capacity", "Charge percent"),
        ("status", "Battery status"),
        ("cycle_count", "Cycle count"),
    ] {
        if let Ok(value) = fs::read_to_string(format!("{BAT}/{name}")) {
            println!("{label}: {}", value.trim());
        }
    }
    let quantity = |name| {
        fs::read_to_string(format!("{BAT}/{name}"))
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()
    };
    for (full_name, design_name, unit, scale) in [
        ("energy_full", "energy_full_design", "Wh", 1_000_000.0),
        ("charge_full", "charge_full_design", "mAh", 1_000.0),
    ] {
        if let (Some(full), Some(design)) = (quantity(full_name), quantity(design_name)) {
            println!(
                "Full / design capacity: {:.2} / {:.2} {unit}",
                full as f64 / scale,
                design as f64 / scale
            );
            if design != 0 {
                println!(
                    "Capacity relative to design: {:.1}%",
                    full as f64 / design as f64 * 100.0
                );
            }
            break;
        }
    }
}
struct CancelOnExit(bool);
impl Drop for CancelOnExit {
    fn drop(&mut self) {
        if self.0 {
            if let Err(error) = sysfs::write("battery_calibration", "off") {
                eprintln!("Calibration cancellation failed: {error}. Check aps battery calibration status.");
            }
        }
    }
}
fn calibrate() -> Result<()> {
    // Prevent concurrent APS sessions. Root can still deliberately control the
    // native firmware through sysfs; this is not an ownership lease in kernel.
    match fs::DirBuilder::new().mode(0o700).create("/run/aps") {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let directory = fs::symlink_metadata("/run/aps")?;
    if directory.uid() != 0 || !directory.is_dir() || directory.mode() & 0o022 != 0 {
        return Err("Unprotected APS runtime directory".into());
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open("/run/aps/calibration.lock")?;
    let meta = lock.metadata()?;
    if meta.uid() != 0 || !meta.is_file() || meta.nlink() != 1 || meta.mode() & 0o077 != 0 {
        return Err("Unprotected calibration lock".into());
    }
    lock.try_lock()
        .map_err(|_| "Another APS calibration session is active")?;
    if !ac_online() {
        return Err("Connect AC power before calibration".into());
    }
    if firmware_value("battery_charge_limit")? != "100" {
        return Err("Select aps battery limit 100 before calibration".into());
    }
    if active()? {
        return Err("Firmware calibration is already active; no new cycle started".into());
    }
    CANCEL.store(false, Ordering::Relaxed);
    // SAFETY: Handlers only set an atomic flag; this short-lived root process
    // uses the same fixed handler for SIGINT/SIGTERM and performs no I/O in it.
    unsafe {
        libc::signal(libc::SIGINT, cancel as *const () as libc::sighandler_t);
        libc::signal(libc::SIGTERM, cancel as *const () as libc::sighandler_t);
        libc::signal(libc::SIGHUP, cancel as *const () as libc::sighandler_t);
    }
    let mut guard = CancelOnExit(true);
    sysfs::write("battery_calibration", "on")?;
    println!("Firmware calibration active. Keep AC connected. Ctrl-C requests cancellation.");
    loop {
        if CANCEL.load(Ordering::Relaxed) {
            return Err("Calibration interrupted; cancellation requested".into());
        }
        if !ac_online() {
            return Err("AC disconnected; calibration cancellation requested".into());
        }
        if !active()? {
            guard.0 = false;
            println!("Firmware calibration is no longer active. This flag does not distinguish completion from an external abort.");
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }
}
pub(crate) fn run(args: &[String]) -> Result<()> {
    let command = parse(args)?;
    if command == Command::Help {
        help();
        return Ok(());
    }
    // SAFETY: geteuid has no pointer arguments. Kernel CAP_SYS_ADMIN is the real gate.
    let root = unsafe { libc::geteuid() } == 0;
    if command == Command::Run && !root {
        return Err("The internal calibration runner requires root".into());
    }
    if !root {
        if let Some(args) = elevated_args(&command) {
            protected_binary()?;
            let status = Process::new("/usr/bin/pkexec").args(args).status()?;
            return if status.success() {
                Ok(())
            } else {
                Err("Battery command failed or authorization was cancelled".into())
            };
        }
    }
    sysfs::Controls::open()?;
    match command {
        Command::Help => unreachable!(),
        Command::Status => {
            println!(
                "Firmware capabilities: {}",
                firmware_value("battery_capabilities")?
            );
            println!("Charge limit: {}%", firmware_value("battery_charge_limit")?);
            println!("Calibration active: {}", active()?);
            telemetry();
        }
        Command::Capabilities => println!("{}", firmware_value("battery_capabilities")?),
        Command::CalibrationStatus => println!("Calibration active: {}", active()?),
        Command::Limit(v) => {
            let value = if v { "80" } else { "100" };
            sysfs::write("battery_charge_limit", value)?;
            println!(
                "Charge limit: {}% (firmware readback)",
                firmware_value("battery_charge_limit")?
            );
        }
        Command::Stop => {
            sysfs::write("battery_calibration", "off")?;
            println!("Calibration active: {}", active()?);
        }
        Command::Start => {
            protected_binary()?;
            let status = Process::new("/usr/bin/systemd-inhibit")
                .args([
                    "--what=sleep",
                    "--mode=block",
                    "--who=APS",
                    "--why=Explicit battery calibration",
                    APS,
                    "battery",
                    "calibration",
                    "run",
                    "--confirm",
                ])
                .status()?;
            if !status.success() {
                return Err("Calibration session failed or was cancelled".into());
            }
        }
        Command::Run => calibrate()?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn calibration_needs_explicit_confirmation_and_elevation_is_fixed() {
        assert_eq!(parse(&[]).unwrap(), Command::Status);
        assert!(parse(&args(&["calibration", "start"])).is_err());
        assert_eq!(
            parse(&args(&["calibration", "start", "--confirm"])).unwrap(),
            Command::Start
        );
        assert_eq!(
            elevated_args(&Command::Start).unwrap(),
            [APS, "battery", "calibration", "start", "--confirm"]
        );
        assert!(elevated_args(&Command::Status).is_none());
        for value in ["0", "81", "adaptive", "80;reboot"] {
            assert!(parse(&args(&["limit", value])).is_err());
        }
        assert_eq!(
            parse(&args(&["calibration", "start", "--help"])).unwrap(),
            Command::Help
        );
    }
}
