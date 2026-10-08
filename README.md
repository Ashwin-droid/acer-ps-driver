# APS — Acer PredatorSense for this laptop

APS provides fan, performance, keyboard lighting and idle-timeout controls for
Acer Predator PHN16S-71, FTYPE_ARX, BIOS V1.28.

Signed raw firmware probes require the compile-time `APS_DEV=1` option. Normal
builds exclude the interface entirely. Read `dev_mode` to check the loaded build.
See [DEVELOPMENT.md](DEVELOPMENT.md) for the separate signing key, raw transports,
replay protection and risks. Normal controls remain bounded and unsigned.

The components are separate:

- Kernel: signed Rust module acer_phn16s.ko, built for Linux
  7.1.5-76070105-generic. It owns hardware access and validates all requests.
- Userspace: unprivileged Rust executable aps, installed at
  /usr/local/sbin/aps. It only reads/writes the typed sysfs interface.

The separate aps-state userspace service saves/restores preferences as your normal
account. There is no runtime polkit action, setuid helper, shared request key or
control socket. The module remains Secure Boot signed with Ashwin Module Signing Key.

During restoration, the performance mode is applied first. The Turbo LED is
restored last, with a two-second settling wait before its saved color and mode
are applied, so the firmware's performance-color update cannot immediately
replace the manual color. Failed performance restores keep the Turbo LED fields
pending for the successful retry; unrelated lighting can still restore.

## Use APS without sudo

    aps status
    aps profile quiet
    aps profile balanced
    aps profile performance
    aps profile turbo
    aps profile eco

    aps fans auto
    aps fans maximum
    aps fans manual 55 65

    aps keyboard info
    aps keyboard static FF0000,00FF00,0000FF,BB00FF 80
    aps keyboard timeout get
    aps keyboard timeout on
    aps keyboard timeout off

    aps lighting list
    aps lighting show strobe
    aps lighting run four-zones
    aps lighting run slow-breathe
    aps lighting run rainbow
    aps lighting run strobe

    aps lighting apply breathing
    aps lighting apply static ffffff
    aps lighting apply shifting 100
    aps lighting apply breathing 80 2
    aps lighting apply --help

Bare lighting apply displays help and makes no lighting change. The positional
syntax is EFFECT [COLOR] [BRIGHTNESS] [SPEED]; named options still work. A number
without a preceding color is brightness. Each command group accepts --help.

Only Static and the userspace Strobe effect accept COLOR. They support one
color for all zones or exactly four comma-separated zone colors. All native
animations use the firmware palette; Rainbow generates its own colors.
Both command routes use one parser and the captured native parameter value:

    aps keyboard breathing 80 2
    aps keyboard neon 100 5
    aps keyboard wave 100 9 --direction left
    aps keyboard shifting 100 9
    aps keyboard zoom 60 3
    aps keyboard meteor 70 5
    aps keyboard twinkling 60 2
    aps keyboard static FF0000,00FF00,0000FF,FFFFFF 60
    aps lighting apply strobe FF8000 30 --speed 1 --duration 5

`aps keyboard EFFECT ...` is an alias for `aps lighting apply EFFECT ...`.
Old saved recipes for effects other than Static/Strobe discard their irrelevant
color field when loaded, without rewriting the user's preset files. Static and
Strobe retain their saved colors. Rainbow holds its last generated frame at exit.

## Turbo button and its RGB LED

    aps button status
    aps button mode "power modes"
    aps button mode hotkey

Power modes cycles Quiet → Balanced → Performance → Turbo → Eco → Quiet. On
battery it skips Performance and Turbo. Hotkey mode emits a fixed F14 press and
release from the virtual APS Turbo Button input device. No configurable keycode,
input injection endpoint or userspace listener is needed for normal operation.
The kernel filters the exact Turbo HID report and debounces it. The PredatorSense
key is separately filtered as key 425 on this laptop's AT keyboard and emits F15.
Other keys pass through unchanged. Button mode is saved/restored by the user-account service;
without valid saved state it defaults to power modes. Press counters reset on reload.

