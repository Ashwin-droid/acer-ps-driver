# APS userspace bundle

System Boot Effect: `aps boot status`, `aps boot effects on|off`, and
`aps boot logo pick [DIRECTORY] | set FILE | prepare FILE OUTPUT | status | reset`.
The picker previews highlighted/hovered images and videos in a Ratatui-image pane
before Enter opens the crop editor. FFmpeg handles automatic center crop
and resize up to 1024x640. Input JPEG/PNG/GIF, MP4/M4V/MOV, MKV/WebM or AVI;
output JPEG or animated GIF. Videos lose audio and use their first 30 seconds.
`aps boot logo prepare clip.mp4 prepared.gif` converts without installation;
`aps boot logo set clip.mp4` converts and requests authorization to install.
The picker shows the first frame and supports the same crop editor for videos.
Phone-video rotation is applied before cropping.

GIF conversion tries 20/10/5/2 fps at 1024x640, then 1 fps at 768x480. It picks
the first result within Acer's 35,000,000-byte limit and APS's additional 64 MiB
decoded-frame-rectangle budget. Input is at most 100 MiB. URLs, playlists,
symlinks and audio-only inputs are rejected. Both the CLI and elevated helper
validate the final GIF; the helper never receives or decodes source video.
These bounds reduce resource use but do not guarantee firmware bug-freedom.
Run conversion/picker as the normal user. Changes request Polkit authorization
for the separate root-owned `aps-boot` helper. The helper must be installed with
`sudo ./install-boot-helper.sh`; it needs the system's mtools utilities. It is
bound to the original Acer ESP identity for this machine. It never writes the
Linux bootloader ESP, bootloader entries or executable EFI files. Existing custom
images are backed up under /var/lib/aps/boot-logo/ before replacement/reset.
Installing a custom image automatically enables boot effects with the same
authorization. A failed enable/readback restores the previous logo files and
reports uncertainty about the firmware flag. Prepare-only and reset do not enable it.
Only the startup effects master flag is a kernel control; images stay userspace.
BIOS also has an independent sound mute setting, not yet exposed through APS.

Firmware GPU MUX: `aps gpu mux status` is unprivileged. Setters such as
`aps gpu mux optimus` automatically request Polkit authorization; using sudo
skips the dialog. Only the fixed installed root-owned APS binary is elevated,
with one validated mode argument. Status/help/lighting never elevate. The kernel
independently enforces administrative privilege. Mode requests go through
the separate kernel module and require restart, but APS never reboots. Status is
firmware-reported selection, not proof of the active GPU/display route. PRIME and
initramfs configuration are untouched. The preference helper rejects MUX fields.

Keyboard/Turbo lighting writes now explain a closed-lid kernel rejection as
docked/headless mode and ask you to open the lid. The lid rule is enforced by the
separate kernel module, not this CLI. Lid-closed boot is supported: aps-state
retains deferred keyboard/Turbo preferences on disk, continues capturing the other
controls, and retries the deferred settings every five seconds until the lid opens.
Partial keyboard restores retain the saved values of zones not yet restored.

`aps` is an unprivileged Rust program that only reads/writes the typed attributes
under `/sys/class/misc/acer-phn16s/`. No driver, hardware FFI, shared secret,
module loader or polkit helper is included.

The separate kernel package must be installed once and grant your account sysfs
access. Run this binary directly without sudo, or run `./install-user.sh` as your
normal user to install it to `~/.local/bin/aps`.

```bash
./aps status
./aps profile balanced
./aps fans manual 55 65 60
./aps keyboard timeout on
./aps lighting effects
./aps lighting apply breathing 80 2
./aps lighting apply shifting 100
./aps lighting apply --help
./aps lighting list
./aps lighting run rainbow
./aps lighting save my-strobe --effect strobe --colors FFFFFF \
  --brightness 60 --speed 2 --duty 0.5 --duration 15
```

Bare `aps lighting apply` only shows help. Positional syntax is
`EFFECT [COLOR] [BRIGHTNESS] [SPEED]`; named options still work.

