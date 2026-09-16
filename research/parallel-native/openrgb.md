# Acer OpenRGB native dispatch on PHN16S-71

Scope: offline inspection of the user-supplied PredatorSense bundle and the read-only Windows volume. No hardware access was performed.

## Bottom line

The Acer build of `OpenRGB.exe` is a loader and OpenRGB-side adapter. It does not contain the PHN16S-71 keyboard transport. At runtime it resolves `RGBDevice.ini` beside its own executable, loads `AcerECKeyboardController.dll`, calls `CheckDeviceStatus`, and uses the returned concrete name to resolve two factory exports. For the saved device name `Acer HID Keyboard Device`, the concrete chain is:

```
OpenRGB.exe
  -> RGBDevice.ini entry EC\ACPI1
  -> AcerECKeyboardController.dll
  -> CheckDeviceStatus(...)
  -> returned name AcerHIDKeyboard
  -> CreateAcerHIDKeyboardControllerClassInstance(...)
  -> CreateRGBController_AcerHIDKeyboardClassInstance(...)
  -> RGBController_AcerHIDKeyboard::UpdateMode()
  -> AcerHIDKeyboardController::SetMode()
  -> descriptor-driven feature report, report ID 0xA4
```

This is a native firmware-effect path. OpenRGB's BREATHING mode is passed once through `UpdateMode` to the DLL's `SetMode`; the code does not animate breathing by repeatedly issuing colors.

## Proven runtime selection

`OpenRGB.exe` function `FUN_140075000` at VA `0x140075000` gets its own image path with `GetModuleFileNameW`, removes the filename, and reads `<image-dir>\RGBDevice.ini`. It iterates `DeviceHWID1..Count`.

The bundle and installed `RGBDevice.ini` are byte-identical, SHA-256 `e05913a279aa9604369b4ec4d886b2caf3c89df0c05b26f809573face6a1fdb0`. Their relevant entries are:

```
DeviceHWID4=EC\ACPI1
[EC\ACPI1]
DeviceName=AcerECKeyboardController
```

For non-USB/non-HID INI keys, `FUN_140075000` appends `.dll`, calls `LoadLibraryExW(name, NULL, 0xA00)`, resolves `CheckDeviceStatus`, then iterates the names returned by that function. `FUN_140076310` at VA `0x140076310` constructs and resolves:

```
Create%sControllerClassInstance
CreateRGBController_%sClassInstance
```

It calls the native controller factory first. Only if that succeeds does it call the RGBController factory with the native object. Missing exports and a zero native-factory result are terminal for that returned controller name.

The ZIP DLL exports the HID factories at VA `0x1800090f0` and `0x180009150`. The installed DLL exports them at VA `0x180009190` and `0x1800091f0`. Both export the same seven names, including `CheckDeviceStatus`, the EC Global and EC Keyboard pairs, and the Acer HID Keyboard pair.

`CheckDeviceStatus` in the ZIP DLL is VA `0x1800085a0`; in the installed DLL it is VA `0x180008550`. The routine first enumerates its lighting HID interface. On the HID-success branch it returns:

```
device type = 1
device name = AcerHIDKeyboard
HWID = USB\VID_xxxx&PID_xxxx
usage page = four hexadecimal digits
```

On the non-HID branch it inspects SMBIOS type 172 and can instead return `AcerECKeyboard`, or `AcerECGlobal,AcerECKeyboard`. The registry value `HKLM\SOFTWARE\OEM\AcerLightingService\HybridMode` affects whether an HID controller is appended to the EC result. Therefore `AcerECKeyboardController.dll` is a multi-backend module; its filename does not imply that the active keyboard transport is EC/WMI.

The saved profile's device name `Acer HID Keyboard Device` is created only by `RGBController_AcerHIDKeyboard`, so it is strong evidence that the Windows run selected the HID branch and the `AcerHIDKeyboard` factories. It is not proof of the exact DLL file that was loaded during that run, because the log/process module list was not captured.