The physical button LED is a separate ModeKeyLED target, 0x65, on this laptop.
Its advertised mask 0x3b supports static RGB and the native operating-mode effect.

    aps turbo-led status
    aps turbo-led auto
    aps turbo-led manual FF0088
    aps turbo-led 00FF00
    aps turbo-led manual
    aps turbo-led off

Auto uses the controller's operating-mode effect so it follows performance mode
without a userspace process. Hex writes select Manual. Manual with no color
restores the last manual setting, default white without valid saved state. Only
six hex digits with optional #, or off, are accepted for colors. Valid saved LED
settings are restored by the user-account service; otherwise the cached setting begins as unknown.

## User-account preferences

The aps-state service runs as ashwinmhaskar (UID/GID 1000), not root. It stores
active settings in $XDG_STATE_HOME/aps/preferences, defaulting to
~/.local/state/aps/preferences. The directory is user-owned 0700 and files are
user-owned 0600. The old root-only /var/lib/aps store has been removed.

    aps preferences status

The CLI stays an unprivileged sysfs wrapper. The separate helper is installed at
/usr/local/libexec/aps-state and managed by aps-state.service, which is enabled
for boot and executes under the normal account. It has no Linux capabilities,
NoNewPrivileges enabled, and a filesystem sandbox allowing writes only to the
user's APS state directory and the APS sysfs controls.

The helper captures performance mode, button mode, all keyboard zones, timeout,
Turbo LED auto/manual mode, and the last manual color. It debounces changes,
writes atomically, fsyncs the new file and its directory, and saves on service
stop/shutdown. It restores saved settings at startup and after a driver reload.
Fan overrides and running software animations are not restarted from disk.

Only a fixed set of preference fields and bounded values is accepted. Files must
be private and owned by the service user. Symlinks, devices, FIFOs, hard links,
oversized/corrupt documents and unknown fields are rejected. Bad preferences are
logged to the service journal and discarded in favor of live state; they are
never parsed by the kernel.

The kernel independently validates every sysfs write regardless of which
userspace program sends it. Its file I/O, disk record decoder, save endpoint and
shutdown persistence hook have all been removed. preferences_generation remains
a read-only change counter used by the helper.

Named lighting preset recipes under ~/.config/aps/profiles remain separate from
the active-settings store.


Manual fan values are CPU/GPU percentages in 40–100. Fan commands apply once
and exit. There is no duration argument, lease, renewal loop, or automatic reset
on process exit, suspend, or module unload. Use `aps fans auto` to request Auto.
Firmware can still change fan settings on power/performance transitions; APS
does not fight those changes or restore fan overrides at boot. A failed fan
configuration still attempts the existing immediate Auto rollback and reports
the error; this is not an expiry timer.

The native performance values are Quiet=0, Balanced=1, Performance=4, Turbo=5,
Eco=6. The CLI requires AC power for Performance/Turbo. Firmware mode changes are
verified by the kernel. APS does not add Windows-only GPU clock offsets.

## Back-of-lid logo

The cover-logo target is independently controlled through normal typed sysfs,
without a developer key, sudo or polkit for each change:

    aps logo status
    aps logo FF00FF 60
    aps logo on 60
    aps logo off
    aps logo breathing 60 3
    aps logo neon 60 3

`on` selects static white. Static colors accept six hex digits; brightness is
0..100 and defaults to 60. Breathing and Neon use native speeds 1..5, default 3.
Breathing fades while cycling RGB; Neon cycles colors without fading dark.
They do not accept a fixed color. Performance changes may temporarily overlay a
firmware mode indication on any normal logo effect. After selecting Eco, the
kernel waits two seconds and reapplies the latest awake logo setting, including
Off. Failed recovery retries after two and five seconds, at most three attempts.
Changing away from Eco or preparing for sleep cancels this recovery; sleep and
wake retain their own lighting policy. Recovery does not modify saved preferences.

