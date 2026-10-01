# Same-file sysfs controls and persistent fan settings

Implemented and installed with production DKMS 0.4.0. The old sysfs filenames
are removed, without compatibility aliases. See the main README's ABI table.
Existing user-owned disk records retain their format; the userspace codec maps
them to the new paths and syntax. The kernel never reads preference files.

## Read semantics

Performance mode, fan mode/percentages, GPU MUX, boot effects, and keyboard
backlight timeout use their existing firmware getters. Failed or unsupported
getter results return errors, not invented defaults or `undefined`.

Keyboard, cover and Turbo lighting lack a verified current-effect getter.
They show `undefined` until configured, then the last accepted desired setting.
Lid/sleep overlays do not rewrite that desired state. Sleep-cover lighting is
an explicit policy with a known Off default, not a guessed hardware state.

`keyboard_lighting` reads one whole keyboard record. Static accepts one or four
colors, followed by optional named brightness. Native effects accept named
brightness, speed and direction. Mixed zone states use four semicolon-separated
entries after `zones`. Undefined entries leave those zones alone on writes;
an entirely undefined request is rejected. Parsing is allocation-free and
bounded to 512 bytes; other controls remain bounded to 128 bytes.

Each complete request validates before any hardware I/O. The state mutex covers
all zone writes and cache updates. Multi-report writes cannot be atomic in the
firmware; successful zones are retained and failed ambiguous zones invalidated,
with the change counter advanced even when a partial write returns an error.

`turbo_lighting` combines mode and manual color. In Auto it reads, for example,
`auto manual=B3001B`, so read/write round trips retain the remembered color.
`manual` selects the remembered color without needing a separate mode file.
All internal-lighting writes still require an open lid. GPU MUX and boot-effect
writes still require initial-namespace CAP_SYS_ADMIN, regardless of file access.

## Fan control

Both the kernel lease and userspace fan-session duration/renewal loop are gone.
`aps fans maximum` and `aps fans manual CPU GPU` apply once and exit. There is no
expiry or fan reset on process exit, suspend, or module unload. Firmware can
still override settings on its own transitions, and APS does not restore fan
overrides from disk at boot. The existing immediate rollback to Auto on a
failed fan configuration remains; malformed requests never reach hardware.

This does not remove firmware I/O deadlines, RGB settling delays, input
validation, or the separate optional firmware keyboard-backlight timeout.

## Verification on this machine

Before starting preference restoration after reload, actual sysfs reads were:

```text
performance_mode=quiet
fan_control=auto
keyboard_lighting=undefined
cover_lighting=undefined
turbo_lighting=undefined
keyboard_backlight_timeout_seconds=0
```

After the user opened the lid, the account service restored:

```text
keyboard_lighting=static FFB000 FF8C00 FF4D00 B3001B brightness=100
turbo_lighting=static B3001B
cover_lighting=static FF8C00 brightness=100
sleep_cover_lighting=static 0000FF brightness=100
```

The keyboard, Turbo and cover values were read and written back to the same
files successfully. Other controls were unchanged and the user preference file
retained its exact pre-upgrade hash. Closed-lid rejection, malformed/oversized
requests, old-filename absence and sleep-policy restoration passed live tests.
No suspend, reboot or GPU MUX switch was performed.

The fan CLI exited immediately for both Maximum and manual 55/65. Both firmware
readbacks remained unchanged after six seconds without renewal. The test then
explicitly restored the original Auto setting. The driver did not expire them.

Root-opened descriptors passed to an unprivileged child still returned EPERM
for MUX and boot-effect writes. Malformed root writes returned EINVAL without
changing either firmware selection. The loaded module matches installed
srcversion `BAF01D22ECD2B3680D92399`, is signed with Ashwin Module Signing Key,
and reports dev_mode=0. Prior CLI/helper/permission-rule binaries are retained
at `/var/lib/aps/sysfs-update.GsMMt1`; previous DKMS versions remain built.
