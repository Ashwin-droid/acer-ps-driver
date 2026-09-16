# Skeptical APS protocol audit for fixed-color Breathing

Scope: Acer Predator PHN16S-71, FTYPE_ARX, BIOS V1.28, ENEK5130 `0CF2:5130`. This is an offline source audit. I made no hardware calls and did not rebuild the driver.

## Bottom line

The signed raw A4 tests did bypass APS's named-lighting transformations. For an 11-byte lighting SET, the envelope checks identity, freshness, signature, report ID, direction of transfer, and exact length, but it does not decode or rewrite bytes 1 through 10. The kernel copies those bytes directly into the HID request buffer. Therefore the tested packet `a4 21 04 64 05 01 00 ae c7 0f 00` reached `hid_hw_raw_request` unchanged, assuming the signed envelope actually contained those bytes.

What the test output does not prove is readback. `dev_probe` records the buffer returned by the SET call. Since the buffer starts as a copy of the submitted packet, the displayed A4 bytes can be only the original request left in memory after a successful transfer. There is no A4 GET in the descriptor contract or APS allow-list. The normal `keyboard_effect` and `keyboard_state` attributes are also software caches updated after SET success, not controller getters.

I found no packing, byte-order, report-ID, report-length, zone-mask, or effect-bit mismatch between APS and the installed Windows DLL. Target-selection state remains a live uncertainty for the raw direction-1 test. That makes a missing "fixed" selector inside the known A4 fields less likely, but it does not prove that fixed-color Breathing is impossible. A higher-level Windows transaction, another target/report, a controller power-state transition, or an undocumented sequence could still matter.

## Packet construction matches the installed DLL

Confidence: high.

The HID descriptor at `research/firmware/enek5130-descriptor.bin` defines numbered Feature report A4 with these usages and widths:

| Full byte | HID usage | Width | Meaning from DLL | APS field |
| --- | --- | ---: | --- | --- |
| 0 | Report ID A4 | 8 | report ID | `0xa4` |
| 1 | 21 | 8 | selected device/target | `0x21` |
| 2 | 41 | 8 | effect, logical max 31 | mode |
| 3 | 43 | 8 | brightness, logical max 100 | brightness |
| 4 | 42 | 8 | speed | speed |
| 5 | 44 | 8 | direction/parameter | direction |
| 6 | 45 | 8 | red | R |
| 7 | 46 | 8 | green | G |
| 8 | 47 | 8 | blue | B |
| 9..10 | 48 | 16 | zone mask | little-endian mask |

The descriptor bytes supporting this are offsets `0x78..0xca`. A4 has one target byte, one effect byte, one brightness byte, five bytes for usages 42 and 44 through 47, then one 16-bit usage 48. Including the report ID, the report is exactly 11 bytes.

The installed DLL's `FUN_180012900` accepts parameters `(target, mode, speed, direction, brightness, red, green, blue, zone)`, saves them at lines 13561 through 13569, then emits usages in descriptor order at lines 13682 through 13784. In particular, it deliberately writes brightness before speed even though speed is the third function parameter. APS does the same in `src/protocol.rs:38-52`, then patches byte 5 in `src/lighting.rs:199-217` or `kernel/hardware_hid.rs:266-275`.

The Windows `HidD_SetFeature` wrapper obtains `FeatureReportByteLength`, allocates and zeroes that size, puts the report ID at byte 0, copies at most `FeatureReportByteLength - 1` payload bytes, and submits the full feature-report length at `research/windows-installed/AcerECKeyboardController.dll.c:10917-10955`. Thus the DLL's temporary 64-byte builder does not show a required 64-byte wire transfer. HID caps reduce it to the descriptor-defined report size.

## Effect and capability mapping

Confidence: high for the mapping, medium for its meaning.

The installed DLL maps UI Breathing value 1 to wire effect 4 at lines 5964 through 5996. It maps UI direction 1 to wire value 1 at lines 6003 through 6020. `FUN_18000dc10` passes mapped effect, speed, mapped direction, brightness, RGB, and an all-zone mask to the controller at lines 10025 through 10058. This agrees with the saved recipe packet.