`opmode` and its `auto` alias are no longer persistent logo choices. Old saved
`logo=opmode BRIGHTNESS` records migrate to Off without discarding the other
preferences. There is no earlier color in those records to recover automatically.
Turbo LED Auto remains available and is unrelated to the removed logo choice.

The account's preference service saves/restores the logo setting. `status` shows
the last accepted setting, not the current physical color or a temporary Turbo
flash. Shifting is deliberately excluded. The kernel owns target 0x83, report
length and zero zone mask; callers cannot choose another component.

The service makes two late logo reapplications, 2 and 10 seconds after initial
restore or driver reload, to tolerate a boot-time firmware reset/overlay after
an accepted write. It asks the kernel to reapply the latest cached choice under
the driver mutex, never sends an old snapshot back over a newer user selection,
and cancels these attempts if a signed-probe session is observed. This is bounded
startup recovery, not a continuous loop. The exact cause of the observed lost
boot write is unproven; a later full reboot is still needed to validate recovery.

## Saved lighting presets and effect settings

Presets are plain data in $XDG_CONFIG_HOME/aps/profiles, falling back to
~/.config/aps/profiles. Saving or inspecting them never calls the kernel.

    aps lighting effects
    aps lighting save my-strobe --effect strobe --colors FFFFFF \
      --brightness 60 --speed 2 --duty 0.5 --duration 15
    aps lighting run my-strobe

    aps lighting apply --effect wave --brightness 70 --speed 3 --direction left
    aps lighting save slow-rainbow --effect rainbow --brightness 65 \
      --speed 10 --direction right --duration 60

| Effect | Settings | Execution |
| --- | --- | --- |
| Static | One or four colors, brightness | Native keyboard effect |
| Off | No color or speed | Native keyboard effect |
| Breathing, neon | Firmware RGB palette; brightness, speed 0–9; no color argument | Native keyboard effect |
| Shifting, zoom, meteor, twinkling | Firmware palette, brightness, speed 0–9; no color argument | Native keyboard effect |
| Wave | Brightness, speed 0–9, left/right direction | Native keyboard effect |
| Strobe | Colors, brightness, 0.2–5 flashes/sec, on-time fraction 0.1–0.9, duration | Rust userspace timer |
| Rainbow | Brightness, 1–60 seconds/cycle, direction, duration | Rust userspace timer |

Some native multicolor effects choose their own palette. Native effects continue
after APS exits. Strobe/rainbow need their userspace process and are subject to
HID/system scheduling latency. A dead process stops issuing frames; it cannot
install a timer or arbitrary code inside the kernel.

The keyboard timeout is a real firmware setting: 30 seconds when on, 0 when off.
The kernel preserves brightness and verifies both values. Software-generated
lighting traffic may interact with idle-timeout behavior.

## System boot effects and custom logo

```bash
aps boot status
aps boot effects on
aps boot effects off
aps boot logo pick ~/Pictures
aps boot logo set ~/Pictures/logo.png
aps boot logo prepare ~/Pictures/logo.png ./prepared.jpg
aps boot logo prepare ~/Videos/clip.mp4 ./prepared.gif
aps boot logo set ~/Videos/clip.webm
aps boot logo status
aps boot logo reset
```

This exposes the master switch from PredatorSense's System Boot Effect panel.
Firmware also defines a separate Sound mute setting, disabled in the BIOS UI
when the master switch is off. The existing PS command does not change that
mute field; APS does not yet expose a verified independent sound setter.
The master toggle uses the
root-only `boot_effects` sysfs control; ordinary status is unprivileged. Changes
take effect at the next boot; APS never reboots automatically or saves this flag
in the ordinary user preferences.

