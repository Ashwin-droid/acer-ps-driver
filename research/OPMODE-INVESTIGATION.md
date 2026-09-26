# What the cover-logo OPMODE command actually represents

Read-only investigation, 2026-09-26. No lighting commands, HID capability
selections, raw probes, profile changes, suspend, module reload, or installation
were performed. The user reports a brief flash only after Turbo/performance
changes, not immediately after the opmode command. They also report that the
sleep logo color persists while a custom awake setting does not.

## Conclusion

OPMODE is a real vendor operation, public mode 0x22 / HID wire mode 6, used by
PredatorSense for a short performance-mode indication. The evidence does not
support presenting it as a normal persistent cover-logo preset named Auto.

APS currently treats it as a persistent replacement for the awake logo. That
loses the previous awake setting and differs from AcerLightingService, which
explicitly skips saving OPMODE through its normal profile-save path. APS also
sends zero RGB instead of exposing the color argument the vendor path supplies.

This conclusively explains why APS cannot restore the pre-opmode saved color
after the user selects opmode. It does not, on its own, establish the cause of
a Static command losing its physical color without a later opmode selection.
The latter needs a separate event/timing observation. There is no physical
color/effect getter in the existing APS interface, so cached intent is not
proof of what the lighting controller is displaying.

## Current machine and state

- Predator PHN16S-71, BIOS V1.28, kernel 7.1.5-76070105-generic.
- Production driver, dev_mode=0; DKMS source package acer-phn16s/0.3.0.
- Cached awake logo: `opmode 0`.
- Saved awake logo: `logo=opmode 0`.
- Cached/saved sleep logo: `static 100 0000FF`.
- Preference generation observed stable at 18 during the inspected period.
- Lid closed; aps-state active with internal-lighting restoration deferred.

The source files for the packet encoding and kernel state path were compared
with the installed DKMS source snapshot. The persistence binary was compared
with the built release artifact. No settings were changed to gather this state.

Do not infer that OPMODE's zero brightness alone explains darkness: the vendor
OPMODE descriptor also uses zero brightness/minimum/maximum. This is not the
same parameter contract as Static brightness=0.

## Vendor evidence

Inputs are the supplied PredatorSense_5.1.514 package and its existing local
Ghidra exports, not a third-party Acer driver. Windows' mounted saved profile
is additional corroboration, not a live Windows observation.

- AcerLightingService.exe SHA256:
  `efc37ab8f18fcb8c98ebdb4c3b8ad710a4225beb85b6efd08daf2e8c93677636`.
- AcerECLightbarController.dll SHA256:
  `0fa5ef7875fbb32d8be285856877876f28b8b6c4a0673a013538be3e87af73a4`.

### 1. Intended visual behavior

The supplied UI's English strings include
`MUI_Introduction_Operating_Mode_Keyboard_Lightbar` and
`MUI_Introduction_Operating_Mode_Light`. Both describe a color indication for
a short period when changing modes. They do not describe continuous
performance-color illumination.

Source: `research/acer-app/dist/widgets/main/shared_assets/i18n/en.json`,
lines 652-653. Earlier physical testing in PROTOCOL.md also records that a Turbo
press briefly interrupts native Breathing and then Breathing resumes. Therefore
a performance-change flash is not evidence that selecting OPMODE is required
to enable indications.

### 2. Explicit performance-color input

The supplied AgentService OPERATING_MODE handler constructs a seven-entry color
table and sends `OPERATING_MODE` through FUN_140041c20. The HID-keyboard branch
uses the same table and requests brightness 100 at the IPC layer. The table is:

```text
index 0: FFFFFF
index 1: 00AEC7
index 2: 00AEC7
index 3: 00AEC7
index 4: 2E09C7
index 5: C700FF
index 6: 10DC00
```

These are vendor-code colors, not a visually verified mapping for every target
on this laptop. Source: `research/decompiled/AcerAgentService.exe.c`, around
lines 34805-34866 and 34986-35049. FUN_140041d50 maps the name OPERATING_MODE
to public mode 0x22. FUN_140041c20 sends that mode, a color, speed zero, the
requested brightness, and direction zero through the lighting-service IPC.

### 3. Cover-logo native mode and parameters

In AcerECLightbarController.dll:

- Factory CreateAcerHIDCoverLogoLEDControllerClassInstance, VA 0x18000b4e0,
  selects fixed target 0x83.
- FUN_180008b60 maps public mode 0x22 to HID mode 6.
- The logo-controller constructor creates an OPMODE record when capability
  bit 5 is set. Its value is 34, flags 64, exactly one color, color_mode 2,
  brightness/min/max zero, and speed/min/max zero.
- FUN_1800155a0 takes the first RGB color when color_mode is 2 and forwards
  it along with mode, speed, direction and brightness.
- FUN_180008f10, AcerHIDLogoLEDController::SetMode, uses the common A4 serializer
  with a zero zone mask.

