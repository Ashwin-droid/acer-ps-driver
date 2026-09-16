//! Named lighting presets, with settings validated per effect.
use super::*;
use std::time::Instant;

#[derive(Clone, Debug, PartialEq)]
struct Preset {
    effect: String,
    colors: Vec<[u8; 3]>,
    brightness: u8,
    speed: f64,
    direction: String,
    duty: f64,
    duration: u64,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            effect: "static".into(),
            colors: vec![[255, 128, 0]],
            brightness: 60,
            speed: 3.0,
            direction: "right".into(),
            duty: 0.5,
            duration: 30,
        }
    }
}

fn native_mode(effect: &str) -> Option<u8> {
    match effect {
        "off" => Some(1),
        "static" => Some(2),
        "breathing" => Some(4),
        "neon" => Some(5),
        "wave" => Some(7),
        "shifting" => Some(8),
        "zoom" => Some(9),
        "meteor" => Some(10),
        "twinkling" => Some(11),
        _ => None,
    }
}

fn accepts_color(effect: &str) -> bool {
    matches!(effect, "static" | "strobe")
}

impl Preset {
    fn validate(&self) -> Result<()> {
        if native_mode(&self.effect).is_none()
            && !matches!(self.effect.as_str(), "strobe" | "rainbow")
        {
            return Err("Unknown lighting effect; run lighting effects".into());
        }
        if self.brightness > 100
            || !matches!(self.direction.as_str(), "left" | "right")
            || self.duration == 0
            || self.duration > 86400
        {
            return Err("Invalid brightness, direction, duration, or color count".into());
        }
        if !self.speed.is_finite() || !self.duty.is_finite() {
            return Err("Settings must be finite numbers".into());
        }
        if accepts_color(&self.effect) && !matches!(self.colors.len(), 1 | 4) {
            return Err("Static and Strobe need one color or exactly four zone colors".into());
        }
        if !accepts_color(&self.effect) && !self.colors.is_empty() {
            return Err(
                "This effect has no color argument; only Static and Strobe accept colors".into(),
            );
        }
        match self.effect.as_str() {
            "strobe" if !(0.2..=5.0).contains(&self.speed) || !(0.1..=0.9).contains(&self.duty) => {
                return Err(
                    "Strobe speed must be 0.2..5 flashes/second; duty must be 0.1..0.9".into(),
                )
            }
            "rainbow" if !(1.0..=60.0).contains(&self.speed) => {
                return Err("Rainbow speed must be 1..60 seconds per cycle".into())
            }
            "strobe" | "rainbow" => (),
            _ if !(0.0..=9.0).contains(&self.speed) || self.speed.fract() != 0.0 => {
                return Err("Native effect speed must be a whole number from 0 to 9".into())
            }
            _ => (),
        }
        Ok(())
    }

