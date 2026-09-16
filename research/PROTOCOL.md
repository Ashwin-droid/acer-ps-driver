# Local protocol evidence

## Boot master versus sound mute correction, 2026-09-16

The earlier conclusion that firmware has only a combined switch was too strong.
V1.26 firmware form definitions contain separate master and mute fields. The
Windows BOOT_SOUND command changes the master and does not write the mute field.
See `BOOT-CONTROL-SEPARATION.md` for the IFR records, native SMM trace and the
read-only V1.28 observations. No independent mute setter is implemented yet.

## Current CLI color contract

Following the Windows capture and the user's visual tests, only Static and the
userspace Strobe effect expose color arguments. Both retain one-color and
four-zone forms. Breathing, Neon, Wave, Shifting, Zoom, Meteor and Twinkling use
firmware colors; Rainbow generates its own. Nonzero RGB values in historical
Windows animation packets were not proof of an exposed or effective color picker.
New native animation requests preserve the captured effect/speed/parameter
layout but use zero RGB placeholders. Old non-configurable preset colors are
discarded on load; Static and Strobe colors are preserved.

Live CLI validation completed all native effects, both wave directions, static
global/four-zone colors, Strobe and Rainbow, and the supported cover-logo modes.
The user reported that everything they watched looked right, then clarified
that only Static took a chosen color among native effects. A subsequent smoke
test verified the colorless argument forms, static one/four-zone forms,
four-color Strobe, and Rainbow completion. Original settings were restored.

## Turbo LED restore ordering

The userspace restore order now ends with Turbo LED color and mode. It waits
two seconds before those fields after a successful performance-mode restore;
if the profile write fails, both LED fields remain pending for its retry.
Other independent controls can still restore. Unit tests cover ordering, the
wait, failure deferral and retry behavior.

A live syscall trace of the installed helper confirmed this sequence:

```text
14:16:24.661701 profile <- eco
14:16:25.105444 logo <- static 100 FF8C00
14:16:25.167241 button_mode <- power modes
14:16:25.223749 sleep 2 seconds
14:16:27.224255 turbo_led <- B3001B
14:16:27.317710 turbo_led_mode <- manual
```

This update changed only userspace restore behavior; the loaded driver remains
production mode (`dev_mode=0`). The trace proves ordering and delay, not a
physical color getter. The separate late-logo reapplications do not rewrite
the performance mode or Turbo LED.

## Logo boot-recovery follow-up

On the next Linux boot, both the user-owned preference file and kernel cache
said `static 100 FF8C00`, but the user reported an unlit/unrestored logo. The
controller also logged a reset-ack timeout during boot. Reapplying the exact
saved setting through normal `aps logo` made it orange, confirmed by the user.
This establishes an accepted early request that was ignored/lost, not corrupt
preferences or missing permissions; the exact firmware/reset timing cause is
not yet proven.

The userspace helper now makes two late logo submissions, after 2 and 10 seconds
following initial restore or driver reload. A stateful `logo=reapply` command
reads the latest cached choice under the kernel mutex, avoiding stale snapshot
overwrites. It refuses active developer sessions; the helper cancels scheduled
attempts when a probe session is observed. Both late submissions ran after the
updated service started. A full reboot of this mitigation remains untested.

The six-Sol native-path review is recorded in
[parallel-native/SUMMARY.md](parallel-native/SUMMARY.md). Its coordinator follow-up
explicitly selected keyboard target 0x21, verified A3, then sent the saved
direction-1 Breathing recipe. The user still observed RGB cycling. This closes
the earlier raw-test selection-state uncertainty without proving that every
possible firmware path lacks fixed-color support.

Sources used for hardware commands:

- Supplied `Predator Sense_Acer_5.1.514_W11x64_A.zip`.
- Ghidra output in `decompiled/` for AcerHardwareService.exe,
  AcerAgentService.exe, AcerECKeyboardController.dll.
- The supplied Electron application extracted from `app.asar` into `acer-app/`.
- This machine's DSDT, SSDT19, and live HID descriptors under `firmware/`.

ASense was downloaded as a reference before the user prohibited GitHub driver
sources. It was set aside. No ASense code or prebuilt driver is included in this
implementation or installed. General Rust kernel API signatures were checked
against the Linux stable source mirror, not a third-party Acer driver.

## Boot recovery observation, 2026-09-12

After an actual reboot, the existing profile getter returned EIO and the old
userspace helper restarted every three seconds. All seven writable sysfs files
could be opened read/write by UID 1000, so this was not a permission failure.
The CLI incorrectly appended access-permission advice to every write error.

