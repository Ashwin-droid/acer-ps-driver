# Callback-driven keys and lid, 2026-10-08

DKMS 0.5.1 removes the driver's dedicated kthread, its 250 ms pending-key check,
and the 500 ms ACPI lid polling schedule. Turbo still arrives through the exact
Acer HID report; PredatorSense still arrives through the built-in AT keyboard.
Their callbacks enqueue preallocated work instead of waiting for a periodic pass.
No undocumented WMI event-enabling method was called.

## Scheduling and lifetime

- Immediate work runs on `system_unbound_wq`, under the existing state mutex.
  HID/input callbacks do no sleeping firmware I/O. Pending key queues remain
  bounded at eight; work drains bounded passes and requeues only if presses remain.
- The input handler also matches the real ACPI Lid Switch device on BUS_HOST,
  with physical path `PNP0C0D/button/input0`. EV_SW/SW_LID queues an authoritative
  ACPI sample. Its callback returns false, preserving the event for logind and
  other listeners. Registration, resume and explicit gated writes also sample
  the lid. There is no event-value override of the real ACPI state.
- One preallocated delayed work item tracks the earliest permitted wake, lid
  retry, or Eco-logo deadline. No deadline means cancellation, not another poll.
  Open-lid and Eco work blocked by wake restoration cannot continuously requeue
  an overdue deadline. Retry limits remain unchanged.
- Suspend pauses event dispatch, discards pending keys, and cancels the deadline.
  POST clears stale presses and queues a fresh lid sample plus wake restoration.
- Unload sets STOPPING under the state mutex, drains PM/sysfs/input/HID producers,
  synchronously cancels delayed and immediate work, then frees virtual input and
  state. All work-object initialization uses the target kernel helpers, including
  debug-object setup, stable allocations and static lockdep keys.

This removes the driver's idle polling, not every userspace timer. The existing
account preference service still checks/restores/saves its state. The one-shot
RGB delays are also intentional. Workqueue scheduling has latency; this removes
the old artificial 250 ms dispatch wait, not all possible delay.

## Live findings

The first event-driven build used the per-CPU workqueue. Physical tests passed,
but performance-mode firmware calls triggered the kernel's workqueue CPU-hog
warning. The final build uses the shared unbound workqueue, as recommended for
such work. No private workqueue thread was substituted for the polling thread.

Physical tests captured F14 and F15 on the dedicated APS virtual input device.
The test temporarily selected Hotkey and restored Power modes afterward. Further
user Turbo presses exercised real performance changes and Eco's delayed logo
reapplication. The user left the machine in Eco; that choice was preserved.

An ACPI input listener observed close then open SW_LID events while APS's cached
lid state followed both, with error=0 and unchanged lighting preferences. This
also verified that the driver's filter did not consume the normal lid events.

After loading the final unbound build and allowing restoration to settle,
`work_status` stayed at `runs=3 timer_runs=2` for ten idle seconds. The old
`[acer-phn16s]` task was absent. Production dev_mode remained 0. Malformed sysfs
requests and closed-lid write rejection passed against the final loaded module.

## Installed-binary verification

The first attempt to install 0.5.1 exposed a DKMS issue: it marked the version
installed but retained the previous binary because both modules had the same
srcversion. The installer now uses an explicit replacement after its signing
checks and verifies exact installed/build byte equality. It also fixes inherited
group-writable intermediate snapshot directories via a safe installer umask.
Only the two affected root-owned directories in this new snapshot were repaired;
no broad permission reset was performed.

The final installed and DKMS-built compressed module SHA256 is
`d0a8bc809e5f2b2bc6b68b791bb7079959033ac6a00c7a8d95c77454ce6d0804`.
Its ELF build ID `897537298144740bfbb486450cabc99e6928659d` matched the live
`/sys/module/acer_phn16s/notes/.note.gnu.build-id` byte-for-byte. Its imports use
`system_unbound_wq`, not a per-CPU queue or kthread API. No new firmware-call
workqueue warnings appeared after this load.

Both production and development configurations compiled locally. The development
configuration was not installed. The Rust host suite passed 157 tests; the DKMS
policy suite passed 15, including byte-mismatch and source-umask regression tests.
`scripts/verify-event-work.py` provides bounded idle, physical-key, and lid checks.

Physical suspend/resume has not been repeated in this update. The user was
offered that check via desktop notification; APS did not initiate suspend.
Host policy tests cover suspended dispatch suppression and one-shot wake/retry
deadlines, but they are not a substitute for physical wake verification.
