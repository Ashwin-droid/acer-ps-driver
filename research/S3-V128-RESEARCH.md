# S3 visibility in PHN16S-71 BIOS V1.28

Read-only investigation, 2026-09-16. No updater was executed, firmware flashed,
EFI variable changed, ACPI table overridden, or suspend triggered.

## Finding

The exact V1.28 image contains an ACPI S3 Support setup question, a Low Power S0
Idle Capability question, and executable code that hides S3 from the ACPI namespace
when Low Power S0 Idle is enabled. The current machine has both stored options
set to 1, but the firmware publishes SS3=0 and the OS sees no S3 state.

This establishes a real firmware configuration gate. It does not establish that
turning it off produces reliable S3 entry/resume on this CPU, board, GPU, or EC.
The presence of generic S3 support modules is not proof of a working resume path.

## Inputs

- Model: Predator PHN16S-71, board FTYPE_ARX, running BIOS V1.28.
- Supplied archive: `/home/ashwinmhaskar/Downloads/BIOS_Acer_1.28_A_A.zip`.
- Archive SHA256: `2279aa4b5f1741ff057401a8c922505bb53d6a28024cc492d1fa18cf25c1976c`.
- Inner installer: `ZBJK_V1_28.exe`. Read as data only.
- Inner firmware: `ZBJK.fd`, 25,137,200 bytes, SHA256
  `29cd0792e0c01d1800b078109aa2703d8bc31d1a569aa8000fa92d47ac29a85c`.
- Bounded LZMA stream at capsule offset 0x3fbc00 expands to 39,165,952 bytes,
  SHA256 `0f28fd15ac4e6374d40f277c285c198c56a7264b2303d08c4b0f76f213f78399`.
- Local evidence and extraction scripts: `research/s3-v128.FSptaf/`.
- Ghidra project: `research/ghidra-projects/S3V128`.

The SFX has `Protect=yes`: each byte of its 7z payload is rotated left by one bit.
`unpack_archive.py` reverses that rotation into a new local archive and verifies
both the start-header and next-header CRCs. 7z extracts the resulting archive;
no Windows or EFI executable is launched. The source ZIP and EXE are unchanged.

## Setup forms

Parsed HII English string package at volume offset 0x1308a98, size 0x7a5f8,
containing 6,865 strings. The RC forms package at 0x1392778 has size 0x44d8c.
IFR instruction lengths and nested scope boundaries validate through its end.
These RC forms use the RC strings earlier in the same SetupUtility PE, not the
separate Acer language package that follows the forms. See `hii-strings.json`
and `ifr.txt`. Other package/string associations in the exploratory dump are
heuristic; this report uses only the identified RC package.

Form 0x100a is ACPI Settings. Varstore ID 1 is `Setup`, size 0xc2d, GUID
`ec87d643-eba4-4bb5-a1e5-3f3e36b20da9`.

| Setting | Question ID | Offset within variable payload | Current value |
| --- | --- | --- | --- |
| Enable ACPI Auto Configuration | 0x58 | 0x0d | 0 |
| ACPI S3 Support | 0x5c | 0x0e | 1 |
| Enable Hibernation | 0x59 | 0x0f | 1 |
| Low Power S0 Idle Capability | 0x111d | 0x1d | 1 |

The current values were read from efivarfs without modification. The file is
3,121 bytes: four EFI attribute bytes followed by the 3,117-byte payload, matching
the form's 0xc2d size. The attribute word is 7. It is root-owned and immutable.
Offsets in this report are research evidence, not a tested write procedure.

S3's one-of question starts at volume 0x1394004:

```text
05 91 0b00 0f00 5c00 0100 0e00 10 10 00 01 00
```

Its choices are 0/Disabled and 1/Enabled. It is under two SuppressIf scopes:

- question 0x58 equals 1, ACPI auto-configuration enabled;
- question 0x111d equals 1, Low Power S0 Idle enabled.

The Low Power S0 Idle question starts at 0x1394196. Its choices are also 0/1.
It has additional platform-dependent visibility/enabling conditions, including
question 0x1cda tied to Acer SystemConfig offset 0x79, and question 0x1cd9 tied to
SetupVolatileData offset 0x79. The Acer field currently reads 2. No claim is made
that these forms are reachable through the normal shipping setup UI.

