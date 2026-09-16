# Mounted Windows evidence, 2026-09-12

All Windows files were read without editing them. The source volume is mounted
at `/mnt/Ashwin`. Ghidra analysis output is stored in this Linux workspace.

The SYSTEM hive's `Select/Current` is 1. The service key
`ControlSet001/Services/AcerLightingService/ImagePath` points to
`Windows/System32/DriverStore/FileRepository/predatorservice.inf_amd64_c634eb8e856fb962/AcerLightingService.exe`.
That package's INF reports `DriverVer = 08/13/2025,1.2.108.3`.

The installed `AcerECKeyboardController.dll` has embedded FileVersion 1.0.3.10
and SHA-256 `ec75748429c1ec079d149ca00203441b6dfd6d3035062fe07ea06d3b5c7b2f79`.
The supplied support ZIP has FileVersion 1.0.3.9 and SHA-256
`4ed2737706c550b12d381c455a4896b96f123b4855a64e652ded892894cdfa63`.
This comparison concerns the individual component, not freshness of the support
download or the rest of Windows. The installed DLL was separately decompiled
into `AcerECKeyboardController.dll.c` here, preserving the ZIP's earlier analysis.

`ProgramData/OEM/AcerLightingService/LightingProfile/LightingProfile.ini` identifies
the backend as `AcerHIDKeyboard Device`. Its saved Breathing section contains:

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

ActiveMode is 1, so this is a saved Breathing recipe, not proof Breathing was
active at Windows shutdown. The PredatorSense scenario configuration also has
random-color enabled, so the saved color alone does not prove fixed-color output.

Installed DLL functions `FUN_1800081a0`, `FUN_180008250`, `FUN_18000dc10`, and
`FUN_180012900` respectively map effect IDs, map directions, prepare the mode,
and serialize A4. Breathing value 1 maps to wire effect 4. Direction 1 maps to
wire 1. The RGB bytes of 0x03C7AE00 are 00 AE C7; the high color-type byte is not
copied into the three RGB fields by the inspected HID writer.

The signed Linux probe for these saved settings is
`a4 21 04 64 05 01 00 ae c7 0f 00`. Visual verification is required; accepted
HID transfer alone cannot establish whether it is fixed-color Breathing.

The user visually tested this exact saved Windows recipe on Linux and reported
that it still cycled RGB. Normal saved preferences were restored afterward.
Directions 0, 1, and 2 have now all been observed to cycle; direction is not a
verified fixed/cycling color selector. This does not prove the hardware can never
do fixed-color Breathing, only that the inspected and tested native path has not
produced it.

After the six-agent review, the coordinator repeated the saved recipe after
explicit A2 selection of 0x21 and A3 verification (`a3 21 01 09 04 fb 07 00 00`).
The user again reported RGB cycling. Unlike the earlier bare raw recipe, this
test established the target-selection prerequisite. See ../parallel-native/SUMMARY.md.
