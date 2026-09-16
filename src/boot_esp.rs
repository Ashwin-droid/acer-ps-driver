//! Narrow userspace file operations on this machine's original Acer ESP.
use crate::boot_media::{self, Result};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};
const PARTUUID: &str = "c1a78bb8-4f60-4b7b-8f47-2a1ba261c29d";
const STORE: &str = "/var/lib/aps/boot-logo";
const NAMES: [&str; 2] = ["AcerLogo.jpg", "AcerLogo.gif"];
fn protected_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() && m.uid() == 0 && m.mode() & 0o022 == 0 => Ok(()),
        Ok(_) => Err(format!("Unsafe boot backup directory: {}", path.display()).into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::DirBuilder::new().mode(0o700).create(path)?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}
pub(crate) fn model_check() -> Result<()> {
    for (field, value) in [
        ("sys_vendor", "Acer"),
        ("product_name", "Predator PHN16S-71"),
        ("board_name", "FTYPE_ARX"),
        ("bios_version", "V1.28"),
    ] {
        if fs::read_to_string(format!("/sys/class/dmi/id/{field}"))?.trim() != value {
            return Err("Unsupported model/BIOS".into());
        }
    }
    Ok(())
}
struct Esp {
    device: PathBuf,
}
impl Esp {
    fn open() -> Result<Self> {
        let device = fs::canonicalize(format!("/dev/disk/by-partuuid/{PARTUUID}"))?;
        let meta = fs::metadata(&device)?;
        if !meta.file_type().is_block_device() {
            return Err("Acer ESP is not a block device".into());
        }
        let number = format!("{}:{}", libc::major(meta.rdev()), libc::minor(meta.rdev()));
        let uevent = fs::read_to_string(format!("/sys/dev/block/{number}/uevent"))?;
        if !uevent
            .lines()
            .any(|line| line == format!("PARTUUID={PARTUUID}"))
        {
            return Err("Acer ESP identity mismatch".into());
        }
        let mountinfo = fs::read_to_string("/proc/self/mountinfo")?;
        if mountinfo
            .lines()
            .any(|line| line.split_whitespace().nth(2) == Some(number.as_str()))
        {
            return Err(
                "Acer ESP is mounted; refusing raw filesystem access. Unmount it first.".into(),
            );
        }
        let output = Command::new("/usr/sbin/blkid")
            .args([
                "-p",
                "-s",
                "TYPE",
                "-s",
                "LABEL",
                "-s",
                "PART_ENTRY_TYPE",
                "-o",
                "export",
            ])
            .arg(&device)
            .env_clear()
            .output()?;
        let fields = String::from_utf8(output.stdout)?;
        if !output.status.success()
            || !fields.lines().any(|l| l == "TYPE=vfat")
            || !fields.lines().any(|l| l == "LABEL=ESP")
            || !fields
                .lines()
                .any(|l| l == "PART_ENTRY_TYPE=c12a7328-f81f-11d2-ba4b-00a0c93ec93b")
        {
            return Err("Expected Acer's FAT EFI System Partition labeled ESP".into());
        }
        let esp = Self { device };
        esp.present()?;
        Ok(esp)
    }
    fn run(&self, tool: &str, args: &[&OsStr]) -> Result<Output> {
        // tool and DOS paths are internal constants, never caller-controlled.
        let output = Command::new(format!("/usr/bin/{tool}"))
            .arg("-i")
            .arg(&self.device)
            .args(args)
            .env_clear()
            .env("MTOOLSRC", "/dev/null")
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "{tool} failed: {}",
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(1000)
                    .collect::<String>()
            )
            .into());
        }
        Ok(output)
    }
    fn present(&self) -> Result<[bool; 2]> {
        let output = self.run("mdir", &["-b".as_ref(), "::/EFI/OEM".as_ref()])?;
        let text = String::from_utf8(output.stdout)?;
        Ok(NAMES.map(|name| {
            text.lines().any(|line| {
                line.trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .is_some_and(|last| last.eq_ignore_ascii_case(name))
            })
        }))
    }
    fn dos(name: &str) -> OsString {
        format!("::/EFI/OEM/{name}").into()
    }
    fn export(&self, name: &str, dest: &Path) -> Result<()> {
        self.run("mcopy", &[Self::dos(name).as_ref(), dest.as_os_str()])?;
        Ok(())
    }
    fn import(&self, source: &Path, name: &str) -> Result<()> {
        self.run(
            "mcopy",
            &["-o".as_ref(), source.as_os_str(), Self::dos(name).as_ref()],
        )?;
        Ok(())
    }
    fn rename(&self, old: &str, new: &str) -> Result<()> {
        self.run("mren", &[Self::dos(old).as_ref(), Self::dos(new).as_ref()])?;
        Ok(())
    }
    fn delete(&self, name: &str) -> Result<()> {
        self.run("mdel", &[Self::dos(name).as_ref()])?;
        Ok(())
    }
}