The descriptor says only that A3 usage 31 is a 32-bit value. The DLL treats it as a capability bitmap, and the higher layer tests `1 << (effect - 1)`. APS repeats that rule in `kernel/hardware_hid.rs:256-275` and `src/main.rs:144-163`. Live mask `0x000007fb` advertises effect 4 because bit 3 is set. This proves the controller advertises Breathing. It does not say whether RGB is honored by that effect.

The public mode list intentionally omits 3, 6, and 12 through 14. The installed mapping shows 6 as UI value `0x22`, consistent with the separate OPMODE use already documented. Values 12 through 14 are reachable from UI values 8 through 10 in the mapping function, but their names and applicability are not established here. They are not evidence for an alternate fixed-color Breathing mode.

## Color packing and the dropped high byte

Confidence: high.

The installed saved color `0x03C7AE00` has low RGB bytes `00 AE C7` and a high byte `03`. `FUN_18000dc10` copies exactly three bytes only when the profile color-mode field equals 2, at lines 10049 through 10055. It never passes the high byte to `SetMode`; the virtual call receives only three one-byte color arguments at lines 10056 through 10058. The high byte is metadata in the Windows profile representation, not an omitted A4 alpha or mode byte.

This also sharpens the Windows-profile inference. `color_mode=2` causes the installed DLL to supply the saved RGB bytes. Other color-mode values result in `00 00 00`. However, a scenario-level random-color choice can still alter which profile reaches this function. The INI alone does not prove the UI ever asked the DLL for fixed color.

## Target selection, initialization, and power assumptions

Confidence: high for APS behavior, medium for equivalence to every Windows path.

APS does more than send a bare A4 in the normal typed path. `set_light` calls `lighting_info`, which performs A1 GET, A2 SET selecting target 21, waits 20 ms, and performs A3 GET before A4 SET at `kernel/hardware_hid.rs:137-151,256-275`. The installed DLL's base-info route likewise selects the target, waits 20 ms, and reads A3 before using it, at `AcerECKeyboardController.dll.c:13255-13260`. Therefore target selection does not explain the typed direction-0 and experimental direction-2 failures.

The exact saved-Windows packet with direction 1 was not sent through that typed path. It was sent only through signed raw `aps-probe`. That raw path opens the model-locked HID device and submits A4 directly. It does not send A1/A2/A3 first. See `kernel/hardware_hid.rs:236-253`. The probe restore helper can restore keyboard state and then Turbo LED state, leaving A2 on target 65. The target byte inside A4 may or may not override that prior selection. As a result, explicit A2 target 21, a 20 ms wait, verified A3 target 21, and then the exact direction-1 A4 packet is an untested combination. Selection state is a credible confounder for that test, though not for the typed direction-0/2 failures.

No inspected A4 path has an explicit commit, latch, power-on, or acknowledgement transaction after SET. The Windows writer returns the boolean result of `HidD_SetFeature`. APS likewise treats an exact-length return from `hid_hw_raw_request` as success. Neither is visible-state verification.

## Zone semantics

Confidence: high for the mask encoding, medium for effect-specific behavior.

The DLL constructs the global mask as `(1 << zone_count) - 1` for zone counts below 17 at lines 10039 through 10044. With four zones this is `0x000f`. Its per-zone route constructs `1 << index` and sends one RGB triplet per call at lines 9971 through 10003. APS matches both patterns in `src/lighting.rs:255-279`.

Nothing in the descriptor or DLL proves that effect 4 consumes per-zone RGB or even consumes RGB at all. The four-zone capability describes the target generally. A useful negative result would be all four one-bit masks, separated by the vendor's 20 ms delay, each carrying the same color. If all still cycle identically, a hidden global-versus-zone interpretation becomes less likely.

## What status actually means

Confidence: high.

There are three distinct echoes that can look like readback:

