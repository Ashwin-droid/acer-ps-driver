// SPDX-License-Identifier: GPL-2.0-only
//! Blocking PM notifier. Hardware is accessed only outside device suspension.
use core::cell::UnsafeCell;
use kernel::{bindings, prelude::*};

unsafe extern "C" fn notify(
    _: *mut bindings::notifier_block,
    event: kernel::ffi::c_ulong,
    _: *mut kernel::ffi::c_void,
) -> i32 {
    // Do not truncate unknown unsigned-long events into known ones.
    let prepare = match event {
        e if e == bindings::PM_SUSPEND_PREPARE as _
            || e == bindings::PM_HIBERNATION_PREPARE as _
            || e == bindings::PM_RESTORE_PREPARE as _ =>
        {
            true
        }
        e if e == bindings::PM_POST_SUSPEND as _
            || e == bindings::PM_POST_HIBERNATION as _
            || e == bindings::PM_POST_RESTORE as _ =>
        {
            false
        }
        _ => return bindings::NOTIFY_DONE as i32,
    };
    crate::sleep_event(prepare);
    // Lighting failure must not veto system sleep. POST still undoes PREPARE.
    bindings::NOTIFY_OK as i32
}

pub(crate) struct Registration {
    block: KBox<UnsafeCell<bindings::notifier_block>>,
}
// SAFETY: The allocation never moves, notifier-core owns mutations of its links,
// and the callback accesses driver state only through its mutex. No Rust
// references to the inner notifier exist while it is registered.
unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

impl Registration {
    pub(crate) fn new() -> Result<Self> {
        let block = KBox::new(
            UnsafeCell::new(bindings::notifier_block {
                notifier_call: Some(notify),
                next: core::ptr::null_mut(),
                priority: 0,
            }),
            GFP_KERNEL,
        )?;
        // SAFETY: Initialized stable allocation, static callback; kept alive
        // until unregister drains all blocking-notifier callbacks.
        let rc = unsafe { bindings::register_pm_notifier(block.get()) };
        if rc < 0 {
            return Err(Error::from_errno(rc));
        }
        Ok(Self { block })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: Exactly one successful registration, no state mutex held.
        // Blocking notifier unregistration waits for in-flight callbacks.
        unsafe { bindings::unregister_pm_notifier(self.block.get()) };
    }
}