The mounted saved Windows profile confirms these fields in
`AcerHIDCoverLogoLED Device_OPMODE`, including `color=0Xc7092e`. Its little-endian
RGB bytes are 2E 09 C7. The service clamps brightness to the mode's declared
0..0 range before applying it. An inferred equivalent packet for that saved
record is therefore:

```text
A4 83 06 00 00 00 2E 09 C7 00 00
```

That packet was reconstructed from the code and saved record, not captured or
sent during this investigation. The existing APS opmode 0 packet is:

```text
A4 83 06 00 00 00 00 00 00 00 00
```

The effect number is correct; the color contract and persistence semantics are
not equivalent. Exact controller behavior for an explicit zero RGB indication
has not been isolated from firmware-generated mode-change flashes.

### 4. Broadcast behavior and persistence exception

AcerLightingSvc_set_devicemode at VA 0x14001a300 handles mode updates. A global
device-0 OPMODE request additionally invokes OpenRGBSetOperatingMode at
0x140015980, which propagates the request to eligible AcerUSB/AcerHID controllers.

OpenRGBSetMode at 0x140014d40 updates the selected mode and queues a device
update, but calls normal profile saving through FUN_140009c10 only when the
mode's value is NOT 0x22. The actual machine-code check is at 0x14001549d:

```text
compare selected mode value with 0x22
if equal, skip call to FUN_140009c10
```

This branch was checked against the supplied EXE, not only the decompiler text.
The save path leads to profile records including ActiveMode in LightingProfile.ini.
The mounted Windows cover-logo profile retains ActiveMode=2, selecting its
BREATHING entry, even though its OPMODE color record is populated.

### 5. Timing is special, but no universal auto-restore is proved

LEDUpdateThreadFunction uses a 150,000,000 ns update spacing ordinarily, but
1,550,000,000 ns after the last applied mode was 0x22. Its clock helper at
0x1400016c0 scales the performance counter to nanoseconds, confirming the units.
Thus a subsequent lighting update is delayed for approximately 1.55 seconds
after an operating-mode indication.

This is a scheduling interval, not proof that the hardware flash always lasts
exactly 1.55 seconds. Nor does this branch alone prove an unconditional timer
that autonomously restores the previous mode. The supplied software preserves
the normal saved profile and makes room for a short indication; controller-side
restoration and later host requests must not be conflated.

## Where APS diverges

1. `src/protocol.rs::logo_report` maps OperatingMode to mode 6, accepts ordinary
   user brightness, but always sends RGB 000000.
2. `kernel/acer_phn16s.rs::write_control` calls set_logo, then replaces
   `state.logo` with that setting. There is only one awake-logo slot, with no
   previous/base setting retained for an OPMODE transition.
3. The normal sysfs write increments preferences_generation. `aps-state`
   captures the new `logo` value and saves it to the user-owned preference file.
4. Resume restores `state.logo`, so a cached opmode remains opmode after wake.
   `logo reapply` also reapplies that same latest slot, not an older custom color.
5. Performance changes go through set_profile_with_lid. That path handles the
   closed-lid blackout for internal LEDs but does not schedule a back-logo
   reapplication. Outside boot's bounded 2/10-second logo retries and system
   resume, it relies on firmware to preserve/resume the current cover effect.

The CLI's `auto` alias is misleading for this target. The Turbo-key LED is a
different device; its own Auto behavior should not be assumed to describe the
cover logo merely because both accept wire mode 6.

## Why the sleep logo survives

The kernel has separate `logo` and `sleep_logo` fields. Suspend applies the latter
through a direct typed HID request without replacing the awake cache or updating
preference generation. The user-owned file stores both keys independently.

Current state therefore requests blue during sleep and opmode while awake.
The working sleep color demonstrates that a native static setting can run on the
logo during sleep. It does not prove that an awake Static command survives every
performance event, or that the prior awake color still exists in APS's cache.

## Recommended correction, not implemented in this diagnostic

- Do not persist OPMODE as the normal logo setting or describe it as sustained
  Auto. Preserve the base choice as Off, Static, Breathing or Neon.
- If exposing an explicit indication action, implement its proper color
  contract and separate it from the persistent base setting. Validate any
  controller-level assumptions with an isolated visual test first.
- If a mode-change event loses a valid base setting, add bounded restoration of
  the latest choice, avoiding stale replay, repeated writes, and conflicts with
  suspend. Confirm that failure separately before implementing a workaround.
- Existing saved opmode entries have already replaced the older awake choice.
  Do not guess the user's intended recovery color from old chat history.

No fix or migration was installed. The report establishes an APS state-model
problem and a vendor-parameter mismatch. The user's Static-retention report is
still not attributable to a specific event without its exact command/timing.

## Follow-up: owner confirms separate Static-retention symptom

The owner explicitly confirms that plain Static can disappear without selecting
OPMODE afterward, while color set by the sleep profile stays. This is a reported
additional issue, not explained away by APS overwriting its base choice with
OPMODE. It has not yet been reproduced under controlled conditions here.

Source comparison confirms that both normal logo writes and suspend invoke
the same `hardware_hid::set_logo` and `protocol::logo_report`. Given the same
Static brightness/RGB, they submit the same A4 target/effect/payload. There is no
separate persistent-color opcode used for sleep.

