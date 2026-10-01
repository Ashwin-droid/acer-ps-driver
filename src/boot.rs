use crate::{
    boot_media::{self, Crop},
    boot_picker,
    boot_policy::Effects,
    Result,
};
use std::{fs, os::unix::fs::MetadataExt, path::Path, process::Command};
const HELPER: &str = "/usr/local/libexec/aps-boot";
pub(crate) fn help() {
    println!("APS system boot effect\n\n  aps boot status\n  aps boot effects on|off\n  aps boot logo pick [DIRECTORY]\n  aps boot logo set FILE\n  aps boot logo prepare FILE OUTPUT\n  aps boot logo status\n  aps boot logo reset\n\nExamples:\n  aps boot logo prepare clip.mp4 logo.gif\n  aps boot logo set clip.webm\n\nEffects controls the startup animation/effects master switch.\nBIOS has a separate sound mute setting; this command does not change it.\nPick opens a Ratatui image/video browser and first-frame crop preview.\nSet automatically center-crops. JPEG/PNG become JPEG. GIF and MP4/MOV,\nMKV/WebM or AVI become GIF, first 30s, up to 20fps, without audio.\nOutput starts at 1024x640; FPS and then resolution fall if needed to fit.\nAcer byte limit: 35,000,000. APS decoded GIF frame budget: 64 MiB.\nLocal input limit: 100 MiB. URLs, playlists and symlinks are rejected.\nPrepare exports a new local file; it never changes firmware or the ESP.\nConversion runs unprivileged; installation requests Polkit. No automatic reboot.\nLogo changes use Acer's original ESP, not the Linux bootloader partition.\nExisting custom logos are backed up before replacement or reset.");
}
fn helper(args: &[&std::ffi::OsStr]) -> Result<()> {
    for path in ["/usr", "/usr/local", "/usr/local/libexec", HELPER] {
        let m = fs::symlink_metadata(path)?;
        if m.uid() != 0
            || m.mode() & 0o022 != 0
            || if path == HELPER {
                !m.is_file()
            } else {
                !m.is_dir()
            }
        {
            return Err(
                "Boot helper must be installed root-owned, with protected parent directories"
                    .into(),
            );
        }
    }
    let root = unsafe { libc::geteuid() } == 0;
    let mut command = Command::new(if root { HELPER } else { "/usr/bin/pkexec" });
    if !root {
        command.arg(HELPER);
    }
    let status = command.args(args).status()?;
    match status.code() {
        Some(0) => Ok(()),
        Some(126) => Err("Authorization cancelled".into()),
        Some(127) => Err("Authorization denied or unavailable".into()),
        _ => Err("Boot operation failed; see the error above".into()),
    }
}
fn normal_user() -> Result<()> {
    if unsafe { libc::geteuid() } == 0 {
        Err("Run image conversion/picker without sudo; APS requests authorization only when installing".into())
    } else {
        Ok(())
    }
}
pub(crate) fn run(args: &[String]) -> Result<()> {
    if args.is_empty()
        || args
            .iter()
            .any(|s| matches!(s.as_str(), "help" | "--help" | "-h"))
    {
        help();
        return Ok(());
    }
    match args {
        [s] if s == "status" => {
            crate::model_check()?;
            println!(
                "Boot animation/effects: {}",
                fs::read_to_string("/sys/class/misc/acer-phn16s/boot_effects")?.trim()
            );
            Ok(())
        }
        [effect, value] if effect == "effects" => {
            let value = Effects::parse(value.as_bytes()).ok_or("Choose on or off")?;
            helper(&["effects".as_ref(), value.name().as_ref()])
        }
        [logo, action] if logo == "logo" && matches!(action.as_str(), "status" | "reset") => {
            helper(&[action.as_ref()])
        }
        [logo, prepare, input, output] if logo == "logo" && prepare == "prepare" => {
            normal_user()?;
            boot_media::prepare(Path::new(input), Path::new(output), Crop::default())?;
            println!("Prepared: {output}");
            Ok(())
        }
        [logo, set, input] if logo == "logo" && set == "set" => {
            normal_user()?;
            crate::model_check()?;
            let work = tempfile::tempdir()?;
            let output = work.path().join("prepared");
            boot_media::prepare(Path::new(input), &output, Crop::default())?;
            helper(&["install".as_ref(), output.as_os_str()])
        }
        [logo, pick, ..] if logo == "logo" && pick == "pick" && args.len() <= 3 => {
            normal_user()?;
            crate::model_check()?;
            let start = args.get(2).map(Path::new).unwrap_or(Path::new("."));
            if let Some((_work, output)) = boot_picker::pick(start)? {
                helper(&["install".as_ref(), output.as_os_str()])?;
            }
            Ok(())
        }
        _ => Err("Usage: aps boot --help".into()),
    }
}
