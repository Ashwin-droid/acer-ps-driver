# Reconciled native-lighting investigation

Six Sol agents independently reviewed the installed keyboard DLL, legacy EC/WMI
paths, services, bundled OpenRGB dispatch, Windows configuration, and APS's
protocol implementation. The coordinator reconciled conflicting interpretations
and checked critical packet offsets against installed machine code.

## Outcome

No working native fixed-color Breathing request was found for PHN16S-71 BIOS
V1.28. This is not a proof that the hardware can never support it. It is a
negative result for the inspected Acer-native paths and the controlled HID
transactions tested on this machine.

The review found a real test-history gap: earlier direction-1 A4 was raw-only
and followed a restore that could leave A2 selecting target 0x65. Typed trials
had covered directions 0 and 2, not 1. The coordinator then ran this full
sequence through the already-installed signed dev interface:

```text
A1 GET                  -> a1 03 65 21 83
A2 SET a2 21            -> success
A3 GET                  -> a3 21 01 09 04 fb 07 00 00
A4 SET                  -> a4 21 04 64 05 01 00 ae c7 0f 00
```

The returned A3 explicitly confirmed keyboard target 0x21, four zones and mask
0x000007fb before A4. The user reported: **Still cycles RGB**. This closes the
selection-state confounder for the tested recipe. The user's normal static zone
colors were restored, the developer session ended, and preference saving resumed.
The individual agent reports describe their state of knowledge before this
coordinator test; any "untested A2 + direction 1" recommendation there is now
superseded by this result.

## Native path inventory

| Path | Evidence and interpretation | Breathing relevance |
| --- | --- | --- |
| Acer HID keyboard mode update | Active backend in retained logs/profile; OpenRGB loads concrete AcerHIDKeyboard factories | UI mode 1 becomes A4 effect 4; chosen RGB is present; no separate fixed/random field |
| HID static bulk/per-zone update | Native UpdateLEDs and single-zone callbacks use effect 2 | Static color programming, not another native Breathing effect |
| Legacy EC keyboard mode | Separate DLL factory and named SetGamingKBBacklight client, 16-byte buffer | Potential alternate backend, but not selected by this installation and not justified as a blind write |
| Legacy EC-global mode | Separate factory, related 16-byte packet with controller-dependent byte 9 | Another shipped backend; model eligibility not established from available SMBIOS cache |
| Legacy SetGamingRgbKb | Per-zone bitmap plus RGB in WMI argument | Static colors, not a Breathing selector |
| Legacy SetGamingLED | Zone/on-off behavior packet | No independently evidenced fixed-color Breathing control |
| EC HID keyboard-backlight status/timeout | Service refresh after ordinary keyboard mode update | Separate brightness/30-second-counter path; no color selection found |
| HID ModeKey/CoverLogo controllers | Separate registered targets 0x65 and 0x83 | Distinct LED components; their modes must not be treated as keyboard encodings |
| Hybrid/backend selection | CheckDeviceStatus, SMBIOS type 172 and HybridMode gate factories | Chooses implementations; not an identified live color-selector command |

## Important positive findings

1. **The actual 11-byte A4 layout matches APS.** The DLL dynamically obtains
   field sizes, but this ENEK5130 descriptor resolves to the same bytes and
   length as APS. The vendor's 64-byte temporary allocation does not mean a
   64-byte feature report should be sent.
2. **The color-type chain is accounted for.** Agent JSON colortype becomes a
   fourth color byte in six-field named-pipe IPC. LightingService uses it for
   OpenRGB color_mode selection under capability flags. The HID writer sends
   only the RGB bytes when color_mode equals 2; it sends no fourth color byte.
   Saved flags=81 is decimal 0x51, not an HID flag byte 0x81.
3. **Installed and ZIP service behavior was compared.** An isolated decompile
   of installed LightingService matched the ZIP decompile byte-for-byte. The
   installed Agent has shifted addresses but the same relevant color/IPC flow.
   Different PE hashes do not establish a different native lighting protocol.
4. **Legacy packet reconstruction was corrected using assembly.** Ghidra's
   overlapping locals initially suggested the wrong offset. The exact installed
   per-keyboard 16-byte packet is:

   ```text
   mode speed brightness 00 direction R G B 03 01 00 00 00 00 00 00
   ```

   The EC-global variant uses the same byte positions with byte 9 equal to 01
   or 03. Zero-initialization and explicit stores are visible in machine code.
   The WMBH 0x14/0x15 mapping to the named setter/getter is still an inference
   from clients, adjacent cases and return sizes, not independently live proof.
5. **Transfer echo is not hardware readback.** A4 SET success and dev-probe's
   displayed packet do not prove the controller honored the requested effect
   or RGB. The visual observations, not those echoes, determine test outcomes.

## What not to conclude or try blindly

- No Windows session has yet demonstrated fixed-color Breathing on this device.
- Extra direction values, unsupported effect IDs and other LED targets are not
  evidence-backed substitutes for fixed-color keyboard Breathing.
- The legacy route's presence in the package and SMI dispatcher does not prove
  it is eligible or safe to use for this HID keyboard. Its SMBIOS selection
  prerequisites and getter/restore behavior remain unverified.
- The current signed WMI probe contract accepts 4/8-byte input/output buffers;
  the legacy keyboard route needs different lengths. This report does not claim
  those legacy calls were run through the existing interface.

## Next decisive evidence

If continuing with a Windows session, capture an actual paired Random-on versus
Random-off Breathing apply, with one selected color and unchanged speed and
brightness. First establish that the nominal fixed setting visibly works. Then
compare the entire user-mode HID feature-request stream, including preparation,
other reports and targets, with Agent/service color metadata and loaded module
paths. This is HID over I2C, so a USB-only sniffer is not the right starting point.

If Windows visibly achieves fixed-color Breathing, its traffic identifies the
missing sequence. If it also cycles in the nominal fixed setting, the software
metadata is not evidence of a working feature on this firmware.

## Reports

- [Installed HID writer](hid.md)
- [Legacy EC/WMI routes and assembly](ec-wmi.md)
- [Agent and LightingService chain](services.md)
- [Bundled OpenRGB dispatch](openrgb.md)
- [Windows configuration and registration](windows-config.md)
- [Independent APS protocol audit](protocol-audit.md)

Agents made no firmware calls, Windows edits, installations, or driver changes.
Their files are confined to their assigned research outputs. The coordinator's
one follow-up visual test used existing signed probes without polkit or reload.
