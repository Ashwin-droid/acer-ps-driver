# Kernel 7.2.6 DKMS compatibility, 2026-10-17

The 0.5.1 autoinstall failed for `7.2.6-76070206-generic`, package version
`7.2.6-76070206.202609141300~1792074708~26.04~18da6d1`. The log showed that the
exact Rust 1.91.1 compiler and matching distribution Rust metadata were found.
The first failure was standalone bindgen, not Rust compilation or module signing.

## Two compatibility changes

1. The new HID include chain reaches `arch/x86/include/asm/ftrace.h`, whose
   function-tracing guard requires `CC_USING_FENTRY`. The target kernel Makefile
   normally supplies `-mfentry` and `-DCC_USING_FENTRY`; our standalone header
   parser omitted them. The builder and local layout-generation script now
   include those flags when `CONFIG_HAVE_FENTRY=y`. Tracing configuration and
   every existing PH_FIELD ABI assertion remain enabled.
2. Once bindgen succeeded, Rust reported that `device_attribute` no longer has
   direct `show`/`store` fields. The 7.2 headers use
   `__SYSFS_FUNCTION_ALTERNATIVE` for legacy and const callbacks. The target's
   generated Rust bindings contain two anonymous unions, or structs under CFI.
   The build helper now examines that exact schema and supplies fixed cfg flags
   for typed initializers. Callback types are still checked by rustc. Unknown
   schemas fail closed; no raw offsets, casts or version-number guesses were
   added to sysfs attribute construction.

The installer also accepts `--kernel RELEASE`, and its final modinfo lookup now
uses that target instead of implicitly querying the running kernel. This permits
preparing and verifying a new kernel before rebooting into it.

## Verification and installation

- Unprivileged isolated build against the real 7.2.6 headers and matching cached
  Rust metadata passed after both changes, including MODPOST.
- A build against the running 7.1.5 headers also passed.
- 157 host Rust tests, Clippy with warnings denied, and 18 DKMS policy tests passed.
- Root-owned production source snapshot `acer-phn16s/0.5.2` built and installed
  successfully through DKMS for both kernels. Both modules were signed with the
  enrolled Ashwin Module Signing Key; installed bytes matched the DKMS builds.
- The new-kernel module reports vermagic `7.2.6-76070206-generic`. Its compressed
  SHA256 is `938830beb1edfd15b8b85d28edb71a04c62f40e9c68eb23ed6e54d0a3b1dc693`.
- Only the five interrupted kernel packages were configured: the 7.2.6 generic
  headers/image, linux-headers-generic, linux-generic and linux-system76.
  All finished with status `ii`; dpkg audit returned no pending problems.
  Package hooks regenerated the new initramfs and GRUB configuration.

The running kernel stayed at 7.1.5. No module unload/reload, reboot, or firmware
setting change was performed. Its loaded build ID remained
`897537298144740bfbb486450cabc99e6928659d`, with Eco/Auto and dev_mode=0 unchanged.
Runtime behavior on 7.2.6 remains to be checked after the user boots it. The CFI
struct-layout selector has unit coverage, not a CFI-enabled kernel build result.
