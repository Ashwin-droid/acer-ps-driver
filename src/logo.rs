//! Cover-logo CLI. Only typed sysfs writes; no developer key or raw calls.
use super::*;
use control::Logo;

pub(super) fn help() {
    println!("Back-of-lid Predator logo\n\n  aps logo status\n  aps logo FF00FF [BRIGHTNESS]\n  aps logo static FF00FF [BRIGHTNESS]\n  aps logo on [BRIGHTNESS]             Static white\n  aps logo off\n  aps logo breathing [BRIGHTNESS] [SPEED]\n  aps logo neon [BRIGHTNESS] [SPEED]\n  aps logo opmode [BRIGHTNESS]         Normally off; flashes on Turbo\n  aps logo auto [BRIGHTNESS]           Alias for opmode\n\nBrightness: 0..100, default 60. Animation speed: 1..5, default 3.\nBreathing fades while cycling RGB; Neon cycles colors without fading dark.\nNeither animation accepts a fixed color. Static accepts six hex digits.\nTurbo may briefly interrupt an animation with a mode indication, then resume it.\nThe operating-mode flash's color mapping is not established.\n\nSettings are saved/restored by the user-account preference service. Native\neffects continue after aps exits. Status is the last accepted setting, not\na live color getter. Shifting is not supported for this logo.");
}
fn byte(args: &[String], index: usize, default: u8, maximum: u8, label: &str) -> Result<u8> {
    let value = match args.get(index) {
        Some(value) => value
            .parse::<u8>()
            .map_err(|_| format!("{label} must be a whole number up to {maximum}"))?,
        None => default,
    };
    if value > maximum {
        return Err(format!("{label} must be at most {maximum}").into());
    }
    Ok(value)
}
pub(super) fn parse(args: &[String]) -> Result<Logo> {
    let (name, rest) = args
        .split_first()
        .ok_or("Choose a logo command; see aps logo --help")?;
    let setting = match name.as_str() {
        "off" if rest.is_empty() => Logo::Off,
        "on" if rest.len() <= 1 => Logo::Static {
            brightness: byte(rest, 0, 60, 100, "Brightness")?,
            rgb: [255; 3],
        },
        "static" if (1..=2).contains(&rest.len()) => Logo::Static {
            brightness: byte(rest, 1, 60, 100, "Brightness")?,
            rgb: rgb(&rest[0])?,
        },
        "auto" | "opmode" if rest.len() <= 1 => Logo::OperatingMode {
            brightness: byte(rest, 0, 60, 100, "Brightness")?,
        },
        "breathing" | "neon" if rest.len() <= 2 => {
            if rest
                .first()
                .is_some_and(|v| v.trim_start_matches('#').len() == 6)
            {
                return Err("Logo Breathing/Neon cycle RGB and do not accept a fixed color; use static COLOR instead".into());
            }
            let brightness = byte(rest, 0, 60, 100, "Brightness")?;
            let speed = byte(rest, 1, 3, 5, "Speed")?;
            if speed == 0 {
                return Err("Logo animation speed must be 1..5".into());
            }
            if name == "breathing" {
                Logo::Breathing { brightness, speed }
            } else {
                Logo::Neon { brightness, speed }
            }
        }
        value if rest.len() <= 1 && rgb(value).is_ok() => Logo::Static {
            brightness: byte(rest, 0, 60, 100, "Brightness")?,
            rgb: rgb(value)?,
        },
        _ => {
            return Err(
                "Use aps logo COLOR, off, breathing, neon, or opmode; see aps logo --help".into(),
            )
        }
    };
    if !setting.valid() {
        return Err("Invalid logo setting".into());
    }
    Ok(setting)
}
pub(super) fn run(args: &[String]) -> Result<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|a| matches!(a.as_str(), "help" | "--help" | "-h"))
    {
        help();
        return Ok(());
    }
    if args == ["status"] {
        sysfs::Controls::open()?;
        println!(
            "Back-logo last accepted setting: {}",
            sysfs::read("logo")?.trim()
        );
        println!(
            "Target / zones / native effect mask: {}",
            sysfs::read("logo_capabilities")?.trim()
        );
        println!("Physical color is not queried; Turbo flashes may temporarily overlay the configured effect.");
        return Ok(());
    }
    let setting = parse(args)?;
    sysfs::Controls::open()?;
    sysfs::write("logo", &setting.to_string())?;
    println!("Back-logo setting sent: {setting}");
    match setting {
        Logo::Breathing { .. } => println!("Native RGB-cycling Breathing; color is not fixed."),
        Logo::Neon { .. } => println!("Native continuous RGB color cycling."),
        Logo::OperatingMode { .. } => {
            println!("Normally off; firmware flashes a color on a Turbo-button event.")
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(args: &[&str]) -> Result<Logo> {
        parse(&args.iter().map(|v| v.to_string()).collect::<Vec<_>>())
    }
    #[test]
    fn valid_commands_match_the_shared_sysfs_grammar() {
        for args in [
            vec!["FF00FF"],
            vec!["static", "#0088ff", "35"],
            vec!["on"],
            vec!["off"],
            vec!["breathing", "60", "3"],
            vec!["neon"],
            vec!["auto", "40"],
            vec!["opmode"],
        ] {
            let setting = p(&args).unwrap();
            assert_eq!(control::logo(setting.to_string().as_bytes()), Some(setting));
        }
        assert_eq!(
            p(&["FF00FF"]).unwrap(),
            Logo::Static {
                brightness: 60,
                rgb: [255, 0, 255]
            }
        );
    }
    #[test]
    fn invalid_or_unsupported_logo_settings_never_reach_sysfs() {
        for args in [
            vec!["shifting"],
            vec!["off", "extra"],
            vec!["FF0000", "101"],
            vec!["breathing", "FF0000"],
            vec!["breathing", "60", "0"],
            vec!["neon", "60", "6"],
            vec!["opmode", "-1"],
            vec!["static", "GGGGGG"],
            vec!["raw", "a4"],
            vec!["83", "2", "255"],
        ] {
            assert!(p(&args).is_err(), "{args:?}");
        }
    }
}