The Ratatui/ratatui-image picker previews the highlighted file before opening it,
including mouse hover and keyboard selection. The preview sits beside the file
list, or below it in narrow terminals. Decoding is debounced and runs in a single
background worker, with stale results discarded. GIF/video browsing shows the first frame.
Enter opens the separate crop editor.
Arrows move the crop, +/- zoom, 0 restores the automatic center crop, Enter prepares
and requests installation authorization, and q/Esc/Ctrl-C cancel. The GIF/video preview
shows its first frame. Rendering uses the terminal's supported image protocol,
with colored half-block fallback. Conversion uses FFmpeg as the normal user.
JPEG/PNG become JPEG. GIF and MP4/M4V/MOV, MKV/WebM or AVI become animated GIF,
limited to the first 30 seconds with audio removed. Phone-video rotation is
applied before preview/crop. The output starts at 1024x640, 40% of this model's
2560x1600 panel, matching the PS dimension rule.

Conversion tries 20, 10, 5 and 2 fps at 1024x640, then 1 fps at 768x480 if needed.
The first candidate that passes all checks wins; APS reports its size and FPS.
The two output budgets are distinct:

- Acer's PS 5.1.514 code rejects sizes above **35,000,000 bytes**, not 35 MiB.
- APS additionally caps the sum of GIF image-descriptor `width * height * 4`
  at **64 MiB**. The inspected firmware retains decoded frame rectangles, so
  a small compressed file alone does not bound playback memory. This is an APS
  policy, not an Acer guarantee, and excludes firmware overhead/scratch space.

Input is limited to regular local files of 100 MiB / 64 million pixels. Container
signatures select fixed demuxers; URLs, playlists, audio-only files, symlinks and
external MOV references are not accepted. FFmpeg runs without privilege, with
bounded threads, decoder pixel limits, output bytes and conversion timeouts.
GIF validation also checks decoding, dimensions, frame count and duration in the
root helper before writing. These checks do not prove the firmware bug-free.

`prepare` only exports a new local file and never authorizes or touches the ESP;
existing output files are not overwritten. `set` and the picker pass the prepared
JPEG/GIF, never the source video, to the existing elevated writer. Firmware
playback has no audio and may end before the whole clip if boot continues.

Logo files stay entirely in userspace. `aps-boot`, installed root-owned under
`/usr/local/libexec`, receives Polkit authorization only after preparation. It
revalidates JPEG/GIF dimensions, format and decoding limits. It writes only the
vendor filenames `EFI/OEM/AcerLogo.jpg` and `AcerLogo.gif`, using mtools on the
original **unmounted** FAT ESP labeled `ESP`. This machine's partition identity
is pinned to `c1a78bb8-4f60-4b7b-8f47-2a1ba261c29d`; disk numbering is not used.
Missing/replaced/mounted/incorrectly labeled partitions fail closed. The Linux
`/boot/efi` partition is not a fallback destination.

Previous custom files are backed up under root-owned `/var/lib/aps/boot-logo/`
before replacement or reset. The new file is staged and read back before the
firmware-visible rename; interrupted/failed operations report their backup path.
After installation and file readback succeed, the helper automatically enables
`aps boot effects on` under the same authorization. An already-on setting is
left alone. If enabling or verifying the setting fails, the old logo files are
restored and the error reports that the firmware flag may have partially changed.
Preparing a file without installing it never changes the boot-effect setting.
Reset removes only the two custom filenames, leaving the firmware's built-in
logo and all bootloader files alone. Logo status also requests authorization to
inspect the protected ESP. No logo files, image parsers or filesystem operations
are added to the kernel. Dependencies are FFmpeg and mtools; neither is downloaded
by APS. Existing preferences and PRIME configuration are unaffected.

## Firmware GPU MUX

```bash
aps gpu mux status
aps gpu mux optimus
aps gpu mux nvidia
aps gpu mux auto
```

The three setters request PredatorSense's Optimus, NVIDIA GPU Only, or Automatic
Select firmware mode. A successful request requires a restart; APS never restarts
the machine. Optimus keeps NVIDIA available for rendering. Auto is the firmware
selection, not a promise of Windows Advanced Optimus behavior under Linux.
External display availability can change after switching. Open the lid before
testing a new mode if your external monitor may become unavailable.

