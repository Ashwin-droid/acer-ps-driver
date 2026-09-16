# Parallel native-path investigation

Completed: see [SUMMARY.md](SUMMARY.md) for reconciled findings and the subsequent
target-selection test result. The six agent reports retain their review-time
recommendations; the summary supersedes any test marked untested there.

Requested by the user: multiple Sol agents, offline inspection before asking for
a Windows session. Windows source files remain read-only. No agent may submit
firmware probes or change the installed driver.

## Work manifest

Seven existing decompiled/firmware inputs total 704,998 lines. Six Sol agents
own separate reports and optional scratch directories here:

| Report | Scope |
| --- | --- |
| hid.md | Installed keyboard DLL HID writer and mapping, compared with ZIP |
| ec-wmi.md | Legacy EC/WMI keyboard and global native routes |
| services.md | Agent/Lighting service color-type, flags, ordering and initialization |
| openrgb.md | Vendor-bundled OpenRGB plugin loading and native dispatch |
| windows-config.md | Active Windows versions, profiles, registration and logs |
| protocol-audit.md | Independent APS/Windows/HID descriptor comparison |

The coordinator owns this index and the final reconciled report. Findings must
identify code addresses or source lines, distinguish possible paths from this
machine's actual path, and separate accepted transfers from observed effects.

## Established observations to avoid repeating

- Exact model: PHN16S-71, FTYPE_ARX, BIOS V1.28; keyboard ENEK5130 0CF2:5130.
- Four keyboard zones, capability mask 0x000007fb; HID target 0x21.
- Native Breathing effect 4 with explicit RGB and direction 0, 1 or 2 cycled RGB.
- Effect 3 left lighting steady/unchanged according to the user.
- Setting Static blue first, then Breathing with blue, still cycled RGB.
- Exact stored Windows recipe `a4 21 04 64 05 01 00 ae c7 0f 00` also cycled RGB.
- Last physical test was restored to the user's saved static zone colors; dev
  session ended at sequence 16 before this investigation.
- No fixed-color software animation was added; user wants native investigation.

The active Windows service path, component hashes/versions, and saved recipe
are documented in ../windows-installed/NOTES.md. The mounted configuration is
historical evidence, not proof that a recipe worked or that the package is latest.

No findings in an agent report should be treated as a verified fix without
coordinator review and, where behavior is uncertain, a controlled visual test.
