//! Suspend-only lighting configuration. Writes update kernel memory, not LEDs.
use super::*;

pub(super) fn help() {
    println!("Sleep lighting\n\n  aps sleep status\n  aps sleep off\n  aps sleep logo static 0000FF 25\n  aps sleep logo COLOR [BRIGHTNESS]\n  aps sleep logo breathing [BRIGHTNESS] [SPEED]\n  aps sleep logo neon [BRIGHTNESS] [SPEED]\n  aps sleep logo off\n\nBrightness: 0..100, default 60. Native animation speed: 1..5, default 3.\nOnly Static accepts a color. Breathing and Neon cycle RGB.\nChanges configure the next suspend; they do not change the awake logo.\nThe kernel keeps keyboard and Turbo LEDs off during sleep, even with the lid open.\nWake or an aborted suspend restores the awake lighting. Hibernation stays dark.\nThe user-account preference service saves this setting and reloads it at boot.\nDefault: all off. Firmware may override lighting during sleep; status is the\nconfigured request, not a physical color readback. Software effects are unavailable.");
}

fn parse(args: &[String]) -> Result<control::Logo> {
    if args == ["off"] {
        return Ok(control::Logo::Off);
    }
    let Some((command, rest)) = args.split_first() else {
        return Err("Use aps sleep logo EFFECT or aps sleep off; see aps sleep --help".into());
    };
    if command != "logo" {
        return Err("Use aps sleep logo EFFECT or aps sleep off; see aps sleep --help".into());
    }
    let setting = logo::parse(rest)?;
    control::sleep_logo(setting.to_string().as_bytes())
        .ok_or_else(|| "Sleep logo supports off, static, breathing, or neon only".into())
}

pub(super) fn run(args: &[String]) -> Result<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "help" | "--help" | "-h"))
    {
        help();
        return Ok(());
    }
    if args == ["status"] {
        sysfs::Controls::open()?;
        let text = sysfs::read("sleep_logo")?;
        let setting = control::sleep_logo(text.as_bytes()).ok_or("Invalid sleep logo readback")?;
        println!("Sleep logo: {setting}");
        println!("Sleep keyboard / Turbo LED: off");
        println!("Sleep state: {}", sysfs::read("sleep_status")?.trim());
        return Ok(());
    }
    let setting = parse(args)?;
    sysfs::Controls::open()?;
    sysfs::write("sleep_logo", &setting.to_string())?;
    println!("Sleep logo: {setting}. Applies on next suspend.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(args: &[&str]) -> Result<control::Logo> {
        parse(
            &args
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn sleep_commands_match_kernel_grammar() {
        for args in [
            vec!["off"],
            vec!["logo", "off"],
            vec!["logo", "static", "0000FF", "25"],
            vec!["logo", "#abcdef", "0"],
            vec!["logo", "on"],
            vec!["logo", "breathing", "25", "2"],
            vec!["logo", "neon", "100", "5"],
        ] {
            let setting = p(&args).unwrap();
            assert_eq!(
                control::sleep_logo(setting.to_string().as_bytes()),
                Some(setting)
            );
        }
        assert_eq!(
            p(&["logo", "static", "0000FF", "25"]).unwrap(),
            control::Logo::Static {
                brightness: 25,
                rgb: [0, 0, 255]
            }
        );
    }
    #[test]
    fn sleep_rejects_extra_fields_and_non_native_or_mode_effects() {
        for args in [
            vec![],
            vec!["logo"],
            vec!["off", "extra"],
            vec!["status", "extra"],
            vec!["keyboard", "on"],
            vec!["turbo-led", "on"],
            vec!["logo", "opmode"],
            vec!["logo", "auto"],
            vec!["logo", "strobe"],
            vec!["logo", "raw", "83"],
            vec!["logo", "reapply"],
            vec!["logo", "neon", "25", "0"],
            vec!["logo", "neon", "101", "3"],
            vec!["logo", "breathing", "0000FF"],
            vec!["logo", "static", "0000FF", "256"],
        ] {
            assert!(p(&args).is_err(), "{args:?}");
        }
    }
}
