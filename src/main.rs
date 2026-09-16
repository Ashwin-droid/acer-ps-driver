mod boot;
mod boot_media;
mod boot_picker;
mod boot_policy;
mod control;
mod gpu;
mod lid_state;
mod lighting;
mod logo;
mod mux;
mod protocol;
mod sysfs;
mod timeout;

use protocol::*;
use std::{env, fs, path::PathBuf, process::ExitCode, thread, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn model_check() -> Result<()> {
    for (field, expected) in [
        ("sys_vendor", "Acer"),
        ("product_name", "Predator PHN16S-71"),
        ("board_name", "FTYPE_ARX"),
        ("bios_version", "V1.28"),
    ] {
        if fs::read_to_string(format!("/sys/class/dmi/id/{field}"))?.trim() != expected {
            return Err(format!("Refusing hardware access: expected {field}={expected}").into());
        }
    }
    Ok(())
}

fn kernel_device() -> Result<sysfs::Controls> {
    sysfs::Controls::open()
}

fn call(control: &sysfs::Controls, request: u32, value: u64) -> Result<u64> {
    control.call(request, value)
}

fn profile_name(mode: u64) -> &'static str {
    match mode {
        0 => "quiet",
        1 => "balanced",
        4 => "performance",
        5 => "turbo",
        6 => "eco",
        _ => "unknown",
    }
}

fn profile_value(name: &str) -> Result<u64> {
    match name {
        "quiet" => Ok(0),
        "balanced" => Ok(1),
        "performance" => Ok(4),
        "turbo" => Ok(5),
        "eco" => Ok(6),
        _ => Err("Choose quiet, balanced, performance, turbo, or eco".into()),
    }
}

fn fan_name(mode: u64) -> &'static str {
    match mode {
        1 => "auto",
        2 => "maximum",
        3 => "manual",
        _ => "unknown",
    }
}