The surrounding sequence differs. Suspend sets keyboard off, sets the configured
sleep logo, then sets Turbo off. The last operation queries/selects the Turbo
target's capabilities before writing its effect. Ordinary logo setting ends
with the cover-logo write. Suspend also blocks other APS hardware writes, then
the platform suspends devices and userspace. Thus possible explanations include
an awake-time overwrite, controller power-state behavior, or a sequencing issue.
None of these possibilities has yet been isolated. A2 metadata-selection effects
must not be asserted from this correlation alone.

The current boot log records sleep at 14:59:31, wake at 15:58:11, and an accepted
awake-lighting restore at 15:58:14 with error=0. That status confirms submissions,
not physical color. The caches/file still contain awake `opmode 0` and sleep
`static 100 0000FF`. The owner was asked whether persistence occurs only during
sleep or also after wake, since these imply different failing transitions.

No new experiment was performed and the normal saved colors were not changed.
The next useful test should observe physical color, the unchanged/changed cached
logo value, and the event that precedes disappearance, without repeatedly
reapplying a color and hiding the cause.

The owner subsequently clarified that the sleep-set color also stays after
waking. This rules out "it lasts only because the host is asleep" as a sufficient
explanation. Static retention is possible in at least this post-resume sequence.

On reinspection the current awake cache remains `opmode 0`, the sleep cache is
`static 100 0000FF`, and the successful wake-restoration log remains present.
The resume function restores the awake slot, not the sleep slot, and then applies
keyboard/Turbo settings. With the present closed lid, both internal targets are
set off and the last capability-selection target is the Turbo LED. The ordinary
logo setter ends with the cover-logo target selected. This provides a concrete
ordering/selection difference to isolate before attributing the issue solely to
the physical sleep state.

The historical wake log does not include the actual submitted logo setting, so
it cannot establish which value was in the awake cache at the exact moment of
that wake. Do not claim a captured sequence of blue followed by opmode from that
log alone. There was still no controlled reproduction or new hardware write.

## Eco-specific correction and implementation, 2026-09-26

The owner narrowed the flash-then-off symptom to transitions into Eco. Other
performance modes retain the selected logo effect. This is physical feedback,
not a conclusion from the HID submission status. The sleep/resume correlation
above remains an observation, not proof of a different persistent-color opcode.

The requested implementation removes OPMODE and its CLI Auto alias as persistent
cover-logo settings. The shared kernel parser rejects them and the normal logo
packet builder can no longer construct wire effect 6. Turbo LED Auto is separate
and unchanged. Old user preference files migrate only an exact bounded
`logo=opmode BRIGHTNESS` record to Off, retaining all other validated settings and
duplicate-key rejection. They cannot recover an earlier base color that was
never saved. On this machine the live record already contained awake orange
`static 100 FF8C00` and sleep blue `static 100 0000FF` before deployment, so neither
needed migration or replacement.

The kernel queues a cover-logo reapplication two seconds after an APS profile
write ends in Eco, including a failed setter whose final readback shows Eco.
The shared setter covers both sysfs commands and the physical Turbo button.
The worker checks the current profile again and uses the latest cached awake
logo under the state mutex. Success ends the recovery; transport errors permit
two more attempts after two and five seconds. Recovery leaves preference
generation unchanged, cancels on another profile or suspend, yields to wake
restoration, and does not interfere with a signed developer session.

This does not synthesize a Windows OPMODE palette or suppress the firmware's
transient mode indication. The two-second wait is a recovery policy, not a
measurement of the exact flash duration. No change to Eco's power/GPU settings
was made. Host tests exercise the delay, latest-setting selection, Off and
animations, cancellation, retry bounds, deadline overflow, and legacy migration.

### Installation and physical verification

Production DKMS `acer-phn16s/0.3.1` was built for
`7.1.5-76070105-generic`, signed with the enrolled Ashwin Module Signing Key,
installed and loaded without a reboot. Loaded and installed srcversion both
read `FBE88282349FAC12655ABFD`; `dev_mode` is 0. Installed APS and aps-state
hashes match their release builds. Version 0.3.0 remains built for rollback;
previous userspace binaries are in `/var/lib/aps/eco-update.C7uYcH`.

Validation passed: 114 host Rust tests, Clippy with warnings denied, 12 DKMS
policy tests, and two explicitly run live sysfs tests. The live tests confirm
invalid requests including opmode/auto are rejected without changing state,
and closed-lid keyboard/Turbo writes still fail with EHOSTDOWN.

A normal-user `aps profile eco` selected Eco with firmware readback. At 17:05:02
local time the kernel logged `Eco logo recovery attempt=1 error=0`. Preference
generation rose only for the profile write, from 6 to 7. Awake orange and sleep
blue remained separate in the sysfs caches and user-owned preference file.
The owner visually confirmed: "Yes, orange stays on" after the Eco indication.
The performance mode was then returned to the pre-test Quiet selection. No
suspend, reboot, raw probe, MUX change, or GPU configuration change was performed.
