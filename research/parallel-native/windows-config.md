# Installed Windows lighting configuration and provenance

Date: 2026-09-12

Scope: offline, read-only inspection of the Windows volume mounted at
`/mnt/Ashwin`. No Windows hive or Windows file was written. This note does not
claim that any saved setting was successfully rendered by the keyboard.

## Result

The installed native route is an Acer service plus an Acer-modified OpenRGB
server and dynamically selected Acer controller DLLs. On this machine the
saved service profile names `AcerHIDKeyboard Device`, and the OEM registry
selects a four-area HID keyboard. I found no second saved key, hidden profile,
or log entry that changes Breathing from cycling colors to a fixed color.

The strongest native fixed-color candidate remains the already-tested saved
Breathing record in `LightingProfile.ini`. Its `color_mode=2` and one RGB color
do not prove fixed-color output. The exact resulting HID recipe continued to
cycle colors in the user's Linux visual test. The PredatorSense scenario file
also records `random=true`, which weakens any inference that the single saved
color means "fixed".

## Installed package and active service

The offline SYSTEM hive reports `Select\Current=1`. Under
`ControlSet001\Services\AcerLightingService`, the service is automatic
(`Start=2`), runs as `LocalSystem`, and its `ImagePath` is:

```text
%SystemRoot%\System32\DriverStore\FileRepository\predatorservice.inf_amd64_c634eb8e856fb962\AcerLightingService.exe
```

The same package owns `AASSvc`, whose image is `AcerCentralService.exe` in that
directory. The active package's INF declares version `1.2.108.3` dated
2025-08-13 at
`.../predatorservice.inf_amd64_c634eb8e856fb962/PredatorService.inf:4-10`.
It installs both services at INF lines 30-57 and copies the lighting service,
OpenRGB server, controller DLLs, and `RGBDevice.ini` at lines 64-90.

An older package remains in the Driver Store:

| package | INF version | date | service image size | keyboard DLL size |
| --- | --- | --- | ---: | ---: |
| `predatorservice.inf_amd64_66c908d5540467e0` | `1.2.107.12` | 2025-03-26 | 245,040 | 166,168 |
| `predatorservice.inf_amd64_c634eb8e856fb962` | `1.2.108.3` | 2025-08-13 | 261,936 | 170,792 |

Evidence: the old and active INF version lines are respectively
`.../66c908d5540467e0/PredatorService.inf:4-10` and
`.../c634eb8e856fb962/PredatorService.inf:4-10`. The adjacent Driver Store
metadata records the old component sizes and 2025-04 timestamps at
`.../predatorservice.inf_amd64_66c908d5540467e0.ini:5-16`, and the active
component sizes and 2025-08 timestamps at
`.../predatorservice.inf_amd64_c634eb8e856fb962.ini:5-16`.

Both packages contain byte-identical `RGBDevice.ini` and byte-identical
`Predator PHN16-71.ini`; their SHA-1 values and sizes match in the two package
metadata files at lines 56 and 68. The PHN16-71 file contains GPU overclock
limits only. It has no lighting capability keys. The installed keyboard DLL is
FileVersion `1.0.3.10`, versus `1.0.3.9` in the supplied support ZIP, as already
recorded in `research/windows-installed/NOTES.md`. Thus any protocol inference
from the ZIP should be checked against the installed DLL.

## How controller plugins are selected

The active `AcerLightingService.exe` contains these paths and operations as
UTF-16 strings:

```text
C:\ProgramData\OEM\AcerLightingService\LightingProfile\LightingProfile.ini
OpenRGB.exe --server
service_common_function::startupOpenRGB
OpenRGBClient::UpdateRGBdevices
```

It imports Windows HID and SetupAPI directly but does not statically import an
Acer controller DLL. The bundled `OpenRGB.exe` contains
`RegisterAcerRGBDevice`, `%s\RGBDevice.ini`, `DeviceName`, and `%ws.dll`. This
is evidence of runtime DLL selection from `RGBDevice.ini`, not COM registration
or a registry plugin list.

`RGBDevice.ini:1-7` declares 40 hardware IDs, including virtual `EC\ACPI1` and
`EC\ACPI2`. Lines 49-61 map those and Acer USB IDs to controller base names.
The corresponding DLL filenames are formed at runtime. In particular,
`EC\ACPI1` maps to `AcerECKeyboardController`, whose package DLL exports both
`CreateAcerECKeyboardControllerClassInstance` and
`CreateRGBController_AcerECKeyboardClassInstance`.