fn status(file: &sysfs::Controls) -> Result<()> {
    println!("Model: Predator PHN16S-71, BIOS V1.28");
    let mut errors = Vec::new();
    for (label, request) in [
        ("CPU fan RPM", GET_CPU_RPM),
        ("GPU fan RPM", GET_GPU_RPM),
        ("Performance mode", GET_PROFILE),
        ("Fan control", GET_FANS),
    ] {
        match call(file, request, 0) {
            Ok(value) if request == GET_PROFILE => {
                println!("{label}: {} ({value})", profile_name(value))
            }
            Ok(value) if request == GET_FANS => println!(
                "{label}: CPU {}, GPU {} (0x{value:x})",
                fan_name(value & 3),
                fan_name((value >> 6) & 3)
            ),
            Ok(value) => println!("{label}: {value}"),
            Err(error) => {
                println!("{label}: unavailable ({error})");
                errors.push(label);
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("Readback failed for {}", errors.join(", ")).into())
    }
}

struct Keyboard;
#[derive(Debug)]
struct KeyboardInfo {
    zones: u8,
    effects: u32,
}

impl Keyboard {
    fn open() -> Result<Self> {
        sysfs::Controls::open()?;
        Ok(Self)
    }
    fn info(&self) -> Result<KeyboardInfo> {
        let text = sysfs::read("keyboard_capabilities")?;
        let mut fields = text.split_ascii_whitespace();
        let zones: u8 = fields.next().ok_or("Missing zone count")?.parse()?;
        let effects = u32::from_str_radix(fields.next().ok_or("Missing effect mask")?, 16)?;
        if fields.next().is_some() || zones != 4 {
            return Err("Unexpected keyboard capabilities".into());
        }
        Ok(KeyboardInfo { zones, effects })
    }
    fn feature<const N: usize>(&self, bytes: [u8; N], write: bool) -> Result<[u8; N]> {
        if !write || N != 11 || bytes[0] != 0xa4 || bytes[1] != 0x21 {
            return Err("Only a typed keyboard setting is supported".into());
        }
        let effect = control::effect(bytes[2]).ok_or("Unsupported keyboard effect")?;
        let direction =
            control::light_parameter(bytes[2], bytes[5]).ok_or("Invalid effect parameter")?;
        let speed = if matches!(bytes[2], 1 | 2) {
            0
        } else {
            bytes[4]
        };
        let text = format!(
            "{} {} {} {} {} {:02X}{:02X}{:02X}\n",
            effect,
            bytes[3],
            speed,
            direction,
            u16::from_le_bytes([bytes[9], bytes[10]]),
            bytes[6],
            bytes[7],
            bytes[8]
        );
        sysfs::write("keyboard_effect", &text)?;
        Ok(bytes)
    }
}

fn rgb(text: &str) -> Result<[u8; 3]> {
    let text = text.strip_prefix('#').unwrap_or(text);
    if text.len() != 6 || !text.is_ascii() {
        return Err("Use a six-digit RGB color, e.g. FF8000".into());
    }
    Ok([
        u8::from_str_radix(&text[0..2], 16)?,
        u8::from_str_radix(&text[2..4], 16)?,
        u8::from_str_radix(&text[4..6], 16)?,
    ])
}

fn help() {
    println!(
        "APS — Acer PredatorSense controls (no sudo needed)

Quick examples:
  aps lighting apply breathing 80 2
  aps keyboard shifting 100 9
  aps lighting apply shifting 100
  aps fans maximum 60
  aps keyboard timeout on

Commands:
aps status
aps profile quiet|balanced|performance|turbo|eco
aps fans auto
aps fans maximum [seconds]
aps fans manual <cpu 40..100> <gpu 40..100> [seconds]
aps keyboard info
aps keyboard static <RRGGBB[,RRGGBB,...]> [brightness 0..100]
aps keyboard breathing|neon|wave [brightness 0..100] [speed 0..9]
aps keyboard shifting|zoom|meteor|twinkling [brightness] [speed]
aps keyboard off
aps keyboard timeout get|on|off
aps preferences status
aps button status
aps button mode \"power modes\"|hotkey
aps turbo-led auto|manual [RRGGBB]|RRGGBB|off|status
aps logo COLOR [brightness] | off | on | status
aps logo breathing|neon [brightness] [speed]
aps logo opmode [brightness]
aps gpu mux status
aps boot status|effects on|off
aps boot logo pick [DIRECTORY] | set FILE | prepare FILE OUTPUT | status | reset
aps gpu mux optimus|nvidia|auto  (Polkit authorization, or use sudo)
aps lighting effects
aps lighting apply EFFECT [COLOR] [BRIGHTNESS] [SPEED] [OPTIONS]
aps lighting save NAME EFFECT [COLOR] [BRIGHTNESS] [SPEED] [OPTIONS]
aps lighting list
aps lighting show NAME
aps lighting run NAME

Detailed help: aps lighting --help, aps fans --help, aps keyboard --help.
Lighting examples and speed units: aps lighting apply --help.

Lighting and ordinary controls run as your normal user after installation. MUX writes require root.
Fan maximum/manual sessions last 60 seconds by default. A kernel lease restores Auto
within about five seconds after a stopped or crashed client.
Fans can take several seconds to ramp up; keep the fan command running.
Only this exact model, board and BIOS are accepted. MUX changes require restart; no LCD overdrive commands exist."
    );
}

fn topic_help(topic: &str) {
    match topic {
        "boot" => boot::help(),
        "gpu" => gpu::help(),
        "logo" => logo::help(),
        "turbo-led" => println!("Turbo / mode-key RGB LED\n\n  aps turbo-led status\n  aps turbo-led auto\n  aps turbo-led manual FF0088\n  aps turbo-led FF0088\n  aps turbo-led manual\n  aps turbo-led off\n\nAuto follows the operating mode in firmware. A hex color selects Manual.\nManual without a color restores the saved manual color (white without saved state).\nOnly an advertised Turbo/Mode-key LED target is touched."),
        "button" => println!("Turbo button\n\n  aps button status\n  aps button mode \"power modes\"\n  aps button mode hotkey\n\nPower modes: Quiet -> Balanced -> Performance -> Turbo -> Eco.\nOn battery, Performance/Turbo are skipped. Hotkey emits F14 press/release.\nThe user-account preference service restores this setting after reboot.\nPredatorSense always emits F15. Press counters reset on module reload."),
        "preferences" => println!("User-account preferences\n\n  aps preferences status\n\naps-state saves/restores active settings in ~/.local/state/aps/ as your account.\nThe kernel validates sysfs writes but does not open or parse preference files."),
        "lighting" => lighting::help(None),
        "fans" => println!("Fan control\n\n  aps fans auto\n  aps fans maximum [SECONDS]\n  aps fans manual CPU_PERCENT GPU_PERCENT [SECONDS]\n\nExamples:\n  aps fans maximum 60\n  aps fans manual 55 65 300\n\nPercentages: 40..100. Duration: 1..86400 seconds, default 60.\nKeep the process running during Maximum/manual control. Fans can take several\nseconds to ramp up. When the command ends, APS requests Auto. If APS crashes\nor stalls, the five-second kernel lease expires and the kernel requests Auto."),
        "profile" => println!("Performance mode\n\n  aps profile quiet|balanced|performance|turbo|eco\n\nExample: aps profile balanced\nPerformance and Turbo require AC power. Changes are checked by firmware readback."),
        "keyboard" => println!("Keyboard controls\n\n  aps keyboard info\n  aps keyboard timeout get|on|off\n  aps keyboard off\n  aps keyboard static COLOR[,COLOR,...] [BRIGHTNESS]\n  aps keyboard breathing|neon|wave [BRIGHTNESS] [SPEED]\n  aps keyboard shifting|zoom|meteor|twinkling [BRIGHTNESS] [SPEED]\n\nOnly Static and the userspace Strobe effect accept colors. Wave also accepts --direction left|right.\nOther effects use their firmware/generated palettes.\nExamples:\n  aps keyboard breathing 80 2\n  aps keyboard shifting 100 9\n  aps keyboard zoom 60 3\n\nThese are aliases for aps lighting apply EFFECT; options and defaults match.\nFor preset recipes and all settings, use aps lighting --help."),
        _ => help(),
    }
}

fn button(args: &[String]) -> Result<()> {
    sysfs::Controls::open()?;
    if args.first().is_some_and(|arg| arg == "mode") && args.len() > 1 {
        let requested = args[1..].join(" ");
        let mode = control::button_mode(requested.as_bytes())
            .ok_or("Button mode must be 'power modes' or 'hotkey'; see aps button --help")?;
        sysfs::write(
            "button_mode",
            match mode {
                control::ButtonMode::PowerModes => "power modes",
                control::ButtonMode::Hotkey => "hotkey",
            },
        )?;
    } else if !(args.is_empty()
        || (args.len() == 1 && matches!(args[0].as_str(), "status" | "mode")))
    {
        return Err(
            "Use aps button status or aps button mode hotkey; see aps button --help".into(),
        );
    }
    println!("Turbo button mode: {}", sysfs::read("button_mode")?.trim());
    println!(
        "Recognized presses: {}",
        sysfs::read("button_presses")?.trim()
    );
    println!("Last action error: {}", sysfs::read("button_error")?.trim());
    println!(
        "PredatorSense key: F15; recognized presses: {}",
        sysfs::read("sense_presses")?.trim()
    );
    Ok(())
}

fn turbo_led(args: &[String]) -> Result<()> {
    sysfs::Controls::open()?;
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        [] | ["status"] => (),
        ["auto"] | ["manual"] => sysfs::write("turbo_led_mode", words[0])?,
        ["manual", color] | [color] => {
            control::turbo_led(color.as_bytes()).ok_or(
                "Use six RGB hex digits, e.g. aps turbo-led FF0088, or aps turbo-led auto",
            )?;
            sysfs::write("turbo_led", color)?;
        }
        _ => return Err("See aps turbo-led --help".into()),
    }
    println!("Turbo LED mode: {}", sysfs::read("turbo_led_mode")?.trim());
    println!(
        "Turbo LED last setting: {}",
        sysfs::read("turbo_led")?.trim()
    );
    println!(
        "Controller target / zones / effect mask: {}",
        sysfs::read("turbo_led_capabilities")?.trim()
    );
    Ok(())
}

