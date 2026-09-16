# Offline EC/WMI keyboard-path investigation

Scope: Acer Predator PHN16S-71 / FTYPE_ARX / BIOS V1.28. This report uses only
the two supplied/deployed DLL decompilations, the mounted Windows files, and the
locally dumped ACPI tables. No firmware method, WMI method, EC register, HID
report, or device getter was executed.

## Result

The legacy EC/WMI keyboard path exists in both AcerECKeyboardController DLL
versions and this machine's firmware, but it is not the backend selected by the
installed Windows lighting profile. `LightingProfile.ini` says
`AcerHIDKeyboard Device`; therefore the already-tested A4 path is the applicable
Windows path for the saved Breathing recipe. The EC/WMI path is an alternate
implementation shipped by Acer, not evidence that Windows uses it on this
machine, and it is not yet a justified write candidate.

No separate legacy command was found that explicitly requests "Breathing with
fixed color." The legacy mode packet does carry one RGB triplet alongside mode,
speed, direction, and brightness, so it is technically interesting, but its
effect semantics cannot be inferred to differ from A4 without a getter capture
or an Acer-selected legacy-machine trace.

## `AcerGamingFunction` method pair and firmware transport

Both DLLs call WMI class `AcerGamingFunction`, method
`SetGamingKBBacklight`, with one 16-byte byte-array input and then sleep 30 ms:

- installed 1.0.3.10: writer `FUN_180015230` at `0x180015230`, call at source
  lines 15513-15589; callers at `0x1800063c0` and `0x18000bf90`.
- ZIP 1.0.3.9: writer `FUN_1800163f0` at `0x1800163f0`, call at source lines
  17037-17113; callers at `0x1800064a0` and the EC-global controller.

The local firmware implements `WMBH(Arg0, Arg1, Arg2)` at
`acer-wmi-table.dsl:4396`. It sets channel ID `CTID=0x4842`, copies `Arg2` to
`WMIB`, sets method ID `MTID=Arg1`, and triggers SMI through system-I/O byte
`WSSP=0xD0` (`WSMI`, lines 2086-2091). Thus the implementation is truly
firmware/SMI backed, not an alternate HID serializer.

Cross-evidence makes the following mapping high confidence:

- method `0x14`: `SetGamingKBBacklight`; calls `WSMI`, returns four-byte `BHSK`
  (`acer-wmi-table.dsl:4556-4560`).
- method `0x15`: `GetGamingKBBacklight`; calls `WSMI`, returns 16-byte `BHLK`
  (`acer-wmi-table.dsl:4562-4566`). The service getter
  `FUN_140023250` at `0x140023250` invokes the named WMI getter with a one-byte
  input and copies status plus ten returned bytes.

The numeric/name association is not printed as plain text in ASL; it is derived
from the named WMI clients plus the adjacent set/get cases and matching 4/16-byte
return shapes. Do not treat `0x14/0x15` as independently live-verified.

Firmware `WMBH` provides no keyboard-specific ASL logic for these cases. It
passes the complete buffers through the SMI mailbox. Consequently DSDT fields
`EC0.KBBL` (ERAM offset 0x48 bit 0) and `EC0.KBST` (offset 0x69 bit 1), shown at
`DSDT.dsl:39000` and `39065`, do not reveal RGB/effect implementation. `KBBL`
is only consulted by `GOTS(0x84)` to report flag `0x08`
(`acer-wmi-table.dsl:1755-1772`). That is an on/off/status flag, not a breathing
color selector. No safe direct-EC RGB register path was found.

## Legacy per-keyboard packet

Installed function `FUN_1800063c0` (`AcerECKeyboardController::SetMode`) is at
source lines 4367-4431; ZIP equivalent `FUN_1800064a0` is at lines 4474-4538.
Its semantic arguments are:

```
SetMode(mode:u8, speed:u16, direction:u16, brightness:u16,
        red:u8, green:u8, blue:u8)
```

Only the low byte of each 16-bit speed/direction/brightness argument is
serialized. Direct disassembly of installed 1.0.3.10 `0x1800063c0` establishes
this exact 16-byte input layout:

```
00 mode
01 speed low byte
02 brightness low byte
03 0x00
04 direction low byte
05 red
06 green
07 blue
08 0x03
09 0x01
0a..0f zero
```

Machine-code evidence: `xorps xmm0,xmm0` followed by `movups [rsp+0x50],xmm0`
zeros all 16 bytes (`0x1800064a8-0x1800064b0`). Explicit byte stores then write
mode at +0, speed at +1, brightness at +2, zero (`bl`) at +3, direction at +4,
red/green/blue at +5/+6/+7, and `mov word [...+8],0x0103` writes bytes 08=03,
09=01 (`0x1800064b5-0x1800064e0`). The remaining bytes stay zero. This corrects
the earlier Ghidra-only reconstruction, which had incorrectly placed direction
at byte 03 and RGB at 04-06. Confidence: exact/proven from installed machine
code.

