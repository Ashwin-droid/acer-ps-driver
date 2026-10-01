#!/usr/bin/python3
"""Production-only DKMS build. No workspace toolchain or nested apt installation."""
import hashlib
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

PACKAGE = "acer-phn16s"
VERSION = "0.4.0"
ENV = {"PATH": "/usr/sbin:/usr/bin:/sbin:/bin", "LC_ALL": "C"}
CACHE = Path("/var/cache/aps-dkms")
CERT = Path("/var/lib/shim-signed/mok/MOK.der")
FINGERPRINT = "F3:2F:08:C3:D8:29:C6:54:C5:A8:09:AB:CF:59:90:44:03:65:47:EA"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def kernel_name(value):
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+[-+.A-Za-z0-9_]*", value),
            "Invalid kernel release")
    return value


def trusted(path):
    """Reject user-writable targets, symlinks, and ancestor directories."""
    path = Path(path).absolute()
    for item in (path, *path.parents):
        info = item.lstat()
        require(info.st_uid == 0 and (stat.S_ISLNK(info.st_mode) or not info.st_mode & 0o022),
                f"Not root-owned/protected: {item}")
    resolved = path.resolve(strict=True)
    for item in (resolved, *resolved.parents):
        info = item.stat()
        require(info.st_uid == 0 and not info.st_mode & 0o022,
                f"Not root-owned/protected: {item}")
    return resolved


def run(args, *, cwd=None, capture=True, timeout=300):
    result = subprocess.run([str(arg) for arg in args], cwd=cwd, env=ENV,
                            check=True, text=True, stdout=subprocess.PIPE if capture else None,
                            timeout=timeout)
    return result.stdout.strip() if capture else None


def config(path):
    values = {}
    for line in path.read_text().splitlines():
        if line.startswith("CONFIG_") and "=" in line:
            key, value = line.split("=", 1)
            values[key] = value.strip('"')
    return values


def enrolled():
    # This distribution's mokutil --test-key returns 1 even for "already
    # enrolled". Match the enrolled list's exact certificate fingerprint.
    listing = run(["/usr/bin/mokutil", "--list-enrolled"])
    require(re.search(r"^SHA1 Fingerprint:\s*" + re.escape(FINGERPRINT) + r"\s*$",
                      listing, re.IGNORECASE | re.MULTILINE), "APS signing certificate is not enrolled")


def rust_version(values):
    expected = values.get("CONFIG_RUSTC_VERSION_TEXT", "")
    match = re.fullmatch(r"rustc (\d+)\.(\d+)\.(\d+) .+", expected)
    require(match, "Target headers do not record a complete Rust compiler version")
    return expected, f"{match[1]}.{match[2]}"


def compiler(values):
    expected, series = rust_version(values)
    for executable in [Path(f"/usr/bin/rustc-{series}"), Path("/usr/bin/rustc")]:
        if executable.exists() and run([trusted(executable), "--version"]) == expected:
            return executable
    raise RuntimeError(f"No matching system compiler for {expected}. Install the matching "
                       f"distribution rustc-{series} package before retrying dkms autoinstall. "
                       "Refusing workspace/rustup compiler or an incompatible version.")


def metadata_files(directory):
    for name in ["libkernel.rmeta", "libcore.rmeta", "libbindings.rmeta", "libmacros.so"]:
        trusted(directory / name)
    return directory