The latest retained bundled OpenRGB log confirms the result on this machine:

* `OpenRGB_20260908_150655.log:1-3` identifies Acer's bundled OpenRGB 0.7 build.
* Lines 9-12 register `AcerHIDKeyboard Device`, `AcerHIDModeKeyLED Device`, and
  `AcerHIDCoverLogoLED Device`, then initialize HID successfully.
* Lines 16-18 show unrelated I2C WMI detection failures. They do not invalidate
  the successful Acer HID registrations.

This proves controller discovery and registration. It does not record effect
packets or prove what colors a Breathing command displayed.

## Model capability configuration

The installed PredatorSense app is version `5.1.468`, from the offline
uninstall registry and the package path
`Program Files/WindowsApps/ULICTekInc.PredatorSenseforNotebook_5.1.468.0_x64__nt9dgb7efx6bt`.
Its model file `app/win32/Predator/Predator PHN16S-71.json:16-23` enables
lighting and selects:

```json
"Lighting": {
  "keyboard": true,
  "keyboard_device": "Helios_EQE",
  "eclogo": true
}
```

The offline SOFTWARE hive adds the machine-specific values:

```text
HKLM\SOFTWARE\OEM\AcerAgentService
  Model_Name_1st = Predator PHN16S-71
  EC_HID_Support = 1
  EC_HID_Version = 0x600
  EC_HID_OC_Capability = 5

HKLM\SOFTWARE\OEM\AcerAgentService\LightSetting
  KeyBoardArea = 4
  KeyBoardColor = 2
  KeyBoardHID = 1
```

These values select a four-area HID keyboard and record Acer's color-mode code
as `2` in the UI and service configuration. They do not define packet bytes or establish that
`KeyBoardColor=2` means fixed-color Breathing. No
`ControlSet001\Services\mssmbios\Data` key exists in this SYSTEM hive, so this
offline route could not confirm the SMBIOS type-172 capability tags found by
the separate EC/WMI analysis.

## Saved profile and alternate settings

`ProgramData/OEM/AcerLightingService/LightingProfile/LightingProfile.ini:1-18`
lists the three successfully registered HID devices and ten keyboard modes.
The keyboard Breathing record is at lines 49-63:

```ini
value=1
flags=81
brightness=100
colors_max=1
colors_min=1
color_mode=2
color=0X3c7ae00
direction=1
speed=5
```

For comparison, STATIC uses `flags=80`, `color_mode=2`, and one color at lines
34-48. SHIFTING, ZOOM, METEOR, and TWINKLING also use `color_mode=2` at lines
109-168. Therefore `color_mode=2` means that the mode accepts a saved color;
it is not, by itself, a fixed-versus-random selector.

The same INI has separate Breathing records for the mode-key LED at lines
211-225 and the cover-logo device later in the file. Those are different LED
devices, not alternate keyboard encodings.

The PredatorSense scenario pool provides conflicting state at
`ProgramData/OEM/PredatorSense/ProfilePool/config.json`. The Gaming scenario's
global Breathing entry is at lines 212-246 and includes one color plus
`random=true`. Its keyboard device entry is STATIC at lines 248-299. Other
scenario entries likewise store `random=true`. This JSON is UI/scenario state;
it is not a capture of the service's hardware write.

I searched the installed Acer/OEM ProgramData trees and relevant app/package
directories for other lighting INI, JSON, config, profile, and log files. No
second keyboard lighting profile or packet history was present. The service and
controller binaries contain a logging path under
`ProgramData\OEM\AcerLightingService\Log`, but that directory is absent and
the offline SOFTWARE hive has no `OEM\AcerLightingService\Log` key. Retained
OpenRGB logs only record device registration.

## What remains worth capturing in Windows

No additional offline artifact contains a known fixed-color-Breathing recipe.
The useful next evidence must come from a live Windows session:

1. Before changing the UI, copy the current lighting INI and scenario JSON for
   comparison. Then set Breathing, turn the UI's random-color control off if
   one is exposed, choose a conspicuous single color, and apply it.
2. Capture the service-to-HID traffic or instrument the installed
   `1.0.3.10` controller DLL while applying that change. A successful visual
   result plus its exact report sequence would distinguish a missing setup
   command from a wrong A4 payload field.
3. Re-read both native profile files after the successful apply. A changed
   field is useful only when paired with the visually verified packet trace.

Merely booting Windows and observing the current profile will not answer the
question. The currently saved recipe has already been reproduced exactly and
visually failed to hold one color.