A signed patched module logged a well-formed profile response with raw value 2.
The user identifies Balanced as the firmware's reset/default mode. Selecting the
saved Eco value 6 succeeded and read back 6. No undocumented mode was written:
the allowed setters remain 0, 1, 4, 5, 6. An unrecognized prior value is never
used for rollback. Failed transport/status/shape validation still aborts writes.

The new helper completed restore from the unchanged user-owned preference file.
Driver unload/reload restoration also passed while the helper stayed running,
with zero service restarts. A subsequent full reboot has not yet been tested.
Unit tests cover individual restore failures, retry-only-failed behavior, LED
color/mode ordering, and refusing invalid records before writes. Shutdown tests
check that incomplete restoration cannot overwrite the saved preference file.

## System Boot Effect, 2026-09-16

PredatorSense UI `MUI_Post_Animation` labels BOOT_SOUND as "System boot animation
& sound". Its CUSTOM_BOOT_LOGO control offers JPEG/GIF and a default reset.
Agent `FUN_14003c340` reads GetGamingMiscSetting selector 6, accepting only the
next byte 0/1 after a successful status. `FUN_14003d830` sets 0x06 or 0x106 through
SetGamingMiscSetting. Same fixed WMBH methods 0x17/get and 0x16/set as MUX, but
selector 6 only. No separately identified sound-only/animation-only selector.

Custom-logo path: Agent `FUN_14003dda0` -> hardware-service apply_bios_logo ->
`FUN_140010ae0` copy_bios_logo. Hardware `FUN_140013670` GetLogoVolume queries
`Win32_Volume Where Label='ESP'`. It copies the source as EFI/OEM/AcerLogo.gif or
AcerLogo.jpg and removes the other variant. Reset `FUN_1400110a0` removes both.
No BIOS flashing or EFI-variable setter was identified in this path. The UI sets
image limits to floor(0.4 * display width/height), supports JPEG/GIF below 35 MiB,
color depth below 32 bits. This model's eDP mode is 2560x1600, hence 1024x640.

Read-only live inspection found original ESP label=ESP, PARTUUID
c1a78bb8-4f60-4b7b-8f47-2a1ba261c29d, with EFI/OEM/Boot and Recovery directories
but neither custom AcerLogo file. This is not the Linux /boot/efi ESP. Root
userspace helper writes only the two vendor files and transaction temporary
copies on that verified, unmounted partition. Existing files are backed up;
the actual original ESP has not been modified during implementation tests.

## Firmware GPU MUX, 2026-09-14

Traced supplied PredatorSense UI -> AcerAgentService -> AcerHardwareService:

- UI GPU_MODE: Optimus=0, NVIDIA GPU Only=1, Automatic Select=2.
- Agent `FUN_14003d800`: SetGamingMiscSetting `((mode + 1) << 8) | 2`.
- Agent `FUN_14003c2f0`: GetGamingMiscSetting selector 2; accepts the next
  result byte only when 1..3, then subtracts one for the UI enum.
- The existing service WMI methods are set 0x16 with 8-byte input/4-byte reply,
  get 0x17 with 4-byte input/8-byte reply, through the same model-specific WMBH
  path already used by the separately selected performance control.
- Normal Rust controls expose only optimus/nvidia/auto, reject raw inputs, and
  never touch selector 1 for LCD overdrive. They do not call PRIME or reboot.
- MUX writes require root: kernel checks initial-namespace CAP_SYS_ADMIN even
  through root-opened fds; attribute creation and installed udev permissions are
  0640. The CLI checks effective uid and requests Polkit authorization for only
  the fixed root-owned installed APS binary plus a canonical mode; sudo skips it.
  Lighting and MUX status remain available to the normal account.
- Firmware selection readback is not proof of active display routing. Setter
  acceptance and actual post-restart routing must be verified separately. No
  automatic rollback on uncertain errors; no preference or resume replay.

## Fans

AcerAgentService.exe `FUN_1400408f0` builds custom fan behavior and percentage
payloads; `FUN_14001b780` forwards behavior to AcerHardwareService, which calls
SetGamingFanBehavior. The non-custom FAN_CONTROL path selects mode+1.

- CPU fan bitmap: bit 0; GPU fan bitmap: bit 3.
- Behavior modes: Auto=1, Maximum=2, Manual=3.
- Set behavior: `9 | mode<<16 | mode<<22`, method 0x0e, eight-byte input.
- Get behavior: selector 9, method 0x0f, four-byte input. Status is byte 0;
  CPU mode is bits 8–9 and GPU mode bits 14–15.
