# Extended BIOS menu hotkey, V1.28

Read-only/offline investigation on 2026-09-16, following the owner's report that
holding Fn+Tab did not expose another menu. No new key sequence was tested on
hardware. No EC, EFI-variable, ACPI, driver, bootloader, or BIOS setting was written.

## Result and limits

Subsequent update: the owner reached the extended ACPI Settings page and provided
a V1.28 photo, but found Low Power S0 Idle Capability uneditable. The separate
grey-out restriction and its hardcoded initialization are documented in
S3-V128-RESEARCH.md. The photo confirms page access, not which key timing worked.

The supplied updater contains an embedded-controller implementation that maps
Tab into a tracked-key bitmap, tests that bitmap together with a vendor modifier
bit and three controller-state gates, and writes the value 9 to EC XRAM 0x0545.
The already-recovered BIOS menu-selection code tests that same EC byte for 9
and selects the extended menu containing the RC Advanced page.

The modifier uses internal key ID 0x8e and modifier mask 0x40. Its role is
consistent with Fn, distinct from the identified Shift, Ctrl and Alt mappings.
The physical legend-to-key-matrix mapping has not been independently verified.
Thus this is strong firmware evidence for the Fn+Tab candidate, not a discovery
of a proven alternative combination, and not proof that the user's failed
attempt reached the necessary controller state.

The exact meanings/timing of the three state gates remain unresolved. Do not
turn the disassembly into an invented number of seconds or a request to retry
the same failed shortcut. No live gate values were sampled.

## Source and EC identity

Same ZIP and capsule hashes as S3-V128-RESEARCH.md. Source ZIP remains unchanged.
The BIOS payload begins at capsule offset 0x1e6b50, following its IFLASH marker
and size fields. An 8051 reset/vector block begins `02 01 00 02 28 00`.

The EC code contains these identity strings:

- EC-relative 0x1000: `ITE EC-V14.0`, a generic controller-code string.
- EC-relative 0x6f54: `ZBJ-KQ  $1.08 $2025/12/29$01$`, the project version.
- Current `/sys/class/dmi/id/ec_firmware_release`: `1.8`.

The project version matches the running machine's reported release. This is not
a byte-for-byte comparison against a live EC flash dump; none was taken.

`inspect_ec.py` creates local analysis views: common bytes 0x0000..0x7fff plus
a selected 32-KiB bank at logical CODE 0x8000..0xffff. For the relevant bank,
EC-relative 0x20000..0x27fff maps to CODE 0x8000..0xffff. Local calls and their
field-reader implementations agree with that mapping. These views are never
used as flash images.

## Tab to menu-request chain

1. The EC keyboard-event dispatcher at EC-relative 0x24c2b loads key byte 0x0766
   and uses the compiler's switch table through common routine 0x7964.
2. Table entry `cc 73 0d` maps key 0x0d to CODE 0xcc73. PS/2 set-2 make code
   0x0d is Tab. The table's other entries and keyboard output paths are consistent
   with set-2 handling.
3. CODE 0xcc73 selects XRAM 0x0768, then 0xccbe writes mask 0x20. The make/break
   path at 0xccd9 ORs that mask into XRAM 0x0128 for event 0 or removes it for
   event 1. This is a key-state bitmap, not an ASCII character.
4. CODE 0xce76 checks the bitmap and gates listed below. At 0xce97..0xce9c it
   writes `0x09` into EC XRAM 0x0545.
5. BIOS module A01ODMDxeDriver function RVA 0x2398 tests the corresponding host
   EC address 0xfe800545 against 9. It then supplies a GUID list containing both
   Acer Advanced and RC Advanced. See S3-V128-RESEARCH.md for the latter's menu.

The write is at capsule offset 0x20b9e7, EC-relative 0x24e97. Its exact bytes are:

```text
90 05 45 74 09 F0
MOV DPTR,#0x0545
MOV A,#0x09
MOVX @DPTR,A
```

## Conditions on that write

| Input | Required value | Reader |
| --- | --- | --- |
| XRAM 0x0128 tracked-key bitmap | Exactly 0x20 | CODE 0xce76 |
| Internal RAM 0xbc bit 6 | Set | CODE 0xd46b |
| XRAM 0x05d0 bit 6 | Clear | CODE 0xd413 |
| XRAM 0x0563 bit 5 | Clear | CODE 0xd42f |
| XRAM 0x1602 bit 3 | Clear | CODE 0xce90 |

The internal modifier translation table at common CODE 0x7471 maps key 0x8e
to mask 0x40 with handler kind 5. Kind 5 dispatches to common CODE 0x6193,
which updates the internal modifier bitmap at 0xbc on make/break. Adjacent
entries map the Shift, Alt and Ctrl modifier masks through their scan-code
output handlers. Calling the nonstandard 0x8e/0x40 entry Fn remains an inference
from this role, rather than a stored printable key name.

Equivalent predicate, leaving the unidentified bits unnamed:

```text
if tracked_keys == TAB_MASK
   and vendor_modifier_is_down
   and not guard_05d0_bit6
   and not guard_0563_bit5
   and not guard_1602_bit3:
    extended_menu_request = 9
```

Several other paths clear the request byte, including common/initial-view
CODE 0xbe97. No claim is made about which of those paths ran during the failed
attempt or which state gate prevented activation.

## Validation and remaining work

- Ghidra 8051 disassembly/decompilation: `ec-menu-branch.txt`.
- `simulate_menu_gate.py` interprets only the reviewed predicate and helper
  instructions against fake RAM. All 32 combinations of Tab presence, modifier,
  and three guard bits matched the predicate; only the required combination
  wrote 9. Unknown opcodes/targets fail closed. No actual EC code was executed
  by the host CPU and no live memory was accessed.
- x86 A01HotKeyDriver and Ps2KeyboardDxe were also extracted/decompiled. The
  direct menu-request writer found here is in EC code, not the generic UEFI
  text-input path. This does not exclude additional indirect/encoded paths.
- A literal search did not find direct 8051 `MOV DPTR,#0x0abb` or neighboring
  0x0aba/0x0abc accesses in the inspected EC region. The alternative BIOS gate
  at host address 0xfe800abb bit 0 therefore still has no identified producer;
  an indirect access would not be excluded by that search.
- Built-in versus external keyboard matters: the recovered branch is fed by
  the EC keyboard-event path. A USB keyboard's own Fn handling cannot be assumed
  to set that internal modifier. The user was asked which keyboard they used.

Next evidence needed is the physical modifier mapping and state-gate timing, or
a safe observation of the actual failed sequence. A static matching EC version
and code path do not establish why the live attempt failed.

Primary scan-code reference:
https://download.microsoft.com/download/1/6/1/161ba512-40e2-4cc9-843a-923143f3456c/translate.pdf