// Testable transaction; production callers supply only a verified Esp and a
// root-controlled backup directory. Files outside EFI/OEM are never destinations.
fn change(esp: &Esp, store: &Path, image: Option<&[u8]>) -> Result<Option<PathBuf>> {
    change_with_checkpoint(esp, store, image, || Ok(()))
}
fn change_with_checkpoint(
    esp: &Esp,
    store: &Path,
    image: Option<&[u8]>,
    checkpoint: impl FnOnce() -> Result<()>,
) -> Result<Option<PathBuf>> {
    let extension = image.map(boot_media::validate).transpose()?;
    let before = esp.present()?;
    if image.is_none() && before == [false, false] {
        return Ok(None);
    }
    let backup = store.join(format!(
        "{}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
        std::process::id()
    ));
    fs::DirBuilder::new().mode(0o700).create(&backup)?;
    // Complete all backups before touching any firmware-visible filename.
    for (index, name) in NAMES.iter().enumerate() {
        if before[index] {
            esp.export(name, &backup.join(name))?;
            File::open(backup.join(name))?.sync_all()?;
        }
    }
    fs::write(
        backup.join("manifest"),
        format!(
            "Acer boot logo backup\npartition={PARTUUID}\njpg={}\ngif={}\n",
            before[0], before[1]
        ),
    )?;
    File::open(backup.join("manifest"))?.sync_all()?;
    File::open(&backup)?.sync_all()?;
    File::open(store)?.sync_all()?;
    let suffix = backup.file_name().unwrap().to_string_lossy();
    let old = [
        format!("APS-{suffix}-old.jpg"),
        format!("APS-{suffix}-old.gif"),
    ];
    let new = format!("APS-{suffix}-new.{}", extension.unwrap_or("jpg"));
    let mut moved = [false; 2];
    let mut staged = false;
    let result = (|| -> Result<()> {
        if let Some(bytes) = image {
            let source = backup.join("new-image");
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&source)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            staged = true;
            esp.import(&source, &new)?;
            let check = backup.join("staged-readback");
            esp.export(&new, &check)?;
            if fs::read(check)? != bytes {
                return Err("ESP staged image readback mismatch".into());
            }
        }
        for index in 0..2 {
            if before[index] {
                esp.rename(NAMES[index], &old[index])?;
                moved[index] = true;
            }
        }
        checkpoint()?;
        if let Some(extension) = extension {
            let target = format!("AcerLogo.{extension}");
            esp.rename(&new, &target)?;
            staged = false;
            let check = backup.join("final-readback");
            esp.export(&target, &check)?;
            if fs::read(check)?.as_slice() != image.unwrap() {
                return Err("ESP final image readback mismatch".into());
            }
        }
        File::open(&esp.device)?.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        // Restore known old bytes, not an inferred firmware setting. Keep backup
        // files even if recovery itself fails, and report their exact location.
        let mut recovery = Vec::new();
        if staged {
            let _ = esp.delete(&new);
        }
        if let Ok(present) = esp.present() {
            for index in 0..2 {
                let restored = if moved[index] {
                    if present[index] {
                        let _ = esp.delete(NAMES[index]);
                    }
                    esp.rename(&old[index], NAMES[index])
                        .or_else(|_| esp.import(&backup.join(NAMES[index]), NAMES[index]))
                } else if present[index] {
                    if !before[index] {
                        esp.delete(NAMES[index])
                    } else {
                        Ok(())
                    }
                } else {
                    Ok(())
                };
                if let Err(e) = restored {
                    recovery.push(e.to_string());
                }
            }
        } else {
            recovery.push("ESP could not be read during recovery".into());
        }
        let _ = File::open(&esp.device).and_then(|f| f.sync_all());
        return Err(format!(
            "{error}; previous logo recovery errors: {recovery:?}; backup: {}",
            backup.display()
        )
        .into());
    }
    for index in 0..2 {
        if moved[index] {
            if let Err(e) = esp.delete(&old[index]) {
                eprintln!("Temporary old copy retained: {e}");
            }
        }
    }
    if staged {
        let _ = esp.delete(&new);
    }
    File::open(&esp.device)?.sync_all()?;
    Ok(Some(backup))
}

