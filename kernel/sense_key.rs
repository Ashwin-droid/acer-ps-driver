// SPDX-License-Identifier: GPL-2.0-only
//! Observe ACPI SW_LID and filter only KEY_PRESENTATION on the built-in keyboard.
use kernel::prelude::*;
#[allow(dead_code, non_camel_case_types, unreachable_pub)]
mod layout {
    include!("hid_layout.rs");
}
type Void = kernel::ffi::c_void;
extern "C" {
    fn input_register_handler(handler: *mut Void) -> i32;
    fn input_unregister_handler(handler: *mut Void);
    fn input_register_handle(handle: *mut Void) -> i32;
    fn input_unregister_handle(handle: *mut Void);
    fn input_open_device(handle: *mut Void) -> i32;
    fn input_close_device(handle: *mut Void);
}
#[repr(C, align(8))]
struct Handler {
    bytes: [u8; layout::PH_INPUT_HANDLER_SIZE as usize],
}
#[repr(C, align(8))]
struct Handle {
    bytes: [u8; layout::PH_INPUT_HANDLE_SIZE as usize],
}
#[repr(C, align(8))]
struct Ids {
    bytes: [u8; 3 * layout::PH_INPUT_ID_SIZE as usize],
}
unsafe extern "C" fn matches(_: *mut Void, device: *mut Void) -> bool {
    // SAFETY: Input core supplies a live device; identity strings have device lifetime.
    let base = device.cast::<u8>();
    let name = unsafe {
        *base
            .add(layout::PH_INPUT_NAME as usize)
            .cast::<*const kernel::ffi::c_char>()
    };
    let phys = unsafe {
        *base
            .add(layout::PH_INPUT_PHYS as usize)
            .cast::<*const kernel::ffi::c_char>()
    };
    !name.is_null()
        && !phys.is_null()
        && ((unsafe { CStr::from_char_ptr(name) } == c"AT Translated Set 2 keyboard"
            && unsafe { CStr::from_char_ptr(phys) } == c"isa0060/serio0/input0")
            || (unsafe { CStr::from_char_ptr(name) } == c"Lid Switch"
            && unsafe { CStr::from_char_ptr(phys) } == c"PNP0C0D/button/input0"))
}
unsafe extern "C" fn filter(_: *mut Void, kind: u32, code: u32, value: i32) -> bool {
    if kind == 5 && code == 0 && matches!(value, 0 | 1) {
        // EV_SW/SW_LID triggers an authoritative ACPI sample in process context.
        crate::events::lid_changed();
        return false; // Preserve delivery to logind and other listeners.
    }
    if kind != 1 || code != 425 {
        return false;
    }
    if value == 1 {
        crate::turbo_button::queue_sense_press();
    }
    // Consume only this key's original press/release/repeat, preserving all other typing.
    true
}
unsafe extern "C" fn connect(handler: *mut Void, device: *mut Void, _: *const Void) -> i32 {
    let result = (|| {
        let mut handle = KBox::new(
            Handle {
                bytes: [0; layout::PH_INPUT_HANDLE_SIZE as usize],
            },
            GFP_KERNEL,
        )?;
        let p = handle.bytes.as_mut_ptr();
        // SAFETY: Fresh aligned input_handle storage. Header-generated fields initialized
        // before registration; allocation is retained until disconnect.
        unsafe {
            *p.add(layout::PH_INPUT_HANDLE_NAME as usize)
                .cast::<*const kernel::ffi::c_char>() = c"aps-sense-key".as_char_ptr();
            *p.add(layout::PH_INPUT_HANDLE_DEVICE as usize)
                .cast::<*mut Void>() = device;
            *p.add(layout::PH_INPUT_HANDLE_HANDLER as usize)
                .cast::<*mut Void>() = handler;
        }
        let result = unsafe { input_register_handle(p.cast()) };
        if result < 0 {
            return Err(Error::from_errno(result));
        }
        let result = unsafe { input_open_device(p.cast()) };
        if result < 0 {
            unsafe { input_unregister_handle(p.cast()) };
            return Err(Error::from_errno(result));
        }
        let _ = KBox::into_raw(handle);
        // Registration need not replay the current switch state. Sample once.
        crate::events::lid_changed();
        pr_info!("acer_phn16s: built-in hotkey/lid input observer attached\n");
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => error.to_errno(),
    }
}
unsafe extern "C" fn disconnect(handle: *mut Void) {
    // SAFETY: Input core serializes disconnect and stops delivering events to the handle.
    unsafe {
        input_close_device(handle);
        input_unregister_handle(handle);
        drop(KBox::from_raw(handle.cast::<Handle>()));
    }
    crate::events::lid_changed();
}
pub(crate) struct Registration {
    handler: KBox<Handler>,
    _ids: KBox<Ids>,
}
unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}
impl Registration {
    pub(crate) fn new() -> Result<Self> {
        let mut handler = KBox::new(
            Handler {
                bytes: [0; layout::PH_INPUT_HANDLER_SIZE as usize],
            },
            GFP_KERNEL,
        )?;
        let mut ids = KBox::new(
            Ids {
                bytes: [0; 3 * layout::PH_INPUT_ID_SIZE as usize],
            },
            GFP_KERNEL,
        )?;
        let p = ids.bytes.as_mut_ptr();
        unsafe {
            *p.add(layout::PH_INPUT_ID_FLAGS as usize).cast::<usize>() =
                layout::PH_INPUT_MATCH_BUS as usize;
            *p.add(layout::PH_INPUT_ID_BUS as usize).cast::<u16>() = 0x11;
            let lid = p.add(layout::PH_INPUT_ID_SIZE as usize);
            *lid.add(layout::PH_INPUT_ID_FLAGS as usize).cast::<usize>() = layout::PH_INPUT_MATCH_BUS as usize;
            *lid.add(layout::PH_INPUT_ID_BUS as usize).cast::<u16>() = 0x19;
        }
        let p = handler.bytes.as_mut_ptr();
        // SAFETY: Exact-size aligned zeroed objects, static callbacks and strings. The id
        // table and handler allocations outlive all registrations and are never moved.
        unsafe {
            *p.add(layout::PH_INPUT_HANDLER_NAME as usize)
                .cast::<*const kernel::ffi::c_char>() = c"aps-sense-key".as_char_ptr();
            *p.add(layout::PH_INPUT_HANDLER_IDS as usize)
                .cast::<*const Void>() = ids.bytes.as_ptr().cast();
            *p.add(layout::PH_INPUT_HANDLER_FILTER as usize)
                .cast::<Option<unsafe extern "C" fn(*mut Void, u32, u32, i32) -> bool>>() =
                Some(filter);
            *p.add(layout::PH_INPUT_HANDLER_MATCH as usize)
                .cast::<Option<unsafe extern "C" fn(*mut Void, *mut Void) -> bool>>() =
                Some(matches);
            *p.add(layout::PH_INPUT_HANDLER_CONNECT as usize)
                .cast::<Option<unsafe extern "C" fn(*mut Void, *mut Void, *const Void) -> i32>>() =
                Some(connect);
            *p.add(layout::PH_INPUT_HANDLER_DISCONNECT as usize)
                .cast::<Option<unsafe extern "C" fn(*mut Void)>>() = Some(disconnect);
        }
        let result = unsafe { input_register_handler(p.cast()) };
        if result < 0 {
            return Err(Error::from_errno(result));
        }
        Ok(Self { handler, _ids: ids })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        unsafe { input_unregister_handler(self.handler.bytes.as_mut_ptr().cast()) };
    }
}