## Executable ACPI gate

Module `AdvancedAcpiDxe`, FFS GUID `c3e69eb2-0429-4bd6-ae4a-8ca02fbacc2e`.
FFS begins at decompressed-volume offset 0x18e9a80; PE begins at 0x18e9b54.
PE image base is zero; relevant PE offsets and RVAs coincide.

The code around RVA 0x2817 loads `Setup` with the Intel GUID above, size 0xc2d,
into module-local storage at RVA 0xfb20. The GUID bytes are at 0xf4d0 and the
UTF-16 variable name at 0xf8b8. Thus 0xfb2e is Setup[0x0e], and 0xfb3d is
Setup[0x1d].

The ACPI-table callback at RVA 0xa38 dispatches DSDT handling to 0x130a.
On that path r13 remains zero, initialized at 0xa81. At 0x19f6 the code matches
the AML NameSeg `SS3_`, integer 0x5f335353. At 0x1a21 it checks the preceding
NameOp byte 0x08. Then:

- 0x1a2a compares Setup[0x1d] against zero;
- 0x1a31 initializes the replacement value to zero;
- 0x1a34 loads Setup[0x0e];
- 0x1a3b conditionally selects that value only if Setup[0x1d] was zero;
- 0x1a3e writes the result into the byte after the four-byte SS3_ name.

Equivalent reconstructed logic:

```text
SS3 = low_power_s0_idle ? 0 : acpi_s3_support
```

Ghidra initially missed this callback's function boundary. Explicitly creating
the analysis function at 0xa38 produced `s3-table-patcher.c`; lines 411-417
independently show the same conditional assignment. The decompiler warns about
type propagation, so the relevant branch was also checked against x86-64
instructions and the actual Setup buffer-loading call. This only changes the
local analysis database, never the source firmware image.

The same callback's FADT branch reads the Low Power S0 Idle field at RVA 0x1b6c
and sets the low-power-idle flag in the FADT when it is nonzero. This is more than
unused menu text: the setting participates in the ACPI tables delivered to the OS.
It does not establish what happens after changing the setting or whether later
firmware policy can override it.

## Running-system corroboration

Fresh copies of only DSDT and FACP were taken from `/sys/firmware/acpi/tables/`
using a read-only source operation. No OEM product-key table or memory was read.

- DSDT SHA256: `3d9fdda833f4638f2748aa4db5c7d67e6d4f2727fafafd0160034bc89389e529`.
- FACP SHA256: `11684900f629b66d8e187d7c212808d0ed18bb305848d56743b8a8fc85eb1b21`.
- iASL reports Low Power S0 Idle = 1, Hardware Reduced = 0 in FACP.
- DSDT declares `Name (SS3, Zero)` and creates `_S3` only inside `If (SS3)`.
- Kernel boot log: `ACPI: PM: (supports S0 S4 S5)`.
- `/sys/power/mem_sleep`: `[s2idle]`.

DSDT disassembly has unresolved external-method warnings because this fresh
disassembly did not supply every SSDT. The SS3 NameOp and conditional _S3 Package
are directly encoded in the table and do not depend on inferred method argument
counts. No compilation or installation of the disassembly was attempted.

## Limits and next boundary

The image also contains named CpuS3DataDxe, S3SaveStateDxe, AcpiS3SaveDxe and
SmmS3SaveState components. Their existence alone cannot prove that the complete
board-specific resume path is wired up, supported by the silicon, or tested.

There is a concrete candidate configuration gate, but no validated switch
procedure or successful S3 experiment. Do not describe this as S3 unlocked or
safe to enable. Disabling modern standby without a working alternative could
lose usable suspend, and an incomplete resume path could hang or reset.

No sound-mode research or implementation was resumed. APS source, installed
driver, boot configuration, Secure Boot state, and sleep settings were untouched
during this investigation.

Linux's distinction between suspend variants and platform-supported S3 is
documented at https://docs.kernel.org/admin-guide/pm/sleep-states.html.

## Follow-up: setup callback and menu route