## OpenRGB mode translation and call order

The ZIP `RGBController_AcerHIDKeyboard` constructor starts at `FUN_18000d2a0`, VA `0x18000d2a0`. It advertises BREATHING only when native capability bit 3 is set. The descriptor has OpenRGB mode value `1`, capability flags `0x51` (decimal 81), one color, and the fixed/native-effect property. Thus profile `value=1 flags=81 color_mode=2` is metadata, not a literal wire tuple.

The relevant RGBController callbacks are:

- `FUN_18000efd0`, VA `0x18000efd0`: `UpdateLEDs`. It forwards the color array to the native controller bulk-color method.
- `FUN_18000f010`, VA `0x18000f010`: per-LED update. It calls native `SetMode` with direct mode 2 and a one-zone mask.
- `FUN_18000f0a0`, VA `0x18000f0a0`: `UpdateMode`. This is the path used for BREATHING. It transforms the OpenRGB mode through native virtual method 0, transforms direction through virtual method 1, takes one RGB value only when `color_mode == 2`, computes the zone mask from the detected zone count, then makes one native `SetMode` call.

The OpenRGB-to-native mode mapping at ZIP VA `0x180008200` includes:

```
OpenRGB 0 -> native 2
OpenRGB 1 -> native 4
OpenRGB 2 -> native 5
OpenRGB 3 -> native 7
...
OpenRGB 15 -> native 1
OpenRGB 34 -> native 6
```

The direction mapping at ZIP VA `0x1800082b0` includes `0 -> 2`, `1 -> 1`, `2 -> 3`, through `5 -> 6`. Consequently the saved BREATHING value 1 and direction 1 become native mode 4 and native direction 1. Speed is forwarded as stored. `flags=81` is not sent as a byte. This explains why treating every saved profile field as wire data is unsafe.

`AcerHIDKeyboardController::SetMode` is the ZIP function `FUN_1800083c0`, VA `0x1800083c0`, and the installed function `FUN_180008370`, VA `0x180008370`. Each forwards nine values to its HID report builder. The builder is ZIP VA `0x180013d00`, installed VA `0x180012900`.

The report builder does not use a fully hard-coded layout. It queries the enumerated device's report metadata for report `0xA4` and field tags `0x41`, `0x43`, `0x42`, `0x44`, `0x45`, `0x46`, `0x47`, and `0x48`; it copies each field using the width reported by that device metadata, caps the write to the feature report byte length, then submits the result. The semantic source order is device index, mode, brightness, speed, direction, R, G, B, zone mask.

For the actual ENEK5130 descriptor, the confirmed feature report length is 11 bytes including report ID. The vendor's metadata-driven construction therefore resolves to the same 11-byte A4 payload length used by APS. Dynamic sizing is an architectural difference from a hard-coded sender, but it is not a remaining explanation for the failure on this exact device.

The builder retries handle acquisition/submission up to five times, closes a failed handle, waits 20 ms, and reopens before retrying. A one-shot raw sender does not reproduce this behavior.

## Which executable and DLL can actually run

The INF installs all service executables, `OpenRGB.exe`, controller DLLs, `RGBDevice.ini`, and dependencies to destination directory ID 13, the DriverStore package directory. `AcerLightingService` is an auto-start service whose binary is `%13%\AcerLightingService.exe`. This supports the current DriverStore sibling set as the intended runtime set.

Only DriverStore copies of these files were found on the mounted Windows volume. There are two package revisions:

- `...predatorservice.inf_amd64_66c908d5540467e0`, dated 2025-04-02
- `...predatorservice.inf_amd64_c634eb8e856fb962`, dated 2025-08-13

The newer package's `OpenRGB.exe` is 4,284,768 bytes and SHA-256 `ab23457e632537897f93bb7f64d4144b8564ffd5c7a2e8766c3266066f9a52c2`. The extracted ZIP executable has the same size but different hash, `64d0d193a84049aec3a7684210564ece49afef381eae54e899b5c6cd639349fb`. A byte comparison shows most of the difference is near the signed tail, but equality was not proven and its code should not be assumed identical without a code-section comparison.