def download_metadata(release, version, cache=CACHE):
    """APT download verifies repository package hashes; dpkg is never invoked to install.

    Headers' postinst runs inside a dpkg transaction. Download+extract into our
    private cache avoids deadlocking its package database or changing system files.
    """
    require(re.fullmatch(r"[A-Za-z0-9.+:~_-]+", version), "Invalid header package version")
    package = f"linux-lib-rust-{release}"
    cache.mkdir(mode=0o755, parents=True, exist_ok=True)
    trusted(cache)
    key = hashlib.sha256(version.encode()).hexdigest()[:16]
    destination = cache / f"{release}-{key}"
    relative = Path("usr/src") / package / "rust"
    if destination.exists():
        return metadata_files(trusted(destination / relative))
    with tempfile.TemporaryDirectory(prefix=".metadata-", dir=cache) as temp:
        temp = Path(temp)
        # Permit APT's download sandbox to traverse only this temporary directory.
        temp.chmod(0o755)
        run(["/usr/bin/apt-get", "-o", "APT::Get::AllowUnauthenticated=false",
             "-o", "Acquire::AllowInsecureRepositories=false",
             "-o", "Acquire::AllowDowngradeToInsecureRepositories=false",
             "-o", "Acquire::Retries=2", "-o", "Acquire::http::Timeout=30",
             "-o", "Acquire::https::Timeout=30", "download", f"{package}={version}"],
            cwd=temp, capture=False)
        archives = list(temp.glob("*.deb"))
        require(len(archives) == 1, "Expected exactly one Rust metadata package")
        archive = archives[0]
        for field, expected in [("Package", package), ("Version", version), ("Architecture", "amd64")]:
            require(run(["/usr/bin/dpkg-deb", "--field", archive, field]) == expected,
                    f"Rust metadata package {field} mismatch")
        extracted = temp / "extracted"
        run(["/usr/bin/dpkg-deb", "--extract", archive, extracted])
        metadata_files(extracted / relative)
        extracted.rename(destination)
    return metadata_files(destination / relative)


def metadata(release, headers):
    package = f"linux-lib-rust-{release}"
    # Match the exact distribution build, not just the upstream release number.
    header_version = run(["/usr/bin/dpkg-query", "-W", "-f=${Version}", f"linux-headers-{release}"])
    rust_dir = headers / "rust"
    if (rust_dir / "libkernel.rmeta").exists():
        installed = run(["/usr/bin/dpkg-query", "-W", "-f=${Version}", package])
        require(installed == header_version, "Installed Rust metadata does not match header package version")
        return metadata_files(trusted(rust_dir))
    print(f"APS DKMS: fetching matching {package}={header_version}", flush=True)
    return download_metadata(release, header_version)


def clang_args(headers):
    paths = ["arch/x86/include", "arch/x86/include/generated", "include",
             "arch/x86/include/uapi", "arch/x86/include/generated/uapi",
             "include/uapi", "include/generated/uapi"]
    return ["-D__KERNEL__", "-DMODULE", "-funsigned-char", "-fno-builtin", "-fms-extensions",
            *[f"-I{headers / path}" for path in paths],
            "-include", str(headers / "include/linux/kconfig.h"),
            "-include", str(headers / "include/linux/compiler_types.h")]


def check_module(module, release, signed=False):
    require(run(["/usr/sbin/modinfo", "-F", "name", module]) == "acer_phn16s", "Wrong module name")
    vermagic = run(["/usr/sbin/modinfo", "-F", "vermagic", module])
    require(vermagic.split()[0] == release, "Module built for wrong kernel")
    require(not run(["/usr/sbin/modinfo", "-p", module]), "Unexpected module parameters")
    # modinfo/nm support compressed modules differently: check raw build before
    # signing, and the final signer's identity independently before installation.
    if not signed:
        data = module.read_bytes()
        require(all(marker not in data for marker in [b"dev_probe\0", b"dev_challenge\0", b"APSDEV01"]),
                "Developer probe interface found in production module")
        imports = run(["/usr/bin/nm", "-u", module])
        require("public_key_verify_signature" not in imports, "Developer signature verifier is present")
    else:
        require(run(["/usr/sbin/modinfo", "-F", "signer", module]) == "Ashwin Module Signing Key",
                "Refusing unsigned module or unexpected signer")
        fingerprint = run(["/usr/bin/openssl", "x509", "-inform", "DER", "-in", trusted(CERT),
                           "-noout", "-fingerprint", "-sha1"])
        require(fingerprint.split("=", 1)[-1] == FINGERPRINT, "Unexpected MOK certificate")
        enrolled()