Further read-only analysis on the same date. The following route is encoded in
the RC form set, GUID `c6d4769e-7f48-4d2a-98e9-87adccf35ccc`:

```text
Advanced                         form 0x0001
  ACPI Table/Features Control    reference at volume 0x1392bff, target 0x0028
    ACPI Settings                reference at volume 0x1393435, target 0x100a
      Low Power S0 Idle Capability
      ACPI S3 Support            hidden while Low Power S0 Idle is enabled
```

This is the extended/second Advanced page. A separate Acer Advanced form set,
GUID `ee8697ed-904c-40d6-a4d2-cc0762ddfc76`, exists in the same image. Seeing an
Advanced heading in the regular UI does not establish that the RC page is visible.
The RC form set is included in SetupUtility's package-registration list at
RVA 0x4188, with the config-access callback registered through RVA 0x9c8c.

### Low Power S0 Idle callback

Question 0x111d has question flags 0x14: CALLBACK plus RESET_REQUIRED.
SetupUtility's callback at RVA 0x7bd4 dispatches that question at RVA 0x8830
to function RVA 0x247e8. This was decompiled separately and checked against
instructions. It accepts browser actions 0/1 and otherwise returns unsupported.
The caller dispatch observed here supplies action 1.

The specific callback loads/copies the current 0xc2d-byte Setup browser state.
If Setup[0x1d] is 1, it sets Setup[0x85d] to 1, then copies the buffer back.
IFR question 0x1b9f at volume 0x13d5377 identifies offset 0x85d as
HID Event Filter Driver. The current EFI payload also has that field set to 1.
When Low Power S0 Idle is zero this callback leaves the HID field unchanged.
No additional disabling-side edit was found in this specific callback. This is
not a claim that later policy initialization or other callbacks cannot affect
the configuration. Browser-state changes are distinct from a direct EFI-variable
write and from successful S3 resume.

Evidence: `setup-sleep-callback.c`, `setup-menu-policy.c`, `setup-menu.c`.

### Extended-menu selection mechanism

The only other literal occurrence of the RC form-set GUID in the decompressed
image is in A01ODMDxeDriver, FFS GUID `05f7aa70-a64a-432c-8cee-4cdecb8671d7`.
Its PE starts at volume 0x1a23ab4. Function RVA 0x2398 overrides the setup-menu
list when one of its conditions is met, including an EC byte equal to 9 at
0xfe800545 or bit 0 set at 0xfe800abb. It clears that latter bit when consuming it.
Other conditions inspect a firmware version string through a service.

The selected GUID list at module RVA 0x62f0 contains Information, Main, Acer
Advanced, RC Advanced, Security, Power, Boot, Exit, and a null terminator. This
is evidence of a built-in extended-menu selection route in this exact image.
No live EC access was performed. The mapping from a physical Fn+Tab chord to
those EC fields was not independently recovered; interpreting the value 9 as
proof of that chord would overstate the evidence.

`SetupUtilityApp` also has menu-filtering code and platform hooks. Consequently
the static list is not a guarantee every page appears in a particular setup session.

### Physical entry steps and their evidence strength

Subsequent user test: Fn+Tab did not expose the menu. The steps below record the
earlier proposed inspection, not a verified procedure. Further binary tracing
is recorded in HOTKEY-V128-RESEARCH.md, including an EC-side request writer and
the still-unidentified state gates.

- The PHN16S-71 manual, printed/PDF page 77, documents F2 at the logo for BIOS,
  followed by F1 for Advanced Settings. The full relevant page was rendered and
  visually checked. Local copy: `PHN16S-71-manual.pdf`.
- An identified PHN16S-71 owner, Rnatio, reports Fn+Tab at boot followed by F2
  in Acer's community. This is first-person corroboration for the model, not an
  Acer-supported procedure or a test of V1.28 on this machine.
- A reasonable read-only attempt is to hold Fn+Tab on the built-in keyboard
  during power-on, release and tap F2, then F1 if the simple interface appears.
  Look for the second Advanced page and the hierarchy above. No supervisor
  password, Secure Boot change, downloaded unlocker, or firmware patch is needed
  merely to try this key sequence. Success is unverified.