pub(crate) fn run(args: &[String]) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("Boot helper requires root".into());
    }
    model_check()?;
    match args {
        [cmd, value] if cmd == "effects" => {
            let value =
                crate::boot_policy::Effects::parse(value.as_bytes()).ok_or("Choose on or off")?;
            fs::write("/sys/class/misc/acer-phn16s/boot_effects", value.name())?;
            println!(
                "Boot animation/effects: {}. Applies at next boot.",
                value.name()
            );
            return Ok(());
        }
        [cmd] if cmd == "status" => {
            let present = Esp::open()?.present()?;
            println!(
                "Custom boot logo: {}",
                match present {
                    [false, false] => "default",
                    [true, false] => "JPEG",
                    [false, true] => "GIF",
                    _ => "both JPEG and GIF present",
                }
            );
            return Ok(());
        }
        [cmd] if cmd == "reset" => {}
        [cmd, _] if cmd == "install" => {}
        _ => {
            return Err(
                "Usage: aps-boot effects on|off | status | install PREPARED_IMAGE | reset".into(),
            )
        }
    }
    let bytes = if args[0] == "install" {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&args[1])?;
        let m = file.metadata()?;
        if !m.is_file() || m.len() > boot_media::MAX_BYTES as u64 {
            return Err("Invalid prepared image file".into());
        }
        let mut data = Vec::new();
        file.take(boot_media::MAX_BYTES as u64 + 1)
            .read_to_end(&mut data)?;
        boot_media::validate(&data)?;
        Some(data)
    } else {
        None
    };
    for path in ["/var", "/var/lib", "/var/lib/aps", STORE] {
        protected_dir(Path::new(path))?;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(Path::new(STORE).join("lock"))?;
    let m = lock.metadata()?;
    if !m.is_file() || m.uid() != 0 || m.nlink() != 1 || m.mode() & 0o077 != 0 {
        return Err("Unsafe boot-logo lock".into());
    }
    lock.lock()?;
    let esp = Esp::open()?;
    let backup = change(&esp, Path::new(STORE), bytes.as_deref())?;
    println!(
        "Boot logo {}. Applies at next boot.",
        if bytes.is_some() {
            "installed"
        } else {
            "reset to default"
        }
    );
    if let Some(backup) = backup {
        println!("Previous custom files backed up at {}", backup.display());
    }
    if bytes.is_some()
        && fs::read_to_string("/sys/class/misc/acer-phn16s/boot_effects")
            .is_ok_and(|value| value.trim() == "off")
    {
        println!("Boot effects are off. Enable them with: aps boot effects on");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replace_and_reset_on_disposable_fat_image_not_real_esp() {
        let dir = tempfile::tempdir().unwrap();
        let device = dir.path().join("fake-esp.img");
        File::create(&device)
            .unwrap()
            .set_len(64 * 1024 * 1024)
            .unwrap();
        assert!(Command::new("/usr/bin/mformat")
            .arg("-i")
            .arg(&device)
            .args(["-F", "::"])
            .status()
            .unwrap()
            .success());
        let esp = Esp { device };
        esp.run("mmd", &["::/EFI".as_ref(), "::/EFI/OEM".as_ref()])
            .unwrap();
        let old = dir.path().join("old.jpg");
        image::RgbImage::new(16, 10).save(&old).unwrap();
        let bytes = fs::read(&old).unwrap();
        esp.import(&old, "AcerLogo.jpg").unwrap();
        let backup = change(&esp, dir.path(), Some(&bytes)).unwrap().unwrap();
        assert_eq!(fs::read(backup.join("AcerLogo.jpg")).unwrap(), bytes);
        assert_eq!(esp.present().unwrap(), [true, false]);
        assert!(
            change_with_checkpoint(&esp, dir.path(), Some(&bytes), || Err(
                "injected pre-commit failure".into()
            ))
            .is_err()
        );
        let recovered = dir.path().join("recovered.jpg");
        esp.export("AcerLogo.jpg", &recovered).unwrap();
        assert_eq!(fs::read(recovered).unwrap(), bytes);
        assert!(change(&esp, dir.path(), Some(b"bad image")).is_err());
        assert_eq!(esp.present().unwrap(), [true, false]);
        change(&esp, dir.path(), None).unwrap();
        assert_eq!(esp.present().unwrap(), [false, false]);
        assert!(change(&esp, dir.path(), None).unwrap().is_none());
    }
}