```bash
./aps button mode "power modes"
./aps button mode hotkey
./aps turbo-led auto
./aps turbo-led manual FF0088
./aps turbo-led off
./aps logo FF00FF 60
./aps logo breathing 60 3
./aps logo neon 60 3
./aps logo off
./aps logo status
```

Cover-logo Breathing fades through RGB and Neon smoothly cycles RGB. They do
not support a chosen fixed color. Performance flashes are temporary firmware
indications, not selectable logo effects. Eco's flash is followed by a bounded
kernel reapplication of the latest normal logo choice. Legacy saved opmode
records migrate to Off while retaining all other valid preferences. Logo
settings persist through the same user-account service. Speed is 1..5, brightness
0..100. `logo on` means static white; use a hex color for another static color.

Sleep lighting has a separate preference:

```bash
aps sleep logo static 0000FF 25
aps sleep logo breathing 25 3
aps sleep logo neon 25 3
aps sleep status
aps sleep off
```

This configures the next suspend without changing the awake logo. Default: off.
The kernel keeps keyboard/Turbo off during sleep and restores normal lighting
after wake or an aborted suspend. Hibernation always stays dark. Only native
logo effects are available; firmware may override them during actual sleep.
`aps-state` saves `sleep_logo` under the normal account and reloads it at boot,
including while the lid is closed. Sleep settings never replace awake colors.

The kernel handles physical-button mode cycling/F14, and PredatorSense/F15.
Auto LED mode follows performance mode in firmware; Manual accepts RGB hex.
These controls require the matching updated kernel bundle. The CLI remains a
sysfs wrapper. A separate aps-state helper saves/restores active preferences as
your normal account, in user-owned 0600 ~/.local/state/aps/preferences. It refuses
to run as root. The kernel never opens or parses preference files.

This bundle includes aps-state and its systemd service template. The system setup
on this laptop installs the helper at /usr/local/libexec/aps-state and enables
aps-state.service with User=ashwinmhaskar, zero capabilities and a narrow filesystem
sandbox. It starts at boot without needing a GUI login. The standalone
install-user.sh installs only the CLI and preset recipes; service setup is separate.

`aps preferences status` reads the helper's user-owned status file. Corrupt,
oversized or unsafe preference files are rejected in userspace, and the kernel
independently validates every restored setting.

Only Static and userspace Strobe accept colors: one color or exactly four zone
colors. All other effects have no color argument. Old recipes for those effects
normalize their previously stored color away when loaded. All captured effects are also
available as `aps keyboard EFFECT ...`, using the same parser and defaults as
`aps lighting apply EFFECT ...`.

The preference helper submits the latest cached logo setting again 2 and 10
seconds after startup/reload, using a kernel-side atomic reapply. These bounded
attempts address accepted boot writes that firmware later ignores/overwrites;
they do not continuously override hardware, and they stop for developer probes.

After boot, the helper restores settings from disk even though the firmware
has reset to its defaults. If a control is unavailable, it retries that control
every five seconds and continues restoring independent controls. The last saved
file is retained until restore and readback complete. The service's
`save-on-stop` action preserves that file if restoration is still incomplete.
The CLI reports firmware I/O failures separately from access-permission errors.

Performance mode is restored before the other controls; Turbo LED color and
mode are the final restore writes, following a two-second settling wait after
that profile change. They remain pending if profile restoration fails, including
across retries. This avoids restoring a manual color before firmware applies
its own performance-mode indication.

The installer copies example presets without overwriting existing ones. Presets
live in `$XDG_CONFIG_HOME/aps/profiles`, or `~/.config/aps/profiles`.

Native effects use speed 0–9 and continue after the process exits. Strobe uses
flashes/sec; rainbow uses seconds/cycle. Those effects are generated in userspace.
Fan overrides require lease renewals; the kernel requests Auto within about five
seconds after renewal stops. A crash or modified CLI does not bypass the kernel's
independent request validation.

Linux x86-64; uses system libc and libgcc_s. No module or Secure Boot key material
is included. The kernel installer is a separate artifact.
