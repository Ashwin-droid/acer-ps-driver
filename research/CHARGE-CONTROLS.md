# PHN16S-71 battery charging controls, 2026-10-26

## Sources and model support

The supplied PredatorSense 5.1.514 UI describes Optimized Battery Charging as an
80% limit. The actual implementation is in `AcerCCAgent/AcerCCAgent.exe`, not the
gaming Agent/HardwareService pair previously used for fans and performance.
`scripts/ExportCharging.java` exported battery-related string xrefs, callers and
callees into ignored `research/charging/` using the existing portable Ghidra.
No vendor executable was run, and no third-party Acer driver was used.

## Windows UI confirmation on this laptop

The user booted Windows and supplied two screenshots in
`C:\Users\ash6c\Documents` on 2026-10-26:

- `Screenshot 2026-10-26 090005.png` shows Battery & Charge with Optimized
  Battery Charging explicitly limited to 80%, and Full Battery Charging
  selected. No AI/adaptive charging option or arbitrary laptop charging
  threshold is visible on this page.
- The same screenshot shows Power-off USB Charger and its 10/20/30% cutoff
  dropdown. This controls when to stop supplying the attached USB device from
  the laptop battery. It is separate from the laptop battery's charging ceiling.
- `Screenshot 2026-10-26 085945.png` shows battery health Good, battery power
  100%, temperature 41 C and cycle count 16. The health check timestamp is
  2026/09/08, so Good is a previously recorded check, not a fresh Linux diagnosis.

These screenshots confirm the charging options visible in the user's installed
Windows application. They do not establish that no other package/version could
offer adaptive charging. Shared-package AI charging strings alone do not prove
that feature is enabled for this model. Neither screenshot shows calibration.
Its support remains based on the native capability query and vendor code.

After the Windows visit, Linux readback still reported capabilities 0x03,
limit 100 and calibration off. No setting was changed during this inspection.

## Native interface

The mounted Windows WMI repository's BatteryControl class record at offset
0x4f60ab identifies GUID `79772EC5-04B1-4BFD-843C-61E7F77B6CC9`, version 2.94.
The local ACPI _WDG maps that GUID to WMBE. The V1.28 AML implements:

| Method | Operation | Result buffer |
| --- | --- | --- |
| 0x13 | GetBattInfoInterface | 4 bytes |
| 0x14 | GetBatteryHealthControlStatus | 8 bytes |
| 0x15 | SetBatteryHealthControl | 4 bytes; notifies BAT1 status change |
| 0x16 | GetBatteryFunctionData | 16 bytes |
| 0x17 | SetBatteryFunctionData | 4 bytes |

Only 0x14 and 0x15 are exposed through bounded typed kernel operations. Getter
input is battery index 1, query 1, followed by eight zero reserved bytes. Setter
input is index 1, function mask, status, and twenty zero reserved bytes. The
allowed masks are 1 for the fixed limiter and 2 for calibration; status is 0/1.
No command accepts a user-supplied method, mask, index, ACPI path or reserved data.

Vendor function `FUN_1403b4240`, OnSetBatteryHealthyPacket, maps optimized/full to
mask 1 enabled/disabled via AcerWMI::SetBatteryHealthControl at `FUN_140360740`.
`FUN_1403ac890`, StartBatteryCalibration, invokes the same method with mask 2.
The getter wrapper unpacks separate capability bits and state bytes. Return code
is a two-byte field; the reply layout is capability byte, return[2], status[5].

## Read-only signed probe and observed capability

A temporary MOK-signed, model-locked build exposed only fixed read queries.
The normal installed production module was restored immediately afterward.
It returned:

```text
GetBatteryHealthControlStatus: 03 00 00 00 00 00 00 00
```

Capabilities 0x03 advertise the fixed limiter and calibration. Adaptive charging's
bit 0x04 is absent. Both current feature states were off, corresponding to a
100% charge limit and inactive calibration.

The newer HID boundary query comes from AcerHID2025::GetBatteryBoundary at
`FUN_140326680`. Its fixed decimal request is `160,0,160,3,11,2,0,0`, padded to
65 bytes. The response began `A0 00 E0 03 0B 00 FF FF FF FF ...`; enabled/lower/
upper fields were unusable, not valid 0/1 and 0..100 values. No boundary setter
was sent. This does not prove every conceivable boundary path impossible, but
does not establish a usable programmable-threshold contract on this laptop.

Adaptive charging also has Windows-side learning/scheduling. It is not exposed
as a fake firmware toggle. The BIOS Normal/Fast Charging entry has no verified
control path here. Power-off USB charging and its 10/20/30% cutoff are separate
from the laptop's charge cap and are not modified.

## Limit verification

The first candidate used the wrong one-byte return-code offset. It rejected its
post-write readback and requested rollback. Diagnostic bytes established:

```text
100%: 03 00 00 00 00 00 00 00
 80%: 03 00 00 01 00 00 00 00
setter result: 00 00 00 00
```

The corrected decoder has these regression fixtures. The next signed candidate
passed 80 -> 100 firmware readback and restored the original 100% setting.
Calibration remained off, other capability/state bits were unchanged, and the
preference generation did not advance. The battery was already at 100%/Full;
therefore an actual recharge cutoff at 80% has NOT been physically verified.
No forced discharge or calibration cycle was started.

Root-opened descriptors passed to the normal user returned EPERM for both
setters. Malformed/out-of-range requests returned EINVAL without changing state.
`scripts/verify-battery-controls.py --test-limit` reproduces only this bounded
limit test and explicitly never starts calibration. It uses unbuffered writes
so a failed sysfs write is not retried during buffered-stream cleanup.

## Public interface and constraints

`battery_capabilities` is read-only. `battery_charge_limit` reads/writes 80 or 100.
`battery_calibration` reads/writes on or off. Setter mode is 0640 and the kernel
independently enforces initial-namespace CAP_SYS_ADMIN. Both use live firmware
readback, capability validation, suspend gating and the existing state mutex.
No battery settings enter the ordinary user preference save/restore format.
Firmware persistence across reboot is not yet verified.

The CLI requests Polkit for setters. Calibration start requires explicit
`--confirm`, AC power, limit 100 and inactive adaptive charging. The foreground
session runs under systemd-inhibit, holds a root-owned runtime lock, monitors AC,
and requests cancellation on interruption or AC loss. Direct root sysfs users
must arrange their own sleep inhibition. SIGKILL/crashes can leave native
calibration active; the stop command is provided. Off alone is not a success
result, since an aborted cycle also becomes inactive.

Calibration support and its command are protocol-traced, but a full calibration
cycle and its cancellation have NOT been physically exercised. No claim of
end-to-end calibration validation is made. Arbitrary voltage/current, force
discharge, adaptive charging and unsupported threshold commands are not exposed.