def build(release):
    root = Path(__file__).resolve().parents[2]
    trusted(root)
    kernel = root / "kernel"
    # Never allow an old .ko/layout to survive a failed prerequisite check.
    for name in ["acer_phn16s.ko", "hid_layout.rs"]:
        (kernel / name).unlink(missing_ok=True)
    headers = trusted(Path("/lib/modules") / release / "build")
    require((headers / "include/config/kernel.release").read_text().strip() == release,
            "Target headers report a different kernel release")
    values = config(headers / ".config")
    for name in ["CONFIG_X86_64", "CONFIG_RUST", "CONFIG_ACPI", "CONFIG_HID", "CONFIG_INPUT", "CONFIG_PM_SLEEP"]:
        require(values.get(name) in ("y", "m"), f"Target kernel lacks {name}")
    rustc = compiler(values)
    rust_dir = metadata(release, headers)
    bindgen = trusted("/usr/bin/bindgen")
    print(f"APS DKMS: kernel={release}; compiler={run([rustc, '--version'])}; metadata={rust_dir}", flush=True)
    with tempfile.TemporaryDirectory(prefix=".abi-", dir=kernel) as temp:
        output = Path(temp) / "hid_layout.rs"
        run([bindgen, kernel / "hid_layout.h", "--formatter", "none", "--use-core", "--allowlist-var", "PH_.*",
             "--no-recursive-allowlist", "--no-layout-tests", "--no-doc-comments",
             "--output", output, "--", *clang_args(headers)], capture=False)
        expected = set(re.findall(r"\bPH_[A-Z0-9_]+\s*=", (kernel / "hid_layout.h").read_text()))
        found = set(re.findall(r"pub\s+const\s+(PH_[A-Z0-9_]+)\s*:", output.read_text()))
        require({value.rstrip(" =\t\n") for value in expected} == found, "Incomplete generated HID layout")
        output.rename(kernel / "hid_layout.rs")
    print("APS DKMS: regenerated ABI layout sha256=" +
          hashlib.sha256((kernel / "hid_layout.rs").read_bytes()).hexdigest(), flush=True)
    command = ["/usr/bin/make", "-C", headers, f"M={kernel}",
               f"RUSTC={rustc}", f"RUSTFLAGS_MODULE=-L {rust_dir}",
               f"-j{min(os.cpu_count() or 1, 8)}", "modules"]
    cc = values.get("CONFIG_CC_VERSION_TEXT", "").split(" ", 1)[0]
    if re.fullmatch(r"[A-Za-z0-9_.+-]+", cc) and (Path("/usr/bin") / cc).exists():
        command.append(f"CC=/usr/bin/{cc}")
    if values.get("CONFIG_LD_IS_LLD") == "y":
        command.append("LD=/usr/bin/ld.lld")
    run(command, capture=False, timeout=600)
    check_module(kernel / "acer_phn16s.ko", release)
    print("APS DKMS: production build verified; DKMS signs next", flush=True)


def pre_install(release, arch):
    require(arch == "x86_64", "Unsupported module architecture")
    directory = trusted(Path("/var/lib/dkms") / PACKAGE / VERSION / release / arch / "module")
    modules = [file for file in directory.glob("acer_phn16s.ko*") if file.is_file()]
    require(len(modules) == 1, "Expected exactly one signed DKMS module")
    check_module(trusted(modules[0]), release, signed=True)
    print("APS DKMS: enrolled signing certificate and module signer verified")


def main(args):
    require(os.geteuid() == 0, "DKMS helper must run as root")
    require(len(args) in (2, 3), "Use build KERNEL or check-install KERNEL ARCH")
    release = kernel_name(args[1])
    if args[0] == "build" and len(args) == 2:
        build(release)
    elif args[0] == "check-install" and len(args) == 3:
        pre_install(release, args[2])
    else:
        raise RuntimeError("Unknown DKMS helper operation")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (RuntimeError, OSError, subprocess.SubprocessError) as error:
        print(f"APS DKMS: {error}", file=sys.stderr)
        sys.exit(1)
