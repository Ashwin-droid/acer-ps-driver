# APS DKMS source installation

`acer-phn16s/0.5.2` is the production-only DKMS package. It still targets
Predator PHN16S-71 / FTYPE_ARX / BIOS V1.28. It does not generalize firmware
commands to other Acer models or enable raw developer probes.

The source installer runs as root:

```bash
python3 scripts/install-aps-dkms.py
# Or prepare a different installed kernel without booting it:
python3 scripts/install-aps-dkms.py --kernel 7.2.6-76070206-generic
```

It copies a fixed manifest of driver sources, shared parsers, and the two HID
descriptor fixtures to `/usr/src/acer-phn16s-0.5.2`. Files there are root-owned
and not group/other-writable. It does not symlink the source to this workspace,
ship signing keys, or use compilers under a user's home directory.

The installer sets a 0022 umask so intermediate snapshot directories are also
protected. After validating the built module's signer and vermagic, it explicitly
replaces this module and compares the installed compressed bytes with the DKMS
build. This matters because Rust submodule edits can leave `srcversion` unchanged;
DKMS can otherwise skip the binary copy while reporting the new version installed.

0.5.2 fixes the tested 7.2.6 upgrade in two places. Standalone bindgen mirrors
Kbuild's fentry compiler option/define when the target config advertises it;
tracing and ABI assertions are not disabled. The builder also inspects the
target Rust metadata's `device_attribute` schema to select plain callback fields
or the newer anonymous callback alternatives. Rust still checks the exact field
types and callback signatures. Unknown layouts fail closed rather than falling
back to raw offsets or a kernel-version guess. Plain and union layouts were
built against the local 7.1.5 and 7.2.6 headers respectively; the CFI struct
selection has unit coverage but has not been built against a CFI-enabled kernel.

The installer checks module resolution and installed bytes for the selected
kernel, not necessarily the running one. It never unloads/reloads the running
module or reboots. Successful build/sign/install for a new kernel still needs
runtime verification after the user boots that kernel.

Prerequisites are DKMS, the target kernel's headers with Rust enabled, its exact
distribution Rust compiler, bindgen/libclang, and the existing enrolled Ashwin
Module Signing Key. The current kernel uses distribution `rustc-1.91` 1.91.1.
For the present Pop!_OS kernel, these were installed with:

```bash
apt-get --no-install-recommends install rustc-1.91 bindgen linux-lib-rust-7.1.5-76070105-generic
```

## Kernel updates

The normal DKMS kernel/header package hooks build and install APS automatically
for eligible new kernels. `AUTOINSTALL=yes`, x86_64, kernel >= 7.1.5, and the
required kernel configuration symbols are enforced by DKMS.

Each build:

1. Verifies target release/configuration and the complete compiler version.
2. Uses Rust metadata matching the exact header package version. If it is not
   installed, downloads that exact `linux-lib-rust-KERNEL=VERSION` through APT
   into `/var/cache/aps-dkms` and extracts it privately. It does not run package
   maintainer scripts or nest `apt install` inside the kernel's dpkg transaction.
   Missing metadata therefore needs network access and an available trusted
   repository package, unless the package/cache was prepared beforehand.
3. Regenerates `hid_layout.rs` from the target headers, including checks on field
   types and callback signatures used by the unsafe Rust transport. No stale
   layout or module is accepted after a failed prerequisite check.
4. Builds with no `aps_dev` flag and checks the artifact for developer endpoints.
5. Lets DKMS sign with the existing enrolled MOK. A pre-install check rejects an
   unsigned module, wrong signer, wrong kernel, or unexpected enrolled certificate.

An unavailable/mismatched compiler, missing metadata, or changed kernel API is a
build failure, not a reason to force-load the old module. If a later kernel moves
to a new Rust compiler series, install that matching distribution compiler and
retry. Automatic kernel rebuilding cannot guarantee compatibility with future
kernel APIs or toolchains. Build logs and failures remain visible through DKMS.

```bash
dkms status -m acer-phn16s
dkms autoinstall -k "$(uname -r)"
modinfo -n acer_phn16s
modinfo -F signer acer_phn16s
```

The installer builds and validates the replacement before moving the old manual
module into `/var/lib/aps/module-backups/`. It preserves that backup and restores
the old file if DKMS installation fails. It does not unload/reload the running
driver or restart the preference service. Existing autoload and sysfs permission
rules remain in place. This migration expects that one-time APS setup was already
performed; it is not a replacement for initial userspace/access-rule installation.

Source snapshots are versioned. Re-running with identical sources can resume a
failed installation; changed sources require a new package version. Development
builds remain a separate manual workflow and are never automatically installed.

The old `packaging/kernel/install.sh` is a fixed-kernel binary-bundle installer.
Do not use it over the DKMS installation: doing so would create an unmanaged copy.
The updated legacy installers refuse a DKMS-managed system before unloading the
module or stopping the preference service. Older already-exported archives do
not contain that guard and should not be used to update this installation.

Offline policy tests:

```bash
python3 -m unittest discover -s tests -p 'test_dkms.py' -v
```