- Set percentage: selector CPU=1, GPU=4; percentage in bits 8–15; method 0x10.
- Get percentage: same selectors, method 0x11; low byte is status, value follows.
- RPM: method 5, four-byte selector CPU=0x0201, GPU=0x0601.
  SSDT19 `GGSI` explicitly reads CPUF/GPUF; WMBH packs the returned RPM in bytes 1–2.

Live: Auto reads 0x41 after removing the status byte; Maximum 0x82; Manual 0xc3.
Maximum and manual 55% setters/readbacks succeeded. Maximum ramped approximately
1640/1651 RPM to 3199/2978 RPM over six seconds. Both sessions returned to Auto.

## Performance

AcerHardwareService.exe `FUN_14000e570` builds `mode<<8 | 0x0b` and calls
SetGamingMiscSetting. Get-operation-mode uses only selector 0x0b. SSDT19 WMBH
dispatches these as methods 0x16 and 0x17. The app enum names setter values
Quiet=0, Default=1, Extreme=4, Turbo=5, Eco=6.

SSDT19 also translates setters to internal ODV0 values, but this is not the WMI
getter's encoding. A first hypothesis using that mapping failed live readback and
was removed. Logged setter 0x10b returned 0, and its getter returned 0x100. Correct
WMI decoding is the app-facing mode byte after the status byte. The initial `2`
was an unknown existing state; its origin has not been established.

The vendor wrappers sleep 30 ms after WMI calls. The Rust setters use that delay
before verification. Profile failure attempts rollback to a recognized prior mode.
Quiet/Balanced transitions and readbacks have passed on this machine.

## Keyboard effects

Live device: I2C HID `0CF2:5130`, ENEK5130. Descriptor usage page FF5A.

- A1: five-byte feature report, target count and up to three target IDs.
- A2: two-byte feature report selecting a target.
- A3: nine-byte feature report: ID, target, control flag, kind, zone count,
  four-byte effect bitmap.
- A4: eleven-byte feature report: ID, target, effect, brightness, speed,
  direction, red, green, blue, zone mask LE16.

`CreateAcerHIDKeyboardControllerClassInstance` at 1800090f0 fixes keyboard target
0x21. `FUN_180013340` selects it with A2, waits 20 ms, then reads A3.
`FUN_180013d00` constructs A4. `FUN_180008200` maps the controller effects.
RGBController_AcerHIDKeyboard at 18000d2a0 tests capability bit `effect-1`.

Known exposed native IDs: off 1, static 2, breathing 4, neon 5, wave 7,
shifting 8, zoom 9, meteor 10, twinkling 11. Effect 6 is OPMODE, not rainbow,
and is deliberately excluded. Native directions follow the app's RIGHT=1/LEFT=2.

Breathing color test, 2026-09-12: the user observed RGB cycling despite explicit
colors with direction 0. The supplied DLL's default-direction conversion maps
0 to 2 in FUN_1800082b0. A temporary signed module allowed a bounded Breathing
parameter 2, and `breathing 60 3 fixed 15 0088FF` was submitted successfully.
The user visually confirmed it STILL cycled RGB. This disproves the proposed
0=cycling / 2=fixed selector for this controller. The experimental value was
removed, and the pre-test four-zone static lighting was restored. Do not claim
that a successful HID transfer proves color support. No-color native cycling
remains available; explicit-color CLI requests now explain the limitation.

Follow-up signed raw HID tests: effect 3 with blue left the keyboard steady/no
change according to the user. Writing Static blue first and then effect 4 with
the same blue still cycled RGB. Saved lighting was restored after both tests.
All follow-up calls used the compile-time dev interface without root or reload.

The mounted Windows drive subsequently supplied the actual registered keyboard
DLL version 1.0.3.10 (ZIP version 1.0.3.9) and its saved Breathing recipe.
That exact recipe, including direction 1 and RGB 00AEC7, also visually cycled
RGB. See `windows-installed/NOTES.md` for file provenance and decoded settings.

Live A3 reports four keyboard zones and mask 0x000007fb. Static global red,
green, blue and four separate zones were sent successfully. Visible confirmation
is separate from transfer success. Strobe/rainbow are Rust-generated sequences,
not guessed firmware effect IDs.

## Cover-logo capability check, 2026-09-12