Status is readable as the normal account; MUX writes require root. When run as
a normal user, the CLI requests Polkit authorization and executes only the fixed
root-owned `/usr/local/sbin/aps gpu mux MODE` with a validated mode. It never
elevates a workspace binary, PATH-selected executable or arbitrary command.
Running with sudo skips the dialog. Cancelling authorization does not fall back
to an unprivileged write. Help, status and lighting never request elevation.
The CLI checks its effective uid, and the kernel independently
requires CAP_SYS_ADMIN in the initial user namespace. `gpu_mux_mode` is created 0640
and the installed boot/udev rule preserves that root-write-only mode. Passing a
writable fd from root or loosening file permissions does not bypass the kernel gate.
Lighting permissions are unchanged. These use the model-locked sysfs interface.
They do not run prime-select, rewrite driver configuration, update initramfs, or
touch LCD overdrive. PRIME configuration is a separate task.

`gpu_mux_mode` reads the firmware-reported mode and accepts only optimus/nvidia/auto.
`mux_request` is read-only and records the last APS request/result since module
load. It is not durable and does not prove which GPU currently drives the panel.
A failed request may have partially applied; APS reports uncertainty and never
attempts a blind rollback. Requests are limited to one firmware attempt per two
seconds. The kernel validates the current getter before permitting a setter.

The preference service deliberately rejects MUX fields. No MUX setter runs at
module load, preference restore, lid transitions, suspend, resume, or unload.
Firmware owns the requested boot selection. Reboot and physical display-route
verification remain separate from a successful command response.

## Sleep and wake

### Closed-lid use

The kernel observes the ACPI button input device's EV_SW/SW_LID events and queues
work to read the authoritative ACPI lid state. It samples once at registration
and after resume, and checks synchronously before keyboard RGB, keyboard timeout,
or Turbo LED writes. There is no periodic lid query. Closing the lid
blacks out only the built-in keyboard and Turbo LED. Opening it restores their
cached settings. The cover logo, displays, external input devices, fans and
performance controls are not disabled by this policy.

With the lid closed, `aps` lighting writes fail with:

```text
The lid is closed (docked/headless mode). Open the lid first to change keyboard or Turbo LED lighting.
```

This is enforced by the kernel, including direct sysfs writes, using EHOSTDOWN.
The temporary off state never replaces cached intent or saved preferences. A
missing/failing lid sensor blocks internal lighting writes with EAGAIN until the
lid can be confirmed open. There is no writable lid override or fake-event path.
`lid_state` reports open/closed/unknown; `lid_status` adds change, attempt and error
counters. Both are read-only. Failed lighting transitions get up to three attempts.

Closed-lid boot defers the affected preference writes in the user-account service,
preserving them on disk while still saving other controls. The service retries at
five-second intervals; the first opening after a docked boot can therefore take
up to five seconds to restore those preferences. Normal subsequent lid changes
schedule work from the input callback without a polling interval. Performance writes reassert the
closed-lid blackout immediately and again after the firmware's mode indication.
Resume also checks the lid, so waking docked keeps internal lighting off while
restoring the back logo. None of this changes your desktop's lid/suspend policy.

### System sleep

The kernel module turns off the keyboard and Turbo LED before system sleep.
The cover logo defaults to off, but can use a separate suspend-only profile:

```bash
aps sleep logo static 0000FF 25
aps sleep logo breathing 25 3
aps sleep logo neon 25 3
aps sleep status
aps sleep off
```

These commands configure the next suspend without changing awake lighting.
Only Static accepts a color. Brightness is 0..100, animation speed 1..5.
The profile is kept in kernel memory and saved/restored by the user-account
`aps-state` service alongside normal preferences. Older records default to off
on a fresh module load. Keyboard and Turbo stay dark even if the lid was open
when suspending; there is no polling of the lid while asleep. Hibernation and
hibernation-image restore always use all-off. Firmware may override the logo
during sleep; an accepted request is not physical readback. No software animation
or CPU wakeup loop runs during sleep. A failed sleep-logo write is logged and
followed by one off attempt, without vetoing sleep.