1. Signed SET result. `Hid::report` allocates a buffer initialized with the submitted packet, calls `hid_hw_raw_request`, then returns that buffer at `kernel/hardware_hid.rs:99-134`. `raw_probe` stores it at lines 236 through 253. Unless the HID transport overwrites a SET buffer, the output is the request itself.
2. `keyboard_effect`. The driver assigns `state.last_light = Some(setting)` only after SET returns success, at `kernel/acer_phn16s.rs:266-283`, and prints that cached struct at lines 232 through 246.
3. `keyboard_state`. The driver updates the affected entries in `state.keyboard_zones` from the submitted setting at lines 274 through 280 and prints those cached entries at lines 154 through 173.

None reads current mode or color from ENEK5130. A4 GET is rejected by both the signed-envelope grammar at `src/dev_protocol.rs:78-84` and the HID transport allow-list at `kernel/hardware_hid.rs:103-106`.

## Interpretation of the existing tests

Confidence: high for transport conclusions, medium for firmware conclusions.

* Mode 4 with directions 0, 1, and 2 cycling RGB is strong evidence that byte 5 is not a fixed-versus-cycle selector on this firmware.
* Static blue followed by mode 4 blue rules out the simple hypothesis that Breathing reuses the prior static color while ignoring its own RGB fields.
* The exact installed recipe cycling shows that a bare raw A4 with the decoded RGB, direction, speed, brightness, and global zone mask did not produce fixed color in the state tested. It does not rule out a required preceding A2 target-21 selection because the restore flow may have left target 65 selected.
* Mode 3 leaving a steady/no-change keyboard does not establish that mode 3 is static Breathing. It may be unsupported and ignored, or may leave the prior state untouched.
* A successful SET and an echoed packet establish transport acceptance only. They do not establish that firmware recognized every field or changed visible state.

## Smallest distinguishing tests, if more hardware testing is authorized

These are conceptual tests only. I did not run them.

1. **Resolve the raw direction-1 selection confounder.** Send A2 target 21, wait 20 ms, GET A3 and verify target 21/mask, then raw-send the exact direction-1 A4 recipe. This precise combination has not been tested. Compare it with the same A4 after selecting target 65. If only the first works, the target byte inside A4 is not sufficient selection state. If neither is fixed, selection is not the missing ingredient.
2. **Test global versus per-zone consumption.** Starting from a known static four-zone pattern, send four effect-4 reports with masks 1, 2, 4, and 8 and a distinct RGB per zone, 20 ms apart. This distinguishes "effect ignores RGB" from "effect requires per-zone programming" better than another mask-15 packet.
3. **Test RGB relevance without guessing a fixed selector.** Hold every byte constant and compare effect-4 packets with RGB red, green, blue, and zero. Record video over at least two full cycles. If phase and color sequence are identical after synchronized application, firmware is likely ignoring A4 RGB for mode 4.
4. **Capture the real Windows transaction.** Trace user-mode HID API calls while selecting fixed-color Breathing in the installed UI, if that UI exposes such a choice. ENEK5130 is HID over I2C, so this should not be described as USB capture. Compare the complete ordered report stream, including A1/A2/A3/A4, other report IDs, delays, and repeated A4 packets. This is the only strong test for an unobserved service-level sequence. The saved INI and DLL serializer cannot supply ordering evidence.
5. **Separate SET echo from device response.** Instrument only the return count and pre/post kernel buffer bytes for A4 SET. Do not label unchanged bytes as readback. If a device-state getter is later found in the vendor trace, add it as a separate operation rather than treating SET output as acknowledgement data.

## Audit verdict

I do not see a packet-format bug that explains the missing fixed-color result. The strongest current hypothesis is that ENEK5130 firmware mode 4 ignores A4 RGB and implements RGB-cycling Breathing on this exact machine. Two alternatives remain materially open: the raw direction-1 recipe may require an explicit preceding A2 target-21 selection, or Windows may send an unobserved sequence outside the known A4 serializer. Replaying the same bare 11-byte recipe will not distinguish them.