The supplied Acer lightbar/logo DLL explicitly binds
`CreateAcerHIDCoverLogoLEDControllerClassInstance` at 0x18000b4e0 to target 0x83
(`AcerECLightbarController.dll.c:8641-8661`). The mounted Windows profile also
registers `AcerHIDCoverLogoLED Device` separately from the keyboard and mode key.

Signed capability-only queries on the live device returned:

```text
A1 target list: a1 03 65 21 83
A3 target 83:   a3 83 01 05 01 3b 00 00 00
```

This advertises one zone and effects bitmap 0x3b: off 1, static RGB 2,
Breathing 4, Neon 5, and OPMODE 6 under the vendor mapping. These are advertised
capabilities, not visual verification that every effect behaves as expected.
No A4 logo-setting request was sent. The previous metadata selection 0x65 was
restored with A2 and the developer session was closed. Normal APS does not yet
expose a dedicated cover-logo control.

Subsequent signed target-0x83 tests used the logo-specific zero zone mask from
`AcerHIDLogoLEDController::SetMode` (DLL function 0x180008f10), not the keyboard
zone mask. User-confirmed results:

- `a4 83 02 3c 00 00 ff 00 ff 00 00`: back logo became steady magenta.
- `a4 83 04 3c 03 01 ff 00 ff 00 00`: native Breathing ran, but cycled RGB
  instead of retaining the requested magenta.
- `a4 83 05 3c 03 01 00 00 00 00 00`: Neon was submitted, but the user moved
  to the next test without initially confirming its appearance. On a repeated
  test, the user confirmed it smoothly cycles colors without fading dark.
- `a4 83 08 3c 03 01 ff 00 ff 00 00`: unadvertised Shifting made the logo
  turn off or stop animating according to the user. Do not expose it as supported.
- `a4 83 06 3c 00 00 00 00 00 00 00`: the user confirmed the operating-mode
  effect settles to a steady color. The color was not specified, and performance
  mode was not changed, so automatic profile-color tracking remains unverified.

Further user observations clarify operating-mode behavior: effect 6 normally
leaves the cover logo off, then pressing Turbo briefly lights/flashes a color.
It is not a continuously displayed performance-mode color. The color mapping
has not been established. With native Breathing (effect 4) active, a Turbo press
briefly flashes the logo and then Breathing resumes. This supports a temporary
firmware indication interrupting/resuming the configured animation, rather than
the power-mode event permanently replacing it. No userspace animation was used.

After the Shifting test, the verified static-magenta setting was resubmitted,
metadata selection was returned to 0x65, and the developer session was closed.
No normal logo CLI/kernel implementation has been added during these probes.

After the probe phase, normal `logo` and `logo_capabilities` sysfs attributes
and the `aps logo` CLI were implemented. The normal path owns target 0x83 and
zero zone mask; it accepts only off, static RGB, RGB-cycling Breathing/Neon,
and operating-mode indication. Brightness is 0..100 and animation speed 1..5.
Fixed colors are deliberately not accepted for the cycling animations.

Installed verification passed malformed direct sysfs rejection and normal-user
disk preference restoration. The user-selected `static 100 FF8C00` setting was
present in the user-owned preference file, temporarily switched off under a
capture freeze, and restored through aps-state without changing other controls.
Both build modes compiled; the production artifact excludes developer probe
code/key, and the installed dev module remains signed by Ashwin Module Signing Key.
A full reboot with the newly added logo field has not been tested.

## 30-second timeout

Live EC HID: `1025:174B`, descriptor includes report A0 with 64 payload bytes.
AcerHardwareService `FUN_140020c30` reads EcHID GetDeviceStatus device 2;
`FUN_140021650` sets it. AcerAgentService Set_Backlight_30s preserves brightness
and chooses timeout 30 when enabled, 0 when disabled.

- Read packet payload first 8 bytes LE: 0x00000202000aa000.
- Set packet payload first 8 bytes LE: 0x00010201000aa000.
- Set brightness is payload bytes 8–9; timeout is bytes 10–11, both LE16.
- Report ID A0 precedes the 64-byte payload.
- Validate returned status E000 and command 000A. Reads also require device index 2.
  This laptop's write acknowledgements fill the data area with FF; writes are
  verified by a separate strict getter, preserving and checking brightness.
- Wait 20 ms between feature SET request and feature GET response.
- No data-version probe or any other device-index request is sent.

## APS sysfs revision

### Verified physical button paths and key LED