After waking, the driver waits two seconds for the controller to settle, then
restores the current cached lighting settings, with the Turbo LED last. Four-zone
Static colors are preserved; native animations restart together. An aborted
suspend also triggers restoration. Failed writes are logged to dmesg and retried
up to three attempts in total.

Temporary sleep lighting writes do not replace your awake preferences or cached
choices. No userspace sleep script or kernel disk access is involved.
During sleep, hardware queries and control writes return EBUSY; cached choices
remain readable. APS does not change fan control merely because sleep starts.
Software effects still need their userspace process; the kernel can restore only
their last accepted frame. Settings never configured through APS have no cached
value to restore, so those components remain off until configured.

Check the callback and retry status without root:

```bash
cat /sys/class/misc/acer-phn16s/sleep_status
# awake cycles=1 attempts=1 error=0
```

## Sysfs ABI and kernel boundary

All attributes are under /sys/class/misc/acer-phn16s/.

| Attribute | Access for configured user group | Accepted write / read |
| --- | --- | --- |
| cpu_fan_rpm | Read | CPU RPM |
| gpu_fan_rpm | Read | GPU RPM |
| performance_mode | Read/write | Firmware readback: quiet, balanced, performance, turbo, eco |
| fan_control | Read/write | Firmware readback: auto, maximum, or manual CPU GPU |
| keyboard_lighting_capabilities | Read | Zone count and hexadecimal native-effect mask |
| keyboard_lighting | Read/write | off; static COLOR or four COLORS, brightness=N; native EFFECT brightness=N speed=N direction=VALUE |
| keyboard_backlight_timeout_seconds | Read/write | Firmware readback and writes: 0 or 30 seconds |
| turbo_button_mode | Read/write | power modes or hotkey |
| turbo_button_presses | Read | Number of recognized physical Turbo presses |
| predatorsense_button_presses | Read | Number of recognized PredatorSense presses |
| turbo_button_error | Read | Last button action's error code, zero for success |
| turbo_lighting | Read/write | off; static COLOR; auto manual=COLOR, remembering the manual color; manual reselects it |
| turbo_lighting_capabilities | Read | Fixed key-LED target, zone count, native-effect mask |
| cover_lighting | Read/write | off; static COLOR brightness=N; breathing/neon brightness=N speed=N; reapply latest cached setting |
| cover_lighting_capabilities | Read | Cover-logo target 83, one zone, native-effect mask |
| sleep_status | Read | awake/sleeping/restoring/error, sleep cycles, restore attempts, last errno |
| sleep_cover_lighting | Read/write | Same grammar as cover_lighting, configuring next suspend; default off; no reapply command |
| lid_state | Read | open, closed or unknown; cached ACPI lid reading |
| lid_status | Read | Lid state, changes, transition attempts and last errno |
| gpu_mux_mode | User read / root write | Firmware-reported mode; write optimus, nvidia or auto; restart required |
| gpu_mux_request | Read | Last APS request/result since module load, not active display routing |
| boot_effects | User read / root write | Firmware readback and writes: on or off |
| preferences_generation | Read | Change counter, not a writable control |
| work_status | Read | Event-work and one-shot deadline execution counters; reads do not schedule work |

Read/write controls use the same grammar in both directions. Short lighting
writes default to brightness 60, animation speed 3, and native/right direction.
Reads include those values explicitly. Only write-only firmware lighting state
without a known accepted setting reads `undefined`; performance, fan, MUX,
backlight timeout and boot-effect getters query firmware and return I/O errors
on failure, not `undefined`. Sleep lighting is a configured policy, initially Off.

Keyboard Static accepts one color for all zones or four colors in zone order.
Mixed per-zone settings use one line such as
`zones static FF0000 brightness=50 ; off ; neon brightness=60 speed=3 direction=native ; undefined`.
Each of its four entries targets one zone. `undefined` entries leave those zones
untouched on writes; an entirely undefined request is rejected. Complete reads
can be written back unchanged. These are cached accepted settings, not a physical
color getter. Lid/sleep blackout does not replace the desired values.

