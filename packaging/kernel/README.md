# APS kernel bundle

Signed Rust `acer_phn16s.ko` and one-time setup/removal scripts. No userspace CLI
or private keys are included.

Check `BUILD_MODE` before installing. Dev builds expose signed raw firmware
probes; production builds compile that interface out. There is no runtime toggle.
Read-only `dev_mode` reports the build. See DEVELOPMENT.md for authorization and risks.

Supported: Acer Predator PHN16S-71, FTYPE_ARX, BIOS V1.28,
Linux 7.1.5-76070105-generic. Signed with Ashwin Module Signing Key. The installer
verifies the matching enrolled certificate before loading.

```bash
sudo ./install.sh ashwinmhaskar
```

The account's primary group receives access to these attributes under
`/sys/class/misc/acer-phn16s/`:

- Read: cpu_fan_rpm, gpu_fan_rpm, keyboard_capabilities, fan_lease_ms,
  button_presses, sense_presses, button_error, turbo_led_capabilities,
  keyboard_state, turbo_led_manual, preferences_generation, logo_capabilities,
  sleep_status, lid_state, lid_status, mux_request.
- Read/write: profile, fan_control, keyboard_effect, keyboard_timeout,
  button_mode, turbo_led, turbo_led_mode, logo, sleep_logo.
- User read / root write: mux_mode, mode 0640 with an additional kernel CAP_SYS_ADMIN check.
- User read / root write: boot_effects, the startup effects master switch, not the separate BIOS sound mute setting.

Attributes are root-owned, mode 0440 for reads and 0660 for ordinary controls.
MUX is the exception: mode 0640 at creation and after the boot/udev permission
rule, with a kernel CAP_SYS_ADMIN check in the initial user namespace on writes.
The CLI requests Polkit authorization for MUX writes unless already root, and
keeps status/lighting unprivileged. Raw HID
devices stay restricted. Production builds have no raw endpoint or developer-key
verifier. Dev builds add `dev_challenge` and `dev_probe` for signed raw HID/WMI
calls. Neither build has a socket or ioctl bypass. The kernel bounds/parses every write independently of
userspace and owns device identities, reports and ACPI method selection.
Normal unsupported operations and values fail before hardware I/O. Normal controls
have no LCD commands. Raw signed calls can have unrelated or harmful firmware effects.

The physical Turbo button can cycle power modes or emit F14; PredatorSense emits F15. Userspace has
no configurable-key or fake-press interface. The separate ModeKeyLED supports
native Auto following performance mode, or bounded manual RGB. HID/input handlers
and the virtual input device are removed during module teardown.

Fan overrides expire after five seconds without renewal; a kernel worker restores
Auto. Unload also requests Auto for an outstanding lease. Firmware failures are
logged/retried. A separate userspace service can restore preferences via these controls.

A kernel blocking PM notifier turns off keyboard and Turbo RGB before sleep.
The cover logo uses the memory-only `sleep_logo` preference during suspend:
off by default, static BRIGHTNESS RGB, or breathing/neon BRIGHTNESS SPEED.
Hibernation and image restore always turn all RGB off. Keyboard and Turbo remain
off during suspend regardless of the last lid reading. A failed sleep-logo write
triggers one off attempt and is logged, but does not veto sleep. Firmware can
override the requested effect during sleep; no physical readback is available.
Temporary lighting writes never change cached awake settings or preference
generation. Hardware reads and all writes return EBUSY while sleeping; cached
settings remain readable. The worker stops hardware/button actions during sleep,
and an active manual fan lease is returned to Auto before sleep.

After resume or an aborted suspend, the worker waits two seconds for the controller,
then restores the latest cached keyboard zones/effect, logo and finally Turbo LED.
Identical keyboard zone settings are combined into one report; four-color Static
remains four-color. Failures are logged and retried after two and five more seconds,
with at most three attempts. `sleep_status` reports phase, cycle count, attempts and
the last errno. No fake-suspend or raw-hardware test endpoint is exposed.
Settings never configured through APS cannot be recovered from the firmware and
remain off until configured; the normal boot preference service populates the cache.
Native effects restart their animation on wake. A running software effect still
needs its userspace process; the kernel restores its last accepted frame only.

While awake, the worker samples the kernel ACPI lid query every 500 ms. Closing
the lid switches off keyboard and Turbo RGB, without changing cached intent or
the cover logo. Opening restores those two components. Keyboard effect/timeout
and Turbo LED writes check the sensor synchronously and return EHOSTDOWN while
closed, or EAGAIN when the sensor is unavailable. Read-only lid_state/lid_status
expose status; there is no lid override. Normal profile/fan/logo controls remain
available. Profile writes reassert the blackout immediately and after two seconds
to handle firmware Turbo indication. Suspend prevents sensor access; wake checks
the lid before restoring internal RGB. Production raw endpoints remain absent.

Cover-logo writes use only target 0x83 and the verified zero zone mask. Accepted
settings are off, static brightness/RGB, native RGB-cycling breathing/neon, and
opmode. Brightness is 0..100, animation speed 1..5. Shifting and user-supplied
targets, directions and report fields are rejected by the normal logo parser.
The `logo` attribute also accepts `reapply`: under the state mutex it resubmits
the latest cached setting. It rejects requests without a cached setting and
refuses to interfere with an active developer session. It is not a preference
value and cannot restore a stale userspace-supplied color.

The kernel does not open preference files or implement disk parsing/save/restore.
Those operations belong to the separate aps-state service running as the normal
account, using private user-owned files under ~/.local/state/aps/. The kernel
validates every sysfs write independently of that service. No root-only preference
store, file I/O helpers, or shutdown persistence notifier remain in the module.

The `mux_mode` control accepts exactly optimus/nvidia/auto. It uses fixed WMBH
method 0x17 selector 2 for reads and 0x16 selector 2 for writes, with enum values
1/2/3. No arbitrary selector, LCD overdrive, reboot or PRIME operation is exposed.
The setter first requires a recognized getter, then rate-limits firmware attempts
to one per two seconds. `mux_request` records the last request and its result since
module load, including uncertain failures. No blind rollback is attempted.
Firmware mode selection requires restart; readback does not prove active routing.
No MUX request is persisted/replayed by APS, PM callbacks, or the preference helper.

`boot_effects` uses WMBH getter 0x17 selector 6 and setter 0x16 with only 0x06/off
or 0x106/on. It is 0640 with the same initial-namespace CAP_SYS_ADMIN gate as MUX,
is rate-limited, blocked during sleep, and never replayed by the preference helper.
All boot-logo image conversion, parsing and EFI file work stays in userspace.

The installer adds a modules-load entry and udev access rule for reboot. This is
not DKMS; another kernel needs a rebuild. Model and BIOS guards fail closed.
For automatic kernel-update builds, use the separate source installer
`scripts/install-aps-dkms.py` documented in `packaging/dkms/README.md`.
This fixed-kernel bundle must not be installed over an APS DKMS installation.

```bash
sudo ./uninstall.sh
```

Removal affects APS kernel/autoload/access files only. The separately installed
userspace binary and presets remain.