The DLL difference is definitely substantive. ZIP `AcerECKeyboardController.dll` is 166,160 bytes, SHA-256 `4ed273...cdfa63`; installed is 170,792 bytes, SHA-256 `ec7574...b2f79`, and several function addresses and internals differ. The installed DLL's existing decompilation must be used for an exact runtime clone.

The DLL load uses a bare filename plus `LoadLibraryExW` flags `0xA00` for the INI path. Those flags constrain dependency and application-directory search, but the module itself still resolves relative to the process application directory. Since the INF places the EXE, INI, and DLL together, the expected pairing is the controller DLL adjacent to the launched `OpenRGB.exe`. The exact launched image path remains suspected until Windows service logs or a process module capture establishes it.

## Exclusions and next actions

- Do not pursue the generic OpenRGB plugin folder mechanism for this keyboard. Acer controller DLLs use a separate vendor loader at `0x140075000`/`0x140076310`, not Qt's `QPluginLoader` and `com.OpenRGBPluginInterface`.
- Do not pursue the internal upstream-style OpenRGB device controllers for this saved device. `Acer HID Keyboard Device` originates in the vendor DLL adapter.
- Do not interpret profile `flags=81` as HID byte `0x81`; it is the mode capability field `0x51`.
- A direct native-equivalent test should reproduce the installed DLL's actual sequence: enumerate the same lighting collection, query its `0xA4` field metadata, select the same device index, map profile mode 1 to native 4, map direction 1 to native 1, send color only for color mode 2, compute the detected-zone mask, and retry/reopen like the DLL. The confirmed 11-byte report length already matches APS and does not need another sizing hypothesis.
- No successful Windows fixed-color BREATHING application has been established in the available evidence. The Windows material proves historical device registration and saved configuration only. Do not label the saved profile as proof that this exact mode/color combination ever worked on the current firmware.
- The earlier exact direction-1 recipe was sent through the RAW route after a restore, not through the typed lighting transaction. That restore may have left A2 at `0x65`. A controlled typed-path test that establishes the required A2 state before A4 is therefore more useful than another raw replay of the same 11 bytes.
- The highest-value Windows evidence is an AcerLightingService/OpenRGB log covering detection and an attempted apply. Its built-in messages expose `CheckDeviceStatus`, returned concrete names, loaded factory names, and the constructed report prefix. A process image/module-path capture would settle which DriverStore revision loaded. Only call an apply "known-good" if keyboard behavior confirms the requested fixed-color breathing effect.
- If Acer's native software can produce fixed-color breathing now, capture A2 and A4 traffic together and compare the device-index byte, A2 prerequisite state, callback ordering, and retry behavior with the typed APS path. Payload length is already reconciled.

## Source pointers

- `research/decompiled/OpenRGB.exe.c`: `FUN_140075000` around lines 76134-76870; `FUN_140076310` around lines 76876-77140.
- `research/decompiled/AcerECKeyboardController.dll.c`: `CheckDeviceStatus` lines 6321-6760; factories lines 6835-6927; mappings lines 6037-6090; HID adapter constructor lines 10045-10380; callbacks lines 11376-11480; report builder lines 15040-15470.
- `research/windows-installed/AcerECKeyboardController.dll.c`: installed `SetMode` around lines 6050-6082; `CheckDeviceStatus` around lines 6200-6500; factories around lines 6827-6910; report builder at line 13506 onward.
- `extracted/PredatorSense_5.1.514/AgentService/driver/RGBDevice.ini` and the newer DriverStore copy.
- `/mnt/Ashwin/Windows/System32/DriverStore/FileRepository/predatorservice.inf_amd64_c634eb8e856fb962/PredatorService.inf`, especially service installation, `ExeCopy`, and `DestinationDirs`.