fn preference_status() -> Result<String> {
    let root = match env::var_os("XDG_STATE_HOME") {
        Some(path) if PathBuf::from(&path).is_absolute() => PathBuf::from(path),
        _ => PathBuf::from(env::var_os("HOME").ok_or("HOME unavailable")?).join(".local/state"),
    };
    let directory = root.join("aps");
    Ok(match fs::read_to_string(directory.join("status")) {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => format!(
            "No service status yet. Preferences: {}\n",
            directory.join("preferences").display()
        ),
        Err(error) => return Err(error.into()),
    })
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "help") && args.len() == 2 {
        topic_help(&args[1]);
        return Ok(());
    }
    if let Some(topic) = args.first().filter(|arg| {
        matches!(
            arg.as_str(),
            "fans"
                | "keyboard"
                | "profile"
                | "button"
                | "turbo-led"
                | "preferences"
                | "logo"
                | "gpu"
        )
    }) {
        if args.len() == 1
            || args
                .iter()
                .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
        {
            topic_help(topic);
            return Ok(());
        }
    }
    match args.first().map(String::as_str) {
        Some("preferences") if args.len() == 1 || (args.len() == 2 && args[1] == "status") => {
            println!(
                "Userspace preference service\n{}",
                preference_status()?.trim()
            );
            Ok(())
        }
        Some("button") => button(&args[1..]),
        Some("turbo-led") => turbo_led(&args[1..]),
        Some("logo") => logo::run(&args[1..]),
        Some("gpu") => gpu::run(&args[1..]),
        Some("boot") => boot::run(&args[1..]),
        Some("lighting") => lighting::run(&args[1..]),
        None | Some("help" | "--help" | "-h") => {
            help();
            Ok(())
        }
        Some("status") if args.len() == 1 => status(&kernel_device()?),
        Some("profile") if args.len() == 2 => {
            let mode = profile_value(&args[1])?;
            if matches!(mode, 4 | 5) {
                let ac = fs::read_dir("/sys/class/power_supply")?
                    .filter_map(|e| e.ok())
                    .any(|e| {
                        fs::read_to_string(e.path().join("type")).is_ok_and(|s| s.trim() == "Mains")
                            && fs::read_to_string(e.path().join("online"))
                                .is_ok_and(|s| s.trim() == "1")
                    });
                if !ac {
                    return Err("Connect AC power before selecting performance or turbo".into());
                }
            }
            call(&kernel_device()?, SET_PROFILE, mode)?;
            println!(
                "Performance mode: {} (verified by firmware readback)",
                profile_name(mode)
            );
            Ok(())
        }
        Some("fans") if args.len() >= 2 => {
            let (request, value, duration) = match args[1].as_str() {
                "auto" if args.len() == 2 => (SET_FANS, 1, 0),
                "maximum" if args.len() <= 3 => (
                    SET_FANS,
                    2,
                    args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(60u64),
                ),
                "manual" if (4..=5).contains(&args.len()) => {
                    let cpu: u8 = args[2].parse()?;
                    let gpu: u8 = args[3].parse()?;
                    if fan_speed(false, cpu).is_none() || fan_speed(true, gpu).is_none() {
                        return Err("Manual fans must be between 40 and 100 percent".into());
                    }
                    (
                        SET_MANUAL,
                        cpu as u64 | ((gpu as u64) << 8),
                        args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(60u64),
                    )
                }
                _ => return Err("Invalid fans command; see --help".into()),
            };
            if (request != SET_FANS || value != 1) && (duration == 0 || duration > 86400) {
                return Err("Session duration must be 1..86400 seconds".into());
            }
            let file = kernel_device()?;
            call(&file, request, value)?;
            status(&file)?;
            println!("Fan setting accepted for {duration}s; the kernel restores Auto if lease renewal stops.");
            for elapsed in 0..duration {
                thread::sleep(Duration::from_secs(1));
                if elapsed + 1 < duration && (elapsed + 1) % 2 == 0 {
                    call(&file, request, value)?;
                }
                if elapsed + 1 == duration {
                    status(&file)?;
                }
            }
            // Explicit restore reports errors; the kernel lease also expires if this process dies.
            if duration > 0 {
                call(&file, SET_FANS, 1)?;
                println!("Fans returned to Auto.");
            }
            Ok(())
        }
        Some("keyboard") if args.len() >= 2 => {
            if args[1] == "timeout" && args.len() == 3 {
                return timeout::run(&args[2]);
            }
            if args[1] == "info" && args.len() == 2 {
                let info = Keyboard::open()?.info()?;
                println!(
                    "ENEK5130 keyboard target 0x21: {} zones, effect mask 0x{:08x}",
                    info.zones, info.effects
                );
                return Ok(());
            }
            let mut forwarded = vec!["apply".to_string()];
            forwarded.extend_from_slice(&args[1..]);
            lighting::run(&forwarded)
        }
        _ => Err("Unknown command or arguments; see --help".into()),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn color_input_is_bounded_and_unicode_safe() {
        assert_eq!(rgb("#ff8000").unwrap(), [255, 128, 0]);
        for bad in ["ééé", "GG0000", "12345", "1234567", ""] {
            assert!(rgb(bad).is_err());
        }
    }
}