This is the most relevant alternate native-effect packet because it holds one
RGB triplet. Nothing in either DLL labels an additional fixed/random-color bit.

## EC-global packet

`AcerECGlobalController::SetMode` is serialized by installed
`FUN_18000bf90` around lines 8912-8970 (ZIP counterpart around 9734-9792), then
sent through the same `SetGamingKBBacklight` writer. Direct disassembly of
installed `0x18000bf90` establishes its exact layout:

```
00 mode
01 speed low byte
02 brightness low byte
03 0x00
04 direction-derived value
05 red
06 green
07 blue
08 0x03
09 color-count/type: 0x01 normally, 0x03 when controller flag != 0
0a..0f zero
```

The RGB triplet is used only when the internal color representation type equals
2 (installed lines 8916-8921). Direction mapping is legacy-specific: source
direction 0 becomes 2, 1 remains 1, and other values become `value+1`
(lines 8905-8915). This mirrors the suspicious default-direction transformation
already seen on the HID side; it is not evidence of a fixed-color selector.
Machine-code evidence: `xorps` plus `movups [rsp+0x58],xmm0` zeros bytes 00-15
at `0x18000c121-0x18000c129`; explicit stores populate offsets +1, +2, +4..+7,
+9, +0, +3, and +8 at `0x18000c12e-0x18000c191`. No instruction changes
+0a..+0f before the WMI call at `0x18000c20d`. This corrects the previous
Ghidra-only uncertainty around bytes 07-09. Confidence: exact/proven from the
installed machine code.

The exported factories prove these are independently instantiable backends:
installed `CreateAcerECGlobalControllerClassInstance` at `0x1800090d0`,
`CreateRGBController_AcerECGlobalClassInstance` at `0x1800090f0`,
`CreateAcerECKeyboardControllerClassInstance` at `0x180009130`, and its RGB
wrapper at `0x180009150` (source lines 6812-6873). Their presence does not mean
device selection chose them.

## Related legacy calls, not fixed-color evidence

- `SetGamingRgbKb` writes each zone separately as a WMI `u64` whose low byte is
  a one-hot zone selector and next bytes are RGB (installed lines 4218-4250).
  This supplies static zone colors; it does not set an effect.
- `SetGamingLED` builds a variable packet starting with byte 0 = 8 and sets a
  zone-enable bitmap in byte 5 (`FUN_180005d30`, installed lines 4085-4180).
  This is described by the DLL as keyboard on/off/LED behavior, not as a color
  mode selector.
- `GetGamingKBBacklight` is the useful state getter. Its returned status plus ten
  bytes could show the firmware's actual legacy representation without writing,
  but it is still a firmware call and was intentionally not executed in this
  offline phase.

## Applicability to PHN16S-71

1. The local V1.28 ACPI table contains WMBH methods 0x14/0x15, so the transport
   is firmware-present on this exact firmware image.
2. Both the supplied DLL and the actually installed 1.0.3.10 DLL retain EC and
   EC-global factories, so these are not merely ancient symbols absent from the
   installed component.
3. The installed profile explicitly names `AcerHIDKeyboard Device`, while the
   machine exposes ENEK5130 and its saved recipe exactly serializes to A4.
   Therefore actual selection evidence points away from EC/WMI.
4. Both EC constructors parse SMBIOS type 172 to obtain capability/revision
   fields. The per-keyboard controller recognizes feature tag 8 values
   0/1/3/4; EC-global recognizes tag 0x17 values 1/2. The offline sources do not
   establish that this PHN16S-71's type-172 record selects either EC backend.

Conclusion: firmware-present, component-present, but unselected/unproven for the
keyboard on this laptop. Confidence: high.

## Next safe read-only candidates (require later authorization)

In order of evidentiary value:

1. In Windows, observe which controller factory the lighting service actually
   instantiates (ETW/debug log or debugger breakpoint only). This avoids any
   device write.
2. Capture the input and result of the service's existing
   `GetGamingKBBacklight` call. Expected candidate is WMBH method 0x15 with its
   normal one-byte input and a 16-byte firmware return. Validate status and keep
   the entire raw return; do not infer fields from position prematurely.
3. Extract and decode this machine's SMBIOS type-172 record offline, checking
   tags 8 and 0x17 against the constructor gates. This can establish whether the
   EC/EC-global controller is eligible without calling firmware.
4. If a write is ever considered, independently capture the existing Windows
   setter input and compare all 16 bytes with the machine-code-derived layout.

No EC/WMI setter is recommended now. A later bounded experiment would require
all four prerequisites: factory-selection evidence, type-172 eligibility,
verified exact 16-byte packet, and a getter/restore path. Display, overdrive,
fan, performance, raw EC-register, and unrelated WMBH methods remain out of
scope.