Version 0.4.0 removes the old filenames, including keyboard_effect/keyboard_state
and turbo_led/turbo_led_mode/turbo_led_manual. There are no sysfs aliases. APS
and its persistence helper translate their existing disk records to this ABI;
saved user preferences do not need to be deleted or rewritten by hand. Upgrade
the CLI, helper, permission rule and module together. External scripts must use
the new filenames and grammar. This combined lighting format is a convenient
APS ABI, not strict one-value-per-file kernel-subsystem integration.

Since DKMS 0.5.1, Turbo HID and PredatorSense input callbacks directly queue
shared unbound workqueue work. There is no dedicated APS kthread or 250 ms queue-check
loop. ACPI/HID operations remain in sleepable process context, under the state
mutex. Input callbacks do not perform firmware calls. SW_LID is observed without
consuming the event, so logind retains its normal lid policy.

One-shot delayed work remains for wake/Eco settling and bounded RGB retries.
No deadline is armed while idle or sleeping, and blocked wake recovery cannot
requeue an already-due deadline in a busy loop. Suspend discards queued presses;
teardown stops/drains input, HID, PM and sysfs producers, cancels delayed/immediate
work synchronously, then frees the virtual input device and state. Workqueues
have scheduling latency; this removes the artificial polling delay, not all latency.
The userspace preference service has its own save/restore checks, unchanged here.

Direct writes also work as the configured normal user:

    printf 'balanced\n' > /sys/class/misc/acer-phn16s/performance_mode
    printf 'static FFB000 FF8C00 FF4D00 B3001B brightness=100\n' > /sys/class/misc/acer-phn16s/keyboard_lighting
    printf 'wave brightness=70 speed=3 direction=left\n' > /sys/class/misc/acer-phn16s/keyboard_lighting
    printf '0\n' > /sys/class/misc/acer-phn16s/keyboard_backlight_timeout_seconds

The kernel is the trust boundary, regardless of which program opens sysfs:

- Writes are bounded to 128 bytes, or 512 for a complete keyboard record, and
  parsed into fixed typed operations before hardware I/O. A multi-zone write
  holds the state mutex, but firmware cannot apply four reports transactionally.
  Partial failures retain successful zones and invalidate ambiguous failed ones.
- Extra arguments, NUL/non-ASCII data, bad numbers, unsupported effects,
  out-of-range values and invalid zone masks are rejected before hardware I/O.
- Zones are limited to the four verified keyboard zones. Static/off reject speed
  settings; left/right direction is exposed only for Wave. Other captured native
  animations use the bounded `native` parameter, with legacy `none` records still accepted.
- HID targets, descriptors, report IDs, lengths and the ACPI path are kernel-owned
  constants. Userspace cannot choose them.
- Production builds have no generic ACPI, HID, memory, port, fan-table or ioctl
  interface. Dev builds add signed raw calls through fixed HID/WMI transports,
  deliberately bypassing the normal firmware command/value allow-list.
- Only fixed F14/F15 events are synthesized from the two filtered physical keys.
  Userspace cannot submit keys or trigger a fake press.
- No display/overdrive, battery or unrelated controls are implemented.
- Hardware requests and state are serialized inside the kernel. HID references
  and device locks protect unplug/unbind; buffers are kernel allocated.
- The CLI has no unsafe blocks or hardware FFI. It is independently compiled.
- Read-only attributes are 0440; controls are 0660, root-owned with the configured
  user's primary group. Raw HID devices retain root-only access.

keyboard_lighting reads all four cached zones, not just the last submitted zone.
There is no verified firmware getter for current colors. Until a zone has been
configured it is undefined, not assumed to be off.

These checks do not prove kernel bugs impossible. Small documented unsafe blocks
remain at the kernel C ABI. The boundary does not rely on the userspace parser,
on keeping the CLI unmodified, or on secrets.

## Separate bundles and installation

