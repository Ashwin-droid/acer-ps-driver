# Native HID keyboard path, offline trace

Scope: installed `AcerECKeyboardController.dll` 1.0.3.10, the ZIP's 1.0.3.9 DLL,
and the ENEK5130 report descriptor. No request in this note was executed.

## Result

The installed HID backend has no serialized fixed-versus-random color bit for
Breathing. Its complete A4 data model is target, effect, brightness, speed,
direction, three color bytes, and a 16-bit zone mask. The profile `flags` field
never reaches A4. `color_mode` only decides whether the three color bytes come
from the first saved color or are zero. The saved Windows recipe already takes
the only colored branch.

Version 1.0.3.10 and 1.0.3.9 implement the same effect mapping, direction
mapping, `color_mode == 2` gate, RGB extraction, and descriptor-driven A4 field
sequence. The newer DLL therefore supplies no alternate native request that
the earlier analysis missed.

## Proven call chain and structure

1. `CreateAcerHIDKeyboardControllerClassInstance` at `0x180009190` fixes target
   `0x21`, initializes a nine-effect controller, and obtains its A3 metadata
   through `FUN_180011ee0`. See
   `research/windows-installed/AcerECKeyboardController.dll.c:6880-6896`.
2. `RGBController_AcerHIDKeyboard` constructs its Breathing mode only when A3
   capability bit `0x08` is set. The mode record has public effect ID `1` and
   flags `0x51`; its defaults/ranges include speed 1..9/default 5, brightness
   0..100/default 50, and one color. See the packed record construction at
   `:9382-9403`. The installed INI's decimal `flags=81` is exactly `0x51`.
3. Applying a mode reaches `FUN_18000dc10` at `0x18000dc10`. It reads effect ID
   at mode-record offset `+0x20`, speed at `+0x40`, brightness at `+0x44`,
   direction at `+0x48`, color mode at `+0x4c`, and the color vector at `+0x50`.
   See `:10025-10058`.
4. The controller vtable maps public effect ID 1 through `FUN_1800081a0` at
   `0x1800081a0` to wire effect 4. It maps direction through `FUN_180008250` at
   `0x180008250`; public direction 0, 1, 2 becomes wire 2, 1, 3. See
   `:5962-6021`. Direct disassembly confirms both switch tables and returns.
5. `FUN_18000dc10` initializes RGB to `00 00 00`. Only `color_mode == 2` loads
   the first three bytes from the first color element. It then calls vtable
   slot `+0x18`, `FUN_180008370`, with mapped effect, raw speed, mapped
   direction, brightness, RGB, and `(1 << zone_count) - 1`. See
   `:10033-10058` and the forwarding call at `:6063-6081`.
6. `FUN_180012900` at `0x180012900` serializes the update. It starts with target
   and asks the parsed HID descriptor for A4 usage widths in this exact order:
   `41,43,42,44,45,46,47,48`. It supplies effect, brightness, speed,
   direction, R, G, B, and zone mask respectively. See
   `:13511-13569` and `:13678-13775` plus the continuation immediately after.
   `FUN_18000f300` then adds report ID A4 for `HidD_SetFeature`.

The descriptor independently proves all widths and the same order. At offsets
`0x7b..0xc9`, report A4 declares target usage 21, effect 41, brightness 43,
speed 42, then usages 44 through 47 as five consecutive 8-bit fields, followed
by usage 48 as one 16-bit field. Thus the final feature report is exactly:

```
A4 21 effect brightness speed direction R G B zone_lo zone_hi
```

There is no trailing byte, reserved flag, alpha/type byte, or checksum in the
descriptor or serializer.

## Color packing

The mode record's color vector stores 32-bit values. The HID apply path copies
the first three bytes in memory and discards byte 3. For saved `0x03c7ae00`,
little-endian storage is `00 ae c7 03`, so A4 receives `R=00 G=ae B=c7`.
This is not a decompiler ordering accident: machine code at
`0x18000dcde..0x18000dce9` loads `[color+0]`, `[color+1]`, `[color+2]`, and at
`0x18000dd0b..0x18000dd23` places them in the corresponding SetMode arguments.

The high byte `03` resembles a color-type/tag field in the profile format, but
the HID report has nowhere to carry it. Calling that interpretation proven
would be too strong. What is proven is simply that the HID writer drops it.

## `flags` and other suspected controls

- `flags=81` is decimal `0x51`, the same value embedded in the Breathing mode
  descriptor. It lives at mode-record offset `+0x24`. The apply function never
  reads that offset. It is UI/profile capability metadata, not an A4 control.
- `color_mode=2` is the sole colored branch. Any other value sends zero RGB;
  there is no alternate packing or second-color branch.
- Breathing is constructed with one color. Although the profile container can
  hold a vector, the apply path reads only the first element and emits one A4.
- The zone mask derives only from A3 zone count. Four zones produce `0x000f`.
  Per-zone writes exist as a separate static-color path and do not add a
  Breathing color selector.
- The report serializer makes up to five device-open attempts, but each attempt
  rebuilds the same bytes. Retry state does not change the request.

## Comparison with ZIP DLL 1.0.3.9

The old equivalents are effect mapper `FUN_180008200`, direction mapper
`FUN_1800082b0`, apply function `FUN_18000f0a0`, and serializer
`FUN_180013d00`. The mappings match at
`research/decompiled/AcerECKeyboardController.dll.c:6034-6093`; the apply gate
and argument flow match at `:11431-11464`; and the serializer queries the same
usages in the same order at `:15229-15324`. Address movement is consistent with
relinking and other code changes, not a revised A4 protocol.

## Candidate requests and exclusions

There is no evidence-backed, distinct A4 request for fixed-color Breathing in
these DLLs. The strongest next evidence would be a Windows HID capture while
the vendor UI visibly produces fixed-color Breathing. If that capture equals
`A4 21 04 brightness speed direction R G B 0f 00`, firmware or service-side
initialization outside this DLL must explain the behavior. If it differs, the
capture identifies the missing field or prerequisite.

For completeness, these values are reachable in code but are poor Breathing
candidates and should not be tried on the basis of this trace:

- Wire directions 3 through 6 are produced by public direction values 2
  through 5. Breathing metadata exposes only its normal direction choice, and
  nothing ties the extra direction codes to color selection.
- The effect mapper contains wire effects 12, 13, and 14 for public modes 8,
  9, and 10. A3 mask `0x7fb` excludes them on this keyboard, and their mode
  identities are Ripple, Snake, and Disco in the generic controller table,
  not alternate Breathing.
- Wire effect 6 is public OPMODE 0x22. It is a separate operating-mode effect,
  not a color-mode flag.
- Changing profile `flags` cannot alter the emitted request because the HID
  apply call chain does not read it.
- Changing `color_mode` away from 2 removes RGB. It cannot preserve the chosen
  fixed color through another encoding.

Bottom line: offline native-HID analysis is exhausted at the A4 boundary. The
tested request already matches the only colored Breathing request this installed
DLL can produce.
