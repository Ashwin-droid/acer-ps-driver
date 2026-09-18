# Boot animation master and independent mute

Read-only investigation on 2026-09-16. No firmware setting was changed.

## Inputs and limits

- Running machine: PHN16S-71 / FTYPE_ARX / BIOS V1.28.
- Windows Downloads contained BIOS_Acer_1.24_A_A.zip, not V1.28.
- Examined Windows/Firmware/BIOS_V1.26.fd, SHA256
  `5ed20cd688e448aac0152aea8a6d2b0d320186ab0552f0455d5a468398e0adba`.
- Its bytes contain the current ESRT firmware class
  `41434552-648c-4365-bafd-eb869f001871` twice. It is older firmware for the
  matching firmware class, not a dump of the running V1.28 implementation.
- Standard LZMA decompression at file offset 0x3fbc00 produces a bounded
  39,165,952-byte firmware volume. Only offline parsing/decompilation was used.

## BIOS form records

English HII string package: volume offset 0x13e1f18, length 33154.
Forms package: 0x13da368, length 2792; parsed instruction boundaries end cleanly.
Varstore ID 0x1234, name SystemConfig, GUID a04a27f4-df00-4d42-b552-39511302113d,
size 0x4b0. Offsets below are V1.26 research data, not authorization to write
the running machine's variable at those offsets.

| Prompt | Question | Field offset | Values |
| --- | --- | --- | --- |
| POST Animation & Sound | 0x0f | 0x100 | 0 disabled / 1 enabled |
| Customize POST animation | 0x10 | 0x11e | 0 disabled / 1 enabled |
| Sound | 0x11 | 0x122 | 0 mute / 1 unmute |

Sound help text is `Mute/Ummute Sound of POST Animation.` Its GrayOutIf expression
includes question 0x0f == 0, OR other question conditions. Therefore the BIOS UI
does not offer an independent sound-only mode with the master disabled. The
normal effective combinations are off, animation-only, and animation+sound.
This is form/UI evidence; no physical reboot behavior was tested in this run.

## PredatorSense and firmware handler

- Both supplied and installed AcerAgentService versions contain BOOT_SOUND and
  CUSTOM_BOOT_LOGO, not a second boot-animation RPC. SOUND_MODE is DTS audio and
  must not be repurposed for startup audio.
- Agent getter `FUN_14003c340` uses misc selector 6; setter `FUN_14003d830`
  sends 0x06 or 0x106. The existing WMBH methods are 0x17/get and 0x16/set.
- Extracted/decompiled A01WMISmmCallback and A01ODMSmmServiceDriver with Ghidra.
- WMI native handler at RVA 0xc7b8 validates selector 6 values 0, 1 or 0xff.
  The extra 0xff value is not a mute-mode discovery: ODM `FUN_00002e38` sets
  EC address 0xfe800a98 bit 5 only for value 1 and clears it otherwise.
- WMI wrapper RVAs 0xc8b6..0xc8e8 copy that EC flag to cached SystemConfig
  address 0x10be0, exactly cache base 0x10ae0 + master offset 0x100. Its success
  path marks the existing configuration update dirty. It does not update the
  sound field at cache base + 0x122. No raw EC access was performed on hardware.
- The getter `FUN_0000300c` returns that same EC bit. Thus `boot_effects=on`
  must not be printed as evidence that sound is unmuted.

## Read-only live corroboration

Read only Setup-a04a27f4-df00-4d42-b552-39511302113d through efivarfs.
Payload length is 1200 bytes, matching the V1.26 form varstore. Candidate fields:
master=1, custom-logo=1, sound=0. Current WMI selector-6 readback is on.
This is consistent with animation enabled and sound muted. Field meaning is
derived from V1.26, so it is not by itself a verified V1.28 write contract.

## Implementation boundary

Corrected APS status/help to label the existing command as the effects master.
No sound setter, raw Setup-variable writer, direct EC accessor, or new firmware
selector was added. Independent sound control needs a verified access path;
the existing PS command cannot be split into two independent writes by renaming it.

Follow-up: the live Setup variable is marked immutable in efivarfs. The
AcerBIOSConfigurationToolSmm module was also decompiled; no verified narrow
sound-mute write method was established. No immutable flag was cleared and
no raw Setup-variable writer was introduced. Custom-logo installation now
auto-enables the already verified master switch, with file rollback on failure.

Extraction is reproducible with scripts/extract-boot-control-research.py.
The extracted PE modules and Ghidra exports are under research/boot-control-v126/.
