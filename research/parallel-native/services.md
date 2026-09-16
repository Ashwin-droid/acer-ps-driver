# Native Acer service color-selection chain (offline)

Scope: Acer PHN16S-71 / FTYPE_ARX / BIOS V1.28. This is an offline static analysis of the supplied support-ZIP services and the services registered on the mounted Windows installation. No hardware request, Windows write, install, download, or runtime service invocation was made.

## Result

The native service chain does preserve `colortype` longer than the final HID report suggests, but it does **not** expose a second, hidden fixed-color Breathing command for this Acer HID keyboard.

`AcerAgentService` appends a one-byte color-type discriminator to the textual RGB color, transports the resulting four-byte color as one of six fixed-width named-pipe arguments, and `AcerLightingService` uses that extra byte only to select OpenRGB `color_mode` 2 versus 3 when the selected mode advertises flags `0x40` and `0x80`. The chosen mode, color, speed, brightness, and direction are then committed through the ordinary OpenRGB controller update. For an `AcerHID` controller the inspected keyboard DLL ultimately serializes only effect, brightness, speed, direction, RGB, and zone mask into A4; `color_mode` is not a distinct HID field.

Consequently, changing JSON `colortype` or `random` does not add a distinct field to A4 beyond effect/RGB/direction. A Windows-only success could still depend on target selection or other controller state around A4, an additional device call not present in this service path, or different HID traffic produced by another component. No such extra service command was identified here; this service analysis does not prove that call ordering or controller state is irrelevant.

## Proven chain

1. **JSON to packed color.** Supplied `AcerAgentService.exe.c` `FUN_140033660` at VA `0x140033660`, lines 35599-35668, masks `colortype` with `3`, ORs in bit 1 for the alternate conversion path, and appends its two-hex-digit value to the color string. Call sites at lines 34186/34294/34334 parse the JSON `colortype`; the ordinary effect route then calls `FUN_14001f0f0` at line 34202. The installed executable has the same behavior in shifted function `FUN_140030610` at VA `0x140030610`, installed decompile lines 32890-33030, called from lines 32124/32231/32264.

2. **Agent-to-lighting IPC.** Supplied `FUN_14001f0f0` at VA `0x14001f0f0`, lines 23669-23810, opens `\\.\\pipe\\AcerLightingService_NamedPipe`. It parses four color bytes and sends command group/code `(2, 6)` with six 4-byte arguments: device index, effect/mode index, packed four-byte color, speed, brightness, direction. The installed equivalent is `FUN_14001c5f0` at VA `0x14001c5f0`, installed lines 21725-21841. There is no separate `random`, `colortype`, or pre-initialization argument.

3. **Lighting-service dispatch.** Supplied and installed `AcerLightingService` decompile identically (`cmp` equal). Its named-pipe handler at lines 19460-19545 reads exactly those six values, byte-reorders the packed color, checks that OpenRGB is ready, and calls `FUN_140014d40` at VA `0x140014d40` (line 19540). Device index 1 additionally triggers only a post-update AcerHID backlight-timeout refresh; it does not issue a color-selection command (lines 19545 onward).

4. **Mode/color selection.** `FUN_140014d40`, lines 15399-15738, locates the selected OpenRGB mode by its advertised mode value. It writes the supplied color into the mode color vector (lines 15623-15627). If mode flag `0x40` is present and the extra color byte's low bit is clear, it sets `color_mode = 2`; if flag `0x80` is present and that bit is set, it sets `color_mode = 3` (lines 15628-15637). Speed and direction are validated/clamped afterward, then `FUN_140013bf0` schedules the normal update (line 15724). This is host-side OpenRGB mode metadata, not an additional Acer HID report field.

5. **Persisted metadata is capability/state, not another command.** `AcerLightingService` writes each mode's `flags`, color-count bounds, `color_mode`, first color, direction, and speed to `LightingProfile.ini` at lines 8000-8075. Agent capability serialization reports `mode_flags` and `color_mode` from the OpenRGB mode structure at supplied lines 43514-43614 (installed lines 40448-40548). These getters do not set lighting and reveal no alternative callback.

6. **Special callbacks do not help keyboard Breathing.** Other `FUN_140014d40` callers are: a MAG-key delayed mode restore at line 14094, an effect `0x22` special route at line 15980, profile restore at line 17987, and the named-pipe handler at line 19540. The only explicit pre/post sequence found waits around MAG-key direct mode and later restores LEDs (`0x140014d40` caller around lines 14050-14110). It is not used by ordinary keyboard Breathing. The named-pipe handler's effect `0x22` special case applies only to device index 0, not the keyboard device index 1.

