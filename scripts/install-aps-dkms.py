#!/usr/bin/python3
"""Install a root-owned, production-only source snapshot without unloading APS."""
import datetime
import hashlib
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("aps_dkms", ROOT / "packaging/dkms/aps_dkms.py")
dkms = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dkms)

FILES = [
    "kernel/Makefile", "kernel/acer_phn16s.rs", "kernel/acpi_controls.rs",
    "kernel/hardware_hid.rs", "kernel/hid_layout.h", "kernel/sense_key.rs",
    "kernel/sleep.rs", "kernel/turbo_button.rs", "src/control.rs", "src/protocol.rs",
    "src/boot_policy.rs", "src/mux.rs", "src/lid_state.rs", "src/sleep_state.rs",
    "research/firmware/acer-ec-hid-descriptor.bin", "research/firmware/enek5130-descriptor.bin",
    "packaging/dkms/aps_dkms.py", "packaging/dkms/check-install",
]


def install():
    dkms.require(os.geteuid() == 0, "Run this installer as root")
    release = dkms.kernel_name(os.uname().release)
    for name, expected in [("product_name", "Predator PHN16S-71"), ("board_name", "FTYPE_ARX"),
                           ("bios_version", "V1.28")]:
        dkms.require(Path(f"/sys/class/dmi/id/{name}").read_text().strip() == expected,
                     "This source snapshot is restricted to PHN16S-71 / FTYPE_ARX / V1.28")
    # Confirm the existing signing setup before DKMS could generate fallback keys.
    dkms.trusted(dkms.CERT)
    private = dkms.trusted("/var/lib/shim-signed/mok/MOK.priv")
    dkms.require(private.stat().st_mode & 0o077 == 0, "Signing key must not be accessible to other accounts")
    fingerprint = dkms.run(["openssl", "x509", "-inform", "DER", "-in", dkms.CERT,
                            "-noout", "-fingerprint", "-sha1"])
    dkms.require(fingerprint.split("=", 1)[-1] == dkms.FINGERPRINT, "Unexpected signing certificate")
    public_key = dkms.run(["openssl", "pkey", "-in", private, "-pubout"])
    certificate_key = dkms.run(["openssl", "x509", "-inform", "DER", "-in", dkms.CERT, "-pubkey", "-noout"])
    dkms.require(public_key == certificate_key, "Signing private key does not match the enrolled certificate")
    dkms.enrolled()
    destination = Path(f"/usr/src/{dkms.PACKAGE}-{dkms.VERSION}")
    fresh = not destination.exists()
    if fresh:
        destination.mkdir(mode=0o755)
    else:
        dkms.trusted(destination)
    for name in FILES:
        source = ROOT / name
        dkms.require(source.is_file() and not source.is_symlink(), f"Invalid source: {name}")
        target = destination / name
        if fresh:
            target.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
            shutil.copyfile(source, target)
            target.chmod(0o755 if name.endswith("check-install") else 0o644)
        else:
            dkms.require(dkms.trusted(target).read_bytes() == source.read_bytes(),
                         f"Source differs at {name}; use a new package version for updates")
    if fresh:
        shutil.copyfile(ROOT / "packaging/dkms/dkms.conf", destination / "dkms.conf")
        (destination / "dkms.conf").chmod(0o644)
    dkms.require((destination / "dkms.conf").read_bytes() == (ROOT / "packaging/dkms/dkms.conf").read_bytes(),
                 "Existing dkms.conf differs; use a new package version")
    # Explicit per-module signing, without changing NVIDIA/v4l2loopback policy.
    override = Path(f"/etc/dkms/{dkms.PACKAGE}.conf")
    policy = ('mok_signing_key="/var/lib/shim-signed/mok/MOK.priv"\n'
              'mok_certificate="/var/lib/shim-signed/mok/MOK.der"\n'
              'modprobe_on_install=""\n')
    if override.exists():
        dkms.require(dkms.trusted(override).read_text() == policy, f"Existing DKMS override needs review: {override}")
    else:
        override.write_text(policy)
        override.chmod(0o644)
    command = ["/usr/sbin/dkms"]
    common = ["-m", dkms.PACKAGE, "-v", dkms.VERSION]
    state = dkms.run([*command, "status", *common])
    if f"{dkms.PACKAGE}/{dkms.VERSION}, {release}, x86_64: installed" in state.splitlines():
        dkms.pre_install(release, "x86_64")
        print("APS DKMS is already installed with this exact source snapshot.")
        return
    if not state:
        dkms.run([*command, "add", *common], capture=False)
    dkms.run([*command, "build", *common, "-k", release], capture=False, timeout=900)
    dkms.pre_install(release, "x86_64")
    # Move the old manually-installed copy out of the module search path, only
    # after a signed replacement has built successfully. Keep it for recovery.
    old = Path(f"/lib/modules/{release}/updates/acer_phn16s.ko")
    backup = None
    if old.exists():
        dkms.trusted(old)
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        backup_dir = Path("/var/lib/aps/module-backups") / (release + "-" + stamp)
        backup_dir.mkdir(mode=0o700, parents=True)
        dkms.trusted(backup_dir)
        backup = backup_dir / old.name
        shutil.copy2(old, backup)
        dkms.require(hashlib.sha256(old.read_bytes()).digest() == hashlib.sha256(backup.read_bytes()).digest(),
                     "Module backup verification failed")
        old.unlink()
    try:
        dkms.run([*command, "install", *common, "-k", release], capture=False, timeout=600)
    except (OSError, subprocess.SubprocessError):
        if backup is not None:
            shutil.copy2(backup, old)
            dkms.run(["/usr/sbin/depmod", release])
        raise
    dkms.require("/updates/dkms/" in dkms.run(["modinfo", "-n", "acer_phn16s"]),
                 "Module resolution did not select the DKMS installation")
    print("APS DKMS installed. The running module was not unloaded or reloaded.")
    if backup is not None:
        print(f"Previous non-DKMS module retained at {backup}")


if __name__ == "__main__":
    try:
        install()
    except (RuntimeError, OSError, subprocess.SubprocessError) as error:
        print(f"APS DKMS installation stopped: {error}", file=sys.stderr)
        sys.exit(1)
