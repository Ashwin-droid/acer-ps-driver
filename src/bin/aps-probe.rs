//! Explicit development tool. OpenSSL signs; the kernel verifies before firmware I/O.
#[path = "../dev_protocol.rs"]
mod wire;
use std::{
    env, fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const ROOT: &str = "/sys/class/misc/acer-phn16s";
fn key_path() -> Result<PathBuf> {
    let config = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or(PathBuf::from(env::var_os("HOME").ok_or("HOME missing")?).join(".config"));
    Ok(config.join("aps/dev-signing/private.pem"))
}
fn uid() -> Result<u32> {
    Ok(fs::metadata("/proc/self")?.uid())
}
fn private(path: &Path, directory: bool) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.uid() != uid()?
        || m.mode() & 0o077 != 0
        || m.file_type().is_symlink()
        || (directory && !m.is_dir())
        || (!directory && (!m.is_file() || m.nlink() != 1 || m.len() > 8192))
    {
        return Err("developer key must be a private regular file owned by this account".into());
    }
    Ok(())
}
fn key() -> Result<PathBuf> {
    let path = key_path()?;
    private(path.parent().ok_or("missing key directory")?, true)?;
    private(&path, false)?;
    Ok(path)
}
fn keygen() -> Result<()> {
    let path = key_path()?;
    let dir = path.parent().ok_or("missing key directory")?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    private(dir, true)?;
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)?;
    let result = Command::new("/usr/bin/openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
        ])
        .stdout(file.try_clone()?)
        .stderr(Stdio::null())
        .status()?;
    if !result.success() {
        return Err("developer key generation failed; no existing key was overwritten".into());
    }
    file.sync_all()?;
    private(&path, false)?;
    println!(
        "Created separate developer key at {}. Do not distribute it.",
        path.display()
    );
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn bytes(text: &str) -> Result<Vec<u8>> {
    if text.len() > 130 || !text.len().is_multiple_of(2) {
        return Err("hex payload must be at most 65 bytes".into());
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            wire::unhex::<1>(p)
                .map(|v| v[0])
                .ok_or("invalid hex payload".into())
        })
        .collect()
}
fn read(name: &str) -> Result<String> {
    Ok(fs::read_to_string(Path::new(ROOT).join(name))?)
}
fn challenge(p: &mut wire::Probe) -> Result<()> {
    if read("dev_mode")?.trim() != "1" {
        return Err("driver is a production build; rebuild with APS_DEV=1".into());
    }
    let text = read("dev_challenge")?;
    let parts: Vec<_> = text.split_ascii_whitespace().collect();
    if parts.len() != 3 {
        return Err("invalid driver challenge".into());
    }
    p.nonce = wire::unhex(parts[0].as_bytes()).ok_or("invalid nonce")?;
    p.sequence = parts[1].parse()?;
    p.expires = parts[2]
        .parse::<u64>()?
        .checked_add(10_000_000_000)
        .ok_or("clock overflow")?;
    Ok(())
}
fn signed(mut p: wire::Probe) -> Result<Vec<u8>> {
    // Pace consecutive CLI invocations; do not retry an ambiguous raw call.
    thread::sleep(Duration::from_millis(120));
    challenge(&mut p)?;
    if !p.valid() {
        return Err("invalid raw transport or packet bounds".into());
    }
    let msg = p.encode();
    let mut child = Command::new("/usr/bin/openssl")
        .args(["dgst", "-sha256", "-sign"])
        .arg(key()?)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("signer stdin missing")?
        .write_all(&msg)?;
    let output = child.wait_with_output()?;
    if !output.status.success() || output.stdout.len() != wire::SIGNATURE_LEN {
        return Err("RSA-2048 signing failed".into());
    }
    let mut packet = msg.to_vec();
    packet.extend(output.stdout);
    Ok(packet)
}
fn submit(packet: &[u8]) -> std::io::Result<()> {
    fs::write(Path::new(ROOT).join("dev_probe"), hex(packet))
}
fn send(p: wire::Probe) -> Result<()> {
    submit(&signed(p)?)?;
    print!("{}", read("dev_probe")?);
    Ok(())
}
fn selftest() -> Result<()> {
    let packet = signed(wire::Probe::empty())?;
    let mut forged = packet.clone();
    forged[wire::MESSAGE_LEN] ^= 1;
    let err = submit(&forged).expect_err("forged signature accepted");
    // RSA rejects malformed padding/representatives with EINVAL or EBADMSG,
    // and a digest mismatch with EKEYREJECTED. A later positive check is vital.
    if !matches!(err.raw_os_error(), Some(22 | 74 | 129)) {
        return Err(format!("expected signature rejection, got {err}").into());
    }
    thread::sleep(Duration::from_millis(120));
    let mut altered = packet.clone();
    altered[48] ^= 1;
    let err = submit(&altered).expect_err("altered signed message accepted");
    if !matches!(err.raw_os_error(), Some(22 | 74 | 129)) {
        return Err(format!("expected tamper rejection, got {err}").into());
    }
    thread::sleep(Duration::from_millis(120));
    submit(&packet)?;
    let err = submit(&packet).expect_err("replayed request accepted");
    if err.raw_os_error() != Some(116) {
        return Err(format!("expected stale rejection, got {err}").into());
    }
    println!("PASS: valid signature accepted; forged signature, modified message, and replay rejected. No firmware call made.");
    Ok(())
}
fn run() -> Result<()> {
    if uid()? == 0 {
        return Err("run aps-probe as your normal developer account, not root".into());
    }
    let args: Vec<_> = env::args().skip(1).collect();
    let mut p = wire::Probe::empty();
    match args.first().map(String::as_str) {
        Some("keygen") if args.len() == 1 => keygen(),
        Some("public-key") if args.len() == 1 => {
            let o = Command::new("/usr/bin/openssl")
                .args(["rsa", "-in"])
                .arg(key()?)
                .args(["-RSAPublicKey_out", "-outform", "DER"])
                .output()?;
            if !o.status.success() {
                return Err("public key export failed".into());
            }
            std::io::stdout().write_all(&o.stdout)?;
            Ok(())
        }
        Some("status") if args.len() == 1 => {
            println!("dev_mode={}", read("dev_mode")?.trim());
            if Path::new(ROOT).join("dev_probe").exists() {
                print!("{}", read("dev_probe")?);
            }
            Ok(())
        }
        Some("selftest") if args.len() == 1 => selftest(),
        Some("finish") if args.len() == 1 => {
            p.operation = 4;
            send(p)?;
            println!("Preference capture resumed. This does NOT undo raw firmware changes.");
            Ok(())
        }
        Some("restore") if args.len() == 1 => {
            if !Command::new("/usr/local/libexec/aps-state")
                .arg("restore")
                .status()?
                .success()
            {
                return Err(
                    "normal preferences could not be restored; dev session remains active".into(),
                );
            }
            p.operation = 4;
            send(p)
        }
        Some("hid") if args.len() == 4 => {
            p.operation = match args[1].as_str() {
                "lighting" => 1,
                "ec" => 2,
                _ => return Err("choose lighting or ec".into()),
            };
            p.action = match args[2].as_str() {
                "get" => 0,
                "set" => 1,
                _ => return Err("choose get or set".into()),
            };
            let data = bytes(&args[3])?;
            p.length = data.len() as u8;
            p.output_length = p.length;
            p.data[..data.len()].copy_from_slice(&data);
            send(p)
        }
        Some("wmi") if args.len() == 4 => {
            p.operation = 3;
            p.method = u32::from_str_radix(args[1].trim_start_matches("0x"), 16)?;
            let data = bytes(&args[2])?;
            p.length = data.len() as u8;
            p.data[..data.len()].copy_from_slice(&data);
            p.output_length = args[3].parse()?;
            send(p)
        }
        _ => {
            println!("APS developer probes (requires compile-time APS_DEV=1)\n\naps-probe keygen\naps-probe public-key  (DER on stdout)\naps-probe status\naps-probe selftest    (no firmware I/O)\naps-probe hid lighting|ec get|set HEX_INCLUDING_REPORT_ID\naps-probe wmi HEX_METHOD LITTLE_ENDIAN_INPUT_HEX OUTPUT_BYTES\naps-probe restore     (restore normal preferences, resume saving)\naps-probe finish      (resume saving WITHOUT undoing raw changes)\n\nRaw calls bypass normal control validation. A signature is authority, not a hardware safety guarantee. No automatic retries or rollback. Preference capture pauses until restore/finish. Never share the private developer key.");
            Ok(())
        }
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("aps-probe: {e}");
        std::process::exit(1);
    }
}