## Installed-versus-ZIP result

- Installed `AcerAgentService.exe`: SHA-256 `46533208b86b48ea901c8ce41368c44b512ba57b941d3e02048b08c039959484`; independently decompiled to `services/decompiled/AcerAgentService.exe.c` (99,967 lines). Its addresses/layout differ from the ZIP Agent, but the color conversion and six-field lighting IPC are semantically the same.
- Installed `AcerLightingService.exe`: SHA-256 `4f07da2fd8ae5a9562254fb33f7434c83dd2a724ac2880ebfc82ed43ec18b66d`; independently decompiled to `services/decompiled/AcerLightingService.exe.c`. Its complete 29,362-line decompile is byte-for-byte identical to the existing ZIP decompile, despite PE hashes differing (consistent with non-code/resource/signing differences).
- Thus the registered installed service pair supplies no newer lighting-service branch that would preserve a distinct fixed/random selector into the Acer HID wire command.

## Interpretation of `random` and the saved recipe

`random` is application/scenario state that determines the `colortype` supplied to the Agent; it is not present in the six-field service IPC. `colortype` survives as the fourth packed color byte and can change OpenRGB's in-memory `color_mode` when the mode advertises the corresponding flags. On this controller that distinction collapses before/at the Acer HID serializer because A4 has only RGB and no color-mode byte. This explains why the saved Windows Breathing recipe can contain `flags=81`, `color_mode=2`, and RGB `00 AE C7`, yet its exact A4 replay still cycles.

The decimal `flags=81` is `0x51`: it contains `0x40` but not `0x80`. Under `FUN_140014d40`, that capability permits the `color_mode=2` branch for the appropriate color-type bit, but not the `color_mode=3` branch. This is evidence that the saved profile describes the host library's single-color mode choice; it is not evidence of a different hardware selector.

## Exact differentiators and next tests (not run)

Further **new direction-value guesses** are low-value, but the existing direction-1 result has an important caveat. Typed trials covered directions 0 and 2. The exact saved-Windows direction-1 packet was sent raw after an `aps-probe` restore and may have inherited A2 target `0x65` rather than explicitly reselecting keyboard target `0x21`. Therefore a correctly ordered `A2 21` selection followed by the saved direction-1 A4 remains a legitimate sequence test. The static-color prime failed, but it also does not prove that every native ordering/state sequence is exhausted.

The decisive Windows-boot test is a **traffic differential**, not another blind write:

1. Before Windows boot, run the one remaining bounded sequence check: explicitly select keyboard target `0x21` with A2, then send the exact saved direction-1 A4 recipe. Treat visible behavior—not transfer success—as the result.
2. In Windows, capture actual HID feature SET traffic while PredatorSense applies Breathing once with scenario random-color enabled and once disabled, keeping RGB/speed/brightness/direction identical.
3. Simultaneously record the Agent request or LightingService named-pipe six-field payload/log. Confirm whether only the packed color's fourth byte changes and whether both runs emit the same A4 bytes.
4. If A4 differs only in RGB or is identical, capture all SET_FEATURE reports immediately before and after A4. Any fixed-color behavior must correlate with an additional report/target/control call or reproducible target/state sequence; that is the only justified candidate to port.
5. If Windows itself also cycles in the supposedly fixed setting, close the native fixed-color Breathing investigation: the UI/profile metadata is aspirational or controller-unsupported for this model/firmware.

Alternative offline work, if needed, is to trace the installed `AcerECKeyboardController.dll` update callback from OpenRGB `color_mode` assignment through its virtual controller method and explicitly prove where mode 2/3 are discarded. Existing analysis already proves A4 lacks that field, so this would strengthen attribution but is unlikely to yield a new command.

## Artifacts

- `services/decompiled/AcerAgentService.exe.c`: isolated installed Agent decompile.
- `services/decompiled/AcerLightingService.exe.c`: isolated installed Lighting decompile.
- `services/agent_color_parse.c`, `agent_mode_parse.c`, `agent_lighting_ipc.c`, `lighting_profile.c`, `lighting_dispatch.c`: focused excerpts from the supplied decompiles.
- `services/agent_hits.txt`, `lighting_hits.txt`: string/key hit inventories.

No claim above treats a successful transfer as visible confirmation. The conclusion is limited to the inspected native service path and this exact installed package/model context.