- If the page is absent or the setting is greyed out, stop rather than guessing
  additional unlock settings. A photo can establish the next step. For an initial
  inspection, exit without saving. Changing the sleep mode remains a separate
  experiment with unverified S3 resume and a recovery requirement.

Primary/manual source:
https://global-download.acer.com/GDFiles/Document/User%20Manual/User%20Manual_Acer_1.0_A_A.pdf?BC=ACER&LC=en&OS=ALL&SC=PA_6&Step3=PREDATOR+PHN16S-71&acerid=638844404130331508

First-person owner report, Rnatio's comments identifying PHN16S-71 and giving the
key sequence:
https://community.acer.com/en/discussion/comment/1367554

UEFI question flag meanings:
https://uefi.org/specs/UEFI/2.10/33_Human_Interface_Infrastructure.html

## User reached ACPI Settings; control greyed out

The user supplied `/home/ashwinmhaskar/Downloads/PXL_20260916_155453533.jpg`,
showing PHN16S-71 BIOS V1.28 on the recovered ACPI Settings page with Low Power
S0 Idle Capability selected. They report that the setting cannot be disabled
because it is greyed out. This confirms access to the relevant form, but does
not by itself identify the precise key sequence or EC state that exposed it.

### Direct reason for the grey-out

At decompressed-volume 0x139418e the forms package has a GrayOutIf followed by
an equality test at 0x1394190:

```text
19 82                  GrayOutIf, scoped
12 06 d9 1c 00 00      EqIdVal(question 0x1cd9, value 0)
```

The next question is Low Power S0 Idle Capability, ID 0x111d. At 0x13d7382,
question 0x1cd9 is defined as a hidden byte at offset 0x79 in varstore 0x100b,
`SetupVolatileData`, GUID `ec87d643-eba4-4bb5-a1e5-3f3e36b20da9`, length 0x7f.
This is distinct from the saved Low Power S0 setting at Setup[0x1d], and from
the Acer SystemConfig[0x79] field controlling whether the question is visible.

### Producer of the disabling value

Extracted and decompiled `PlatformInitDxe`, FFS GUID
`ca84408a-0929-4f11-bfed-18c7d9576c6b`, PE at volume 0x16ec3b0.
Function RVA 0x2c6c reads SetupVolatileData and BoardInfoSetup. In its successful
initialization path it explicitly sets the relevant byte to zero:

```text
2c8f: lea rax,[rbp-0x49]    ; SetupVolatileData buffer base
...
2e43: mov byte [rbp+0x30],0 ; 0x30 - (-0x49) = offset 0x79
...
2e81: mov r8d,2            ; EFI_VARIABLE_BOOTSERVICE_ACCESS
2e8e: mov r9,rsi           ; size 0x7f
2e91: lea rcx,[SetupVolatileData name]
2e9b: lea rax,[rbp-0x49]
2e9f: mov [rsp+0x20],rax   ; payload pointer
2eaf: call [runtime_services+0x58] ; SetVariable
```

The actual store to offset 0x79 is a literal zero, not a CPUID/capability result
or a lookup of the screen's Safe Setting option. Other neighboring fields do
use platform services, but this field does not on the recovered path.
Ghidra represents it as `local_2f = 0` in FUN_00002c6c, relative to the buffer
starting at local_a8. The instruction-level buffer arithmetic confirms the offset.

SetupUtility reads the same 0x7f-byte variable into its buffer at RVA 0x1aa6f0
before exposing the form. The variable is published with boot-service-only
attributes, explaining why it is not normally visible via Linux efivarfs after
ExitBootServices. No live read of that buffer or raw memory was attempted.

Conclusion: the screenshot/user report matches a build-level UI editability
restriction explicitly initialized in V1.28. Opening the extended menu does
not remove it. No verified ordinary-menu prerequisite was found that unlocks
this control. Do not recommend changing Safe Setting, passwords, Secure Boot,
or unrelated ACPI settings as a guessed workaround.

This remains a UI-policy finding, not proof of working or impossible physical
S3 resume. A direct Setup-variable experiment would bypass this UI restriction,
but firmware may still reject or override it and S3 resume remains unverified.
No bypass or persistent write was implemented or performed.