- dist/aps-userspace-0.2.0-linux-x86_64.tar.gz: APS binary, aps-state helper, service template,
  example presets, normal-user CLI installer and userspace documentation. No module or keys.
- dist/aps-kernel-0.2.0-7.1.5.tar.gz: signed module, one-time installer,
  permission helper, udev rule and uninstaller. No userspace CLI or key material.

This laptop now uses the production-only `acer-phn16s/0.3.0` DKMS source package.
The current kernel's module is installed under
`/lib/modules/7.1.5-76070105-generic/updates/dkms/acer_phn16s.ko.zst`.
/etc/modules-load.d/aps.conf loads it after reboot; the udev rule restores narrow
sysfs permissions. Migration did not unload the running module or change MOK enrollment.

DKMS rebuilds for eligible kernel updates using the target headers and matching
system Rust compiler. HID/input layout constants are regenerated, with field and
callback type checks. Missing Rust metadata is downloaded through APT at the exact
header-package version into a root-owned cache. Missing compiler/API compatibility
fails the build instead of reusing the old module. See
[DKMS installation and limits](packaging/dkms/README.md) for details.

```bash
dkms status -m acer-phn16s
sudo python3 scripts/install-aps-dkms.py
```

The older fixed-kernel binary-bundle installer takes the normal account name.
Do not install that unmanaged bundle over a DKMS installation:

    sudo ./install.sh ashwinmhaskar

The userspace bundle can run directly, or install-user.sh installs to
~/.local/bin/aps without root. The requested system installation here is
/usr/local/sbin/aps.

Boot recovery: this BIOS can report raw performance mode `2` before APS sets a
mode, with Balanced as the firmware boot default. The driver accepts only the
documented named modes for writes; it never replays `2` as a setter value. A
valid but unrecognized prior mode no longer blocks restoring the saved mode.
Transport or malformed firmware replies still fail, and every setting is
checked by readback. A failed selection can roll back only to a recognized mode.

The user-account helper retries unavailable settings every five seconds while
restoring independent settings. It retains the existing disk preferences until
restore and capture succeed. Shutdown during an incomplete restore preserves
that file instead of overwriting it with firmware defaults. Permission guidance
is shown only for actual permission errors, not firmware I/O failures.

Every module binary is tied to the kernel it was built for. DKMS handles rebuilds
for compatible updates, but cannot guarantee future API/toolchain compatibility.
The model/board/BIOS guards remain unchanged. The legacy binary archive itself
is not a DKMS package.

## Build and verification

    cargo test
    cargo clippy --all-targets -- -D warnings
    cargo build --release
    bash scripts/generate-hid-layout.sh
    bash scripts/build-kernel.sh

    # Normal-user boundary validation, no accepted hardware writes.
    cargo test --test sysfs_contract kernel_rejects_invalid_requests_without_changing_control_state -- --ignored --exact
    # Changes fans temporarily, then restores their original setting.
    cargo test --test sysfs_contract fans_persist_after_cli_exit_without_a_lease -- --ignored --exact

The closed-lid and open-lid tests must be run separately in their indicated
physical states; do not run all ignored tests together. The open-lid test writes
back existing lighting values. Sleep-preference tests change only that memory
policy and restore it, without suspending the machine.

The module build uses locally extracted distribution Rust 1.91.1 and exact kernel
Rust metadata. HID layout constants come from the local kernel headers; no C
driver implementation is compiled.

Verified: normal-user status/lighting/timeout/profile/fan writes; malformed direct
sysfs writes rejected with EINVAL and no control-state change; Secure Boot
signature identity. Current ABI and persistent-fan behavior have dedicated tests.
research/PROTOCOL.md documents the
supplied-binary and local-firmware command provenance.

## Removal

Stop active APS fan/effect commands, then run the kernel bundle's:

    sudo ./uninstall.sh

It unloads and removes only APS kernel/autoload/permission files. The CLI and
presets remain. Remove /usr/local/sbin/aps separately if desired. Firmware
lighting/profile choices can persist independently of the module.