The initial ACPI-event approach was not the active route on this model and was
removed. Live button capture showed HID report 04 85 ff. AcerHardwareService.exe
WindowsInput at 140018be0 identifies the corresponding 0xff8500 value as
"Operation Mode Switch". The exact report is now filtered by the model-specific
APS HID driver, preserving normal HID parsing and hid-generic's input-per-app and
reset/resume behavior. In hotkey mode real F14 press/release events were captured.
Power-mode cycling was physically verified, Eco -> Quiet, then Eco was restored.

The physical PredatorSense key instead arrives as Linux key 425 on "AT Translated
Set 2 keyboard", physical path isa0060/serio0/input0. A narrowly matched input
filter consumes only that key's press/release/repeat and queues fixed F15. Real
F15 press/release events were captured. Other typing passes through. Neither
a user-selected keycode nor a writable fake-press endpoint exists.

AcerECLightbarController.dll exports at 18000b5a0/18000b640 identify independent
TurboKeyLED 0x64 and ModeKeyLED 0x65 targets. This machine advertises 0x65, one LED
zone, native effect mask 0x3b. Effect 6 is native operating-mode-following Auto,
2 is static RGB, 1 is off. Static RGB and Auto were exercised without changing
performance or keyboard settings. The user's later chosen manual color is
preserved by the kernel journal, rather than overwritten by the tests.

### Current persistence: normal-user helper

The kernel journal implementation has been removed, including filesystem APIs,
record decoding, save/status endpoints, and the reboot notifier. The old
/var/lib/aps files were removed after migration.

The separate aps-state service runs as ashwinmhaskar, UID/GID 1000, with no
effective Linux capabilities and NoNewPrivileges enabled. Preferences are private
user-owned files under ~/.local/state/aps/. The service reads/writes only typed
sysfs controls and its own state directory. APS CLI remains an unprivileged
wrapper; kernel validation and the fan watchdog remain independent.

Verified: live settings migrated without change; automatic saves on button-mode
changes; complete restore after driver unload/reload while the same helper stayed
running; user ownership and 0700/0600 modes; helper rejection of corrupt,
oversized, permissive, symlink and FIFO files. The kernel module imports no
filp_open/kernel_read/kernel_write/vfs_fsync/vfs_truncate/dentry_open or reboot
persistence symbols. No actual machine reboot was performed.

Lighting CLI positional syntax accepts the screenshot examples. Bare lighting
apply and help routes were verified to print usage without changing lighting.

The 0.2.0 kernel owns both ACPI and HID access. The separately compiled APS binary
contains only sysfs operations and has no hardware FFI. The ioctl endpoint was
removed; its misc device serves only as the sysfs anchor. Attributes are bounded
to 128-byte requests, parsed with the shared allocation-free control grammar and
independently validated by the kernel. The hardware targets and reports are not
selectable by userspace.

Verified as UID 1000: installed /usr/local/sbin/aps; profile, fan, keyboard-effect
and timeout writes; native and software lighting effects. Read-only attributes
are root:user-group 0440; writable controls are root:user-group 0660. Raw HID
devices remain root-only. The installed kernel is signed by Ashwin Module Signing
Key. No shared request key, socket, daemon or polkit runtime is involved.

Twelve userspace/unit tests pass. Two live kernel contract tests pass: malformed
direct sysfs writes return EINVAL without changing control state, and a five-second
override lease restores Auto without a running CLI. Kernel logs show lease expiry
and no new BUG/Oops/general-protection-fault/panic or APS restore failures during
the sysfs verification run. Reboot autoload and udev access restoration are
configured; a reboot was not performed.

## Source hashes (original inputs)

| File | SHA-256 |
| --- | --- |
| AcerECKeyboardController.dll | 4ed2737706c550b12d381c455a4896b96f123b4855a64e652ded892894cdfa63 |
| AcerHardwareService.exe | 0b040eca539311c5d440e108aa0da560effe8a11763a48687f6b837c917a8212 |
| AcerAgentService.exe | 93044b1dd2295fcfc56c18af2b8753ceccb537059924af515301ee08f3d99f1c |
| DSDT | 3d9fdda833f4638f2748aa4db5c7d67e6d4f2727fafafd0160034bc89389e529 |
| SSDT19 | 6c86c3389f020b817b20a20a28449a2039ea4e89470b36fded5e2ee4b034239b |
| ENEK5130 descriptor | aea16043e2c5ad2f1e5164a3462db5475ce9bb9f5cc600af04b9333574b5c50b |