    fn parse(args: &[String]) -> Result<Self> {
        let mut preset = Self::default();
        let mut expanded = Vec::new();
        let mut remaining = args;
        if let Some(effect) = remaining.first().filter(|arg| !arg.starts_with('-')) {
            expanded.extend(["--effect".to_string(), effect.clone()]);
            remaining = &remaining[1..];
            if let Some(color) = remaining.first().filter(|arg| {
                !arg.starts_with('-')
                    && (arg.trim_start_matches('#').len() == 6
                        || arg.contains(',')
                        || arg.starts_with('#'))
            }) {
                expanded.extend(["--colors".to_string(), color.clone()]);
                remaining = &remaining[1..];
            }
            for option in ["--brightness", "--speed"] {
                if let Some(value) = remaining.first().filter(|arg| !arg.starts_with('-')) {
                    expanded.extend([option.to_string(), value.clone()]);
                    remaining = &remaining[1..];
                }
            }
        }
        expanded.extend_from_slice(remaining);
        let args = expanded.as_slice();
        if !args.len().is_multiple_of(2) {
            return Err(format!(
                "Missing value after '{}'. See aps lighting apply --help.",
                args.last().map(String::as_str).unwrap_or("option")
            )
            .into());
        }
        let mut seen = std::collections::HashSet::new();
        for pair in args.chunks_exact(2) {
            let option = if pair[0] == "--color" {
                "--colors"
            } else {
                pair[0].as_str()
            };
            if !seen.insert(option) {
                return Err(format!("Duplicate option {}", pair[0]).into());
            }
            match option {
                "--effect" => preset.effect = pair[1].clone(),
                "--colors" => preset.colors = pair[1].split(',').map(rgb).collect::<Result<_>>()?,
                "--brightness" => preset.brightness = pair[1].parse().map_err(|_| "Brightness must be an integer from 0 to 100")?,
                "--speed" => preset.speed = pair[1].parse().map_err(|_| "Speed must be a number; see aps lighting apply --help for units")?,
                "--direction" => preset.direction = pair[1].clone(),
                "--duty" => preset.duty = pair[1].parse()?,
                "--duration" => preset.duration = pair[1].parse()?,
                _ => return Err(format!("Unknown option '{}'. Use aps lighting apply EFFECT [COLOR] [BRIGHTNESS] [SPEED], or named options. See aps lighting apply --help.", pair[0]).into()),
            }
        }
        if !seen.contains("--effect") {
            return Err("Choose an effect, e.g. aps lighting apply breathing. See aps lighting apply --help.".into());
        }
        if seen.contains("--duty") && preset.effect != "strobe" {
            return Err("--duty applies only to strobe".into());
        }
        if seen.contains("--direction") && !matches!(preset.effect.as_str(), "wave" | "rainbow") {
            return Err("--direction is exposed only for wave and rainbow".into());
        }
        if seen.contains("--speed") && matches!(preset.effect.as_str(), "static" | "off") {
            return Err("Static/off have no speed setting".into());
        }
        if !accepts_color(&preset.effect) {
            if seen.contains("--colors") {
                return Err(format!("{} has no color argument. Only Static and Strobe accept colors; use {} [BRIGHTNESS] [SPEED].", preset.effect, preset.effect).into());
            }
            preset.colors.clear();
        }
        preset.validate()?;
        Ok(preset)
    }

    fn serialize(&self) -> String {
        let colors = self
            .colors
            .iter()
            .map(|c| format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]))
            .collect::<Vec<_>>()
            .join(",");
        format!("version=1\neffect={}\ncolors={}\nbrightness={}\nspeed={}\ndirection={}\nduty={}\nduration={}\n",
            self.effect, colors, self.brightness, self.speed, self.direction, self.duty, self.duration)
    }

    fn deserialize(text: &str) -> Result<Self> {
        if text.len() > 4096 {
            return Err("Preset is too large".into());
        }
        let mut fields = std::collections::HashMap::new();
        for line in text.lines() {
            let (key, value) = line.split_once('=').ok_or("Invalid preset line")?;
            if fields.insert(key, value).is_some() {
                return Err("Duplicate preset field".into());
            }
        }
        if fields.len() != 8 || fields.get("version") != Some(&"1") {
            return Err("Invalid preset version or fields".into());
        }
        let get = |key| fields.get(key).copied().ok_or("Missing preset field");
        let mut preset = Self {
            effect: get("effect")?.into(),
            colors: if get("colors")?.is_empty() {
                Vec::new()
            } else {
                get("colors")?.split(',').map(rgb).collect::<Result<_>>()?
            },
            brightness: get("brightness")?.parse()?,
            speed: get("speed")?.parse()?,
            direction: get("direction")?.into(),
            duty: get("duty")?.parse()?,
            duration: get("duration")?.parse()?,
        };
        // Ignore legacy non-configurable colors without rewriting user recipes.
        // Static and Strobe retain their configured colors.
        if !accepts_color(&preset.effect) {
            preset.colors.clear();
        }
        preset.validate()?;
        Ok(preset)
    }

    fn report(&self, mode: u8, color: [u8; 3], mask: u16) -> Result<[u8; 11]> {
        if mode == 1 {
            return keyboard_report(1, 0, 0, [0; 3], mask)
                .ok_or_else(|| "Invalid off frame".into());
        }
        let speed = if native_mode(&self.effect).is_some() {
            self.speed as u8
        } else {
            0
        };
        let rgb = if mode == 2 { color } else { [0; 3] };
        let mut report = keyboard_report(mode, self.brightness, speed, rgb, mask)
            .ok_or("Invalid lighting frame")?;
        report[5] = match mode {
            4 | 5 | 8..=11 => 1,
            7 => {
                if self.direction == "right" {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        };
        Ok(report)
    }
}

fn preset_path(name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.len() > 32
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(
            "Preset names must contain 1..32 letters, digits, hyphens, or underscores".into(),
        );
    }
    let config = match env::var_os("XDG_CONFIG_HOME") {
        Some(value) if PathBuf::from(&value).is_absolute() => PathBuf::from(value),
        _ => PathBuf::from(env::var_os("HOME").ok_or("HOME is unavailable")?).join(".config"),
    };
    Ok(config
        .join("aps")
        .join("profiles")
        .join(format!("{name}.preset")))
}

fn color_wheel(hue: f64) -> [u8; 3] {
    let hue = hue.rem_euclid(1.0) * 6.0;
    let ramp = ((hue.fract()) * 255.0).round() as u8;
    match hue.floor() as u8 {
        0 => [255, ramp, 0],
        1 => [255 - ramp, 255, 0],
        2 => [0, 255, ramp],
        3 => [0, 255 - ramp, 255],
        4 => [ramp, 0, 255],
        _ => [255, 0, 255 - ramp],
    }
}

fn send_frame(
    keyboard: &Keyboard,
    zones: u8,
    mode: u8,
    preset: &Preset,
    colors: &[[u8; 3]],
) -> Result<()> {
    let cycle_color = [[0, 0, 0]];
    let colors = if mode != 2 && colors.is_empty() {
        &cycle_color[..]
    } else {
        colors
    };
    if colors.len() != 1 && colors.len() != zones as usize {
        return Err(format!("This keyboard needs one color or {zones} zone colors").into());
    }
    for (i, color) in colors.iter().enumerate() {
        let mask = if colors.len() == 1 {
            ((1u32 << zones) - 1) as u16
        } else {
            1u16 << i
        };
        keyboard.feature(preset.report(mode, *color, mask)?, true)?;
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn apply(preset: &Preset) -> Result<()> {
    preset.validate()?;
    let keyboard = Keyboard::open()?;
    let info = keyboard.info()?;
    if let Some(mode) = native_mode(&preset.effect) {
        if info.effects & (1 << (mode - 1)) == 0 {
            return Err(format!("Keyboard does not support {}", preset.effect).into());
        }
        send_frame(&keyboard, info.zones, mode, preset, &preset.colors)?;
        match mode {
            1 => println!("Keyboard lighting off."),
            2 => println!("Applied static: brightness {}%.", preset.brightness),
            4 => println!(
                "Applied breathing: RGB cycling, brightness {}%, speed {}/9.",
                preset.brightness, preset.speed
            ),
            7 => println!(
                "Applied wave: brightness {}%, speed {}/9, direction {}.",
                preset.brightness, preset.speed, preset.direction
            ),
            _ => println!(
                "Applied {}: brightness {}%, speed {}/9.",
                preset.effect, preset.brightness, preset.speed
            ),
        }
        return Ok(());
    }
    if info.effects & 3 != 3 {
        return Err("Static/off support required for software effects".into());
    }
    println!("Running {} for {}s: brightness {}, speed {}. This effect is generated by the Rust process.",
        preset.effect, preset.duration, preset.brightness, preset.speed);
    let start = Instant::now();
    let mut previous_strobe = None;
    let mut final_colors = if preset.effect == "rainbow" {
        (0..info.zones)
            .map(|zone| color_wheel(zone as f64 / info.zones as f64))
            .collect::<Vec<_>>()
    } else {
        preset.colors.clone()
    };
    while start.elapsed().as_secs_f64() < preset.duration as f64 {
        let elapsed = start.elapsed().as_secs_f64();
        if preset.effect == "strobe" {
            let on = (elapsed * preset.speed).fract() < preset.duty;
            if previous_strobe != Some(on) {
                send_frame(
                    &keyboard,
                    info.zones,
                    if on { 2 } else { 1 },
                    preset,
                    &preset.colors,
                )?;
                previous_strobe = Some(on);
            }
            thread::sleep(Duration::from_millis(10));
        } else {
            let sign = if preset.direction == "right" {
                1.0
            } else {
                -1.0
            };
            let colors = (0..info.zones)
                .map(|zone| {
                    color_wheel(sign * elapsed / preset.speed + zone as f64 / info.zones as f64)
                })
                .collect::<Vec<_>>();
            send_frame(&keyboard, info.zones, 2, preset, &colors)?;
            final_colors = colors;
            thread::sleep(Duration::from_millis(20));
        }
    }
    send_frame(&keyboard, info.zones, 2, preset, &final_colors)?;
    if preset.effect == "strobe" {
        println!("Effect finished; left the preset's static colors on the keyboard.");
    } else {
        println!("Effect finished; left the last generated rainbow frame on the keyboard.");
    }
    Ok(())
}

pub(super) fn help(effect: Option<&str>) {
    println!(
        "APS lighting

Usage:
  aps lighting apply EFFECT [COLOR] [BRIGHTNESS] [SPEED] [OPTIONS]
  aps lighting save NAME EFFECT [COLOR] [BRIGHTNESS] [SPEED] [OPTIONS]
  aps lighting run NAME       Apply a saved preset
  aps lighting list           List saved presets
  aps lighting show NAME      Show a preset's settings

Examples:
  aps lighting apply breathing
  aps lighting apply static ffffff
  aps lighting apply shifting 100
  aps lighting apply breathing 80 2
  aps lighting apply wave 80 --speed 3 --direction left
  aps lighting apply strobe ffffff 60 --speed 2 --duty 0.5 --duration 15
  aps lighting apply rainbow 65 --speed 6 --duration 30
  aps lighting save night breathing 40 2

COLOR is six hex digits (optional #), or comma-separated zone colors.
Only STATIC and STROBE accept COLOR or --color/--colors.
Without a color, the first number is BRIGHTNESS, from 0 to 100.
Defaults: brightness 60%, native speed 3, wave direction right.
Static and Strobe default to FF8000 when COLOR is omitted.
All other effects have NO color argument; they use firmware/generated palettes.
Old presets for those effects ignore their previously stored color field.

Effects:
  off                         Turns lighting off; no color or speed
  static                      Colors and brightness; no speed
  breathing, neon, shifting, zoom, meteor, twinkling
                              Firmware colors, brightness, native speed 0..9; no COLOR
  wave                        Native speed 0..9, --direction left|right; no COLOR
  strobe                      --speed 0.2..5 flashes/sec, --duty 0.1..0.9
  rainbow                     --speed 1..60 seconds per color cycle

Named options also work:
  --effect NAME --colors HEX[,HEX,...] --brightness 0..100 --speed VALUE
  --direction left|right --duty FRACTION --duration SECONDS

Native effects continue after aps exits. Strobe/rainbow run for --duration
seconds (default 30) and require the process to stay running.
Bare 'aps lighting apply' only shows this help; it never changes lighting."
    );
    if let Some(effect) = effect {
        match effect {
            "strobe" => println!("\nStrobe: speed is flashes/second. Duty 0.5 means equal on/off time.\nExample: aps lighting apply strobe ffffff 60 --speed 2 --duty 0.5"),
            "rainbow" => println!("\nRainbow: speed is seconds/cycle; larger values mean slower changes.\nExample: aps lighting apply rainbow 65 --speed 10 --direction left"),
            "static" | "off" => println!("\n{effect} has no speed or direction setting."),
            name if native_mode(name).is_some() => println!("\n{name}: brightness 0..100, speed on the controller's 0..9 scale."),
            _ => (),
        }
    }
}

pub(super) fn run(args: &[String]) -> Result<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h" | "help"))
        || (args.len() == 1 && args[0] == "apply")
    {
        let effect = args
            .iter()
            .find(|arg| native_mode(arg).is_some() || matches!(arg.as_str(), "strobe" | "rainbow"));
        help(effect.map(String::as_str));
        return Ok(());
    }
    match args.first().map(String::as_str) {
        Some("effects") if args.len() == 1 => {
            help(None);
            Ok(())
        }
        Some("apply") => apply(&Preset::parse(&args[1..])?),
        Some("save") if args.len() >= 2 => {
            let preset = Preset::parse(&args[2..])?;
            let path = preset_path(&args[1])?;
            fs::create_dir_all(path.parent().ok_or("Invalid preset path")?)?;
            fs::write(&path, preset.serialize())?;
            println!("Saved preset {} to {}", args[1], path.display());
            Ok(())
        }
        Some("run" | "show") if args.len() == 2 => {
            let preset = Preset::deserialize(&fs::read_to_string(preset_path(&args[1])?)?)?;
            if args[0] == "show" {
                print!("{}", preset.serialize());
                Ok(())
            } else {
                apply(&preset)
            }
        }
        Some("list") if args.len() == 1 => {
            let folder = preset_path("example")?
                .parent()
                .ok_or("Invalid preset folder")?
                .to_path_buf();
            if folder.exists() {
                let mut names = fs::read_dir(folder)?
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|x| x == "preset"))
                    .map(|e| {
                        e.path()
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect::<Vec<_>>();
                names.sort();
                for name in names {
                    println!("{name}");
                }
            }
            Ok(())
        }
        _ => Err("Use lighting effects for commands and per-effect settings".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parsed(args: &[&str]) -> Result<Preset> {
        Preset::parse(
            &args
                .iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn screenshot_commands_accept_positional_arguments() {
        assert_eq!(parsed(&["shifting", "100"]).unwrap().brightness, 100);
        assert_eq!(parsed(&["breathing"]).unwrap().effect, "breathing");
        assert!(parsed(&["breathing", "ffffff"]).is_err());
        let p = parsed(&["breathing", "80", "2"]).unwrap();
        assert_eq!((p.brightness, p.speed), (80, 2.0));
        assert_eq!(parsed(&["static", "000000"]).unwrap().colors, [[0; 3]]);
        assert_eq!(
            parsed(&["wave", "80", "--speed", "3", "--direction", "left"])
                .unwrap()
                .direction,
            "left"
        );
    }
    #[test]
    fn palette_effects_remove_color_arguments_and_migrate_old_presets() {
        for args in [
            vec!["breathing"],
            vec!["breathing", "70", "2"],
            vec!["--effect", "breathing", "--speed", "2"],
        ] {
            let p = parsed(&args).unwrap();
            assert!(p.colors.is_empty());
            assert_eq!(p.report(4, [0; 3], 15).unwrap()[5], 1);
            assert_eq!(Preset::deserialize(&p.serialize()).unwrap(), p);
        }
        for args in [
            vec!["breathing", "0088ff"],
            vec!["breathing", "--color", "0088ff"],
            vec!["--effect", "breathing", "--colors", "0088ff"],
        ] {
            assert!(parsed(&args)
                .unwrap_err()
                .to_string()
                .contains("no color argument"));
        }
        assert!(parsed(&["breathing", "000000"]).is_err());
        let old = "version=1\neffect=breathing\ncolors=00A0FF\nbrightness=60\nspeed=2\ndirection=right\nduty=0.5\nduration=30\n";
        let p = Preset::deserialize(old).unwrap();
        assert!(p.colors.is_empty());
        assert_eq!(
            p.report(4, [0; 3], 15).unwrap(),
            [0xa4, 0x21, 4, 60, 2, 1, 0, 0, 0, 15, 0]
        );
        for effect in [
            "breathing",
            "neon",
            "wave",
            "shifting",
            "zoom",
            "meteor",
            "twinkling",
            "rainbow",
        ] {
            assert!(parsed(&[effect, "--color", "FF0000"]).is_err());
            assert!(parsed(&[effect, "60", "3"]).unwrap().colors.is_empty());
        }
        assert!(Preset::deserialize(&old.replace("colors=00A0FF", "colors=")).is_ok());
        assert!(Preset::deserialize(
            &old.replace("colors=00A0FF", "colors=")
                .replace("effect=breathing", "effect=static")
        )
        .is_err());
    }
    #[test]
    fn named_options_still_work_and_conflicts_are_explained() {
        assert_eq!(
            parsed(&[
                "--effect",
                "strobe",
                "--colors",
                "ffffff",
                "--brightness",
                "80",
                "--speed",
                "2"
            ])
            .unwrap(),
            parsed(&["strobe", "ffffff", "80", "2"]).unwrap()
        );
        assert!(parsed(&["breathing", "80", "--brightness", "90"])
            .unwrap_err()
            .to_string()
            .contains("Duplicate"));
        assert!(parsed(&["breathing", "--speed"])
            .unwrap_err()
            .to_string()
            .contains("Missing value"));
        assert!(parsed(&["--brightness", "80"])
            .unwrap_err()
            .to_string()
            .contains("Choose an effect"));
        assert!(parsed(&["breathing", "101"]).is_err());
        assert!(parsed(&["static", "80", "3"]).is_err());
    }
    #[test]
    fn presets_round_trip_without_executable_content() {
        let preset = Preset::default();
        assert_eq!(Preset::deserialize(&preset.serialize()).unwrap(), preset);
        for text in [
            "version=1\neffect=$(reboot)",
            "version=1\nversion=1",
            "effect=strobe",
        ] {
            assert!(Preset::deserialize(text).is_err());
        }
        assert!(preset_path("../../etc/passwd").is_err());
        assert!(preset_path("night-mode_1").is_ok());
    }
    #[test]
    fn settings_are_validated_per_effect() {
        let mut p = Preset {
            effect: "strobe".into(),
            ..Preset::default()
        };
        p.speed = f64::NAN;
        assert!(p.validate().is_err());
        p.speed = 6.0;
        assert!(p.validate().is_err());
        p.speed = 2.0;
        assert!(p.validate().is_ok());
        p.duty = 1.0;
        assert!(p.validate().is_err());
        p.duty = 0.5;
        p.effect = "rainbow".into();
        p.speed = 0.1;
        assert!(p.validate().is_err());
        let args = ["--effect", "static", "--speed", "3"].map(String::from);
        assert!(Preset::parse(&args).is_err());
    }
    #[test]
    fn rainbow_color_wheel() {
        assert_eq!(color_wheel(0.0), [255, 0, 0]);
        assert_eq!(color_wheel(1.0 / 3.0), [0, 255, 0]);
        assert_eq!(color_wheel(2.0 / 3.0), [0, 0, 255]);
        assert_eq!(color_wheel(1.0), color_wheel(0.0));
    }
    #[test]
    fn captured_native_modes_keep_parameters_but_no_color_selection() {
        for (effect, mode) in [
            ("shifting", 8),
            ("zoom", 9),
            ("meteor", 10),
            ("twinkling", 11),
        ] {
            let p = parsed(&[effect, "100", "9"]).unwrap();
            assert_eq!(
                p.report(mode, [255, 0, 0], 15).unwrap(),
                [0xa4, 0x21, mode, 100, 9, 1, 0, 0, 0, 15, 0]
            );
        }
        let neon = parsed(&["neon", "100", "5"]).unwrap();
        assert_eq!(
            neon.report(5, [0; 3], 15).unwrap(),
            [0xa4, 0x21, 5, 100, 5, 1, 0, 0, 0, 15, 0]
        );
        let wave = parsed(&["wave", "100", "9", "--direction", "left"]).unwrap();
        assert_eq!(
            wave.report(7, [0; 3], 15).unwrap(),
            [0xa4, 0x21, 7, 100, 9, 2, 0, 0, 0, 15, 0]
        );
    }
    #[test]
    fn static_and_strobe_keep_one_or_four_colors() {
        for effect in ["static", "strobe"] {
            let one = parsed(&[effect, "123456", "60"]).unwrap();
            assert_eq!(one.colors, [[0x12, 0x34, 0x56]]);
            assert_eq!(Preset::deserialize(&one.serialize()).unwrap(), one);
            let four = parsed(&[effect, "FF0000,00FF00,0000FF,FFFFFF", "60"]).unwrap();
            assert_eq!(
                four.colors,
                [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255; 3]]
            );
            assert_eq!(Preset::deserialize(&four.serialize()).unwrap(), four);
            for count in [2, 3, 5, 16] {
                let colors = vec!["123456"; count].join(",");
                assert!(parsed(&[effect, &colors]).is_err());
            }
            assert_eq!(
                &one.report(2, one.colors[0], 15).unwrap()[6..9],
                &[0x12, 0x34, 0x56]
            );
        }
        let old="version=1\neffect=shifting\ncolors=FF0000\nbrightness=60\nspeed=3\ndirection=right\nduty=0.5\nduration=30\n";
        assert!(Preset::deserialize(old).unwrap().colors.is_empty());
        assert!(parsed(&["off", "FF0000"]).is_err());
    }
}
