// SPDX-License-Identifier: GPL-2.0-only
//! Exact Acer HID mode-switch report -> bounded action queue. No ACPI event guessing.
use core::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use kernel::{bindings, prelude::*};
#[allow(dead_code, non_camel_case_types, unreachable_pub)]
mod layout {
    include!("hid_layout.rs");
}
extern "C" {
    fn input_allocate_device() -> *mut kernel::ffi::c_void;
    fn input_free_device(device: *mut kernel::ffi::c_void);
    fn input_register_device(device: *mut kernel::ffi::c_void) -> i32;
    fn input_unregister_device(device: *mut kernel::ffi::c_void);
    fn input_set_capability(device: *mut kernel::ffi::c_void, kind: u32, code: u32);
    fn input_event(device: *mut kernel::ffi::c_void, kind: u32, code: u32, value: i32);
    fn power_supply_is_system_supplied() -> i32;
    fn __hid_register_driver(
        driver: *mut kernel::ffi::c_void,
        module: *mut kernel::ffi::c_void,
        name: *const kernel::ffi::c_char,
    ) -> i32;
    fn hid_unregister_driver(driver: *mut kernel::ffi::c_void);
    fn hid_open_report(hid: *mut kernel::ffi::c_void) -> i32;
    fn hid_hw_start(hid: *mut kernel::ffi::c_void, connect_mask: u32) -> i32;
    fn hid_hw_stop(hid: *mut kernel::ffi::c_void);
    fn hidinput_reset_resume(hid: *mut kernel::ffi::c_void);
}
static INPUT: AtomicPtr<kernel::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
static ACTIVE: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicU32 = AtomicU32::new(0);
static PRESSES: AtomicU64 = AtomicU64::new(0);
static LAST: AtomicU64 = AtomicU64::new(0);
static SENSE_LAST: AtomicU64 = AtomicU64::new(0);
static SENSE_PENDING: AtomicU32 = AtomicU32::new(0);
static SENSE_PRESSES: AtomicU64 = AtomicU64::new(0);
#[repr(C)]
struct Id {
    bus: u16,
    group: u16,
    vendor: u32,
    product: u32,
    data: usize,
}
static IDS: [Id; 2] = [
    Id {
        bus: 0x18,
        group: 1,
        vendor: 0x1025,
        product: 0x174b,
        data: 0,
    },
    Id {
        bus: 0,
        group: 0,
        vendor: 0,
        product: 0,
        data: 0,
    },
];
#[repr(C, align(8))]
struct HidDriver {
    bytes: [u8; layout::PH_DRIVER_SIZE as usize],
}
const _: () = assert!(core::mem::size_of::<Id>() == layout::PH_ID_SIZE as usize);
const _: () = assert!(core::mem::align_of::<HidDriver>() >= layout::PH_DRIVER_ALIGN as usize);

unsafe extern "C" fn probe(hid: *mut kernel::ffi::c_void, _: *const Id) -> i32 {
    let result = (|| {
        // SAFETY: HID core supplies a live, exclusively probing object. Offsets come from
        // exact kernel headers. Only the exact report descriptor is accepted.
        let base = hid.cast::<u8>();
        let length = unsafe { *base.add(layout::PH_HID_DEV_RSIZE as usize).cast::<u32>() } as usize;
        let pointer = unsafe {
            *base
                .add(layout::PH_HID_DEV_RDESC as usize)
                .cast::<*const u8>()
        };
        let expected = include_bytes!("../research/firmware/acer-ec-hid-descriptor.bin");
        if length != expected.len() || pointer.is_null() {
            return Err(ENODEV);
        }
        if unsafe { core::slice::from_raw_parts(pointer, length) } != expected {
            return Err(ENODEV);
        }
        // Preserve hid-generic's normal per-application input behavior.
        unsafe {
            *base.add(layout::PH_HID_QUIRKS as usize).cast::<u32>() |= layout::PH_HID_INPUT_PER_APP;
        }
        let parsed = unsafe { hid_open_report(hid) };
        if parsed < 0 {
            return Err(Error::from_errno(parsed));
        }
        let started = unsafe { hid_hw_start(hid, layout::PH_HID_CONNECT_DEFAULT) };
        if started < 0 {
            return Err(Error::from_errno(started));
        }
        pr_info!("acer_phn16s: Acer HID mode-switch report listener attached\n");
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(error) => error.to_errno(),
    }
}
unsafe extern "C" fn remove(hid: *mut kernel::ffi::c_void) {
    unsafe { hid_hw_stop(hid) };
}
unsafe extern "C" fn reset_resume(hid: *mut kernel::ffi::c_void) -> i32 {
    // SAFETY: HID core owns/resumes the device. Preserve generic input state recovery.
    let claimed = unsafe {
        *hid.cast::<u8>()
            .add(layout::PH_HID_CLAIMED as usize)
            .cast::<u32>()
    };
    if claimed & layout::PH_HID_CLAIMED_INPUT != 0 {
        unsafe { hidinput_reset_resume(hid) };
    }
    0
}
unsafe extern "C" fn raw_event(
    _: *mut kernel::ffi::c_void,
    _: *mut kernel::ffi::c_void,
    data: *mut u8,
    size: i32,
) -> i32 {
    if !ACTIVE.load(Ordering::Acquire) || !crate::events::accepting() || size != 3 || data.is_null() {
        return 0;
    }
    // SAFETY: HID core supplies size readable bytes, and we read only an exact three-byte report.
    let report = unsafe { core::slice::from_raw_parts(data, 3) };
    let (last, pending, presses) = if crate::control::turbo_hid_report(report) {
        (&LAST, &PENDING, &PRESSES)
    } else if crate::control::sense_hid_report(report) {
        (&SENSE_LAST, &SENSE_PENDING, &SENSE_PRESSES)
    } else {
        return 0;
    };
    if !ACTIVE.load(Ordering::Acquire) {
        return 0;
    }
    let stamp = unsafe { bindings::ktime_get_boot_fast_ns() };
    let previous = last.load(Ordering::Relaxed);
    if stamp.saturating_sub(previous) < 250_000_000
        || last
            .compare_exchange(previous, stamp, Ordering::AcqRel, Ordering::Relaxed)
            .is_err()
    {
        return 0;
    }
    presses.fetch_add(1, Ordering::Relaxed);
    let _ = pending.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |value| {
        Some(value.saturating_add(1).min(8))
    });
    crate::events::queue();
    // Returning zero preserves normal HID parsing and all unrelated keyboard reports.
    0
}

pub(crate) fn take_press() -> bool {
    PENDING
        .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n| n.checked_sub(1))
        .is_ok()
}
pub(crate) fn presses() -> u64 {
    PRESSES.load(Ordering::Relaxed)
}
pub(crate) fn take_sense_press() -> bool {
    SENSE_PENDING
        .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n| n.checked_sub(1))
        .is_ok()
}
pub(crate) fn sense_presses() -> u64 {
    SENSE_PRESSES.load(Ordering::Relaxed)
}
pub(crate) fn queue_sense_press() {
    if !crate::events::accepting() { return; }
    SENSE_PRESSES.fetch_add(1, Ordering::Relaxed);
    let _ = SENSE_PENDING.fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n| {
        Some(n.saturating_add(1).min(8))
    });
    crate::events::queue();
}
pub(crate) fn discard_presses() {
    PENDING.store(0, Ordering::Release);
    SENSE_PENDING.store(0, Ordering::Release);
}
pub(crate) fn pending() -> bool {
    PENDING.load(Ordering::Acquire) != 0 || SENSE_PENDING.load(Ordering::Acquire) != 0
}
pub(crate) fn on_ac() -> bool {
    unsafe { power_supply_is_system_supplied() > 0 }
}
pub(crate) fn emit_f14() -> Result<()> {
    emit_key(184)
}
pub(crate) fn emit_f15() -> Result<()> {
    emit_key(185)
}
fn emit_key(key: u32) -> Result<()> {
    if !matches!(key, 184 | 185) {
        return Err(EINVAL);
    }
    let input = INPUT.load(Ordering::Acquire);
    if input.is_null() {
        return Err(ENODEV);
    }
    // SAFETY: Work is drained before this input device is freed. Only F14/F15 are emitted.
    unsafe {
        input_event(input, 1, key, 1);
        input_event(input, 0, 0, 0);
        input_event(input, 1, key, 0);
        input_event(input, 0, 0, 0);
    }
    Ok(())
}

pub(crate) struct Registration {
    input: *mut kernel::ffi::c_void,
    driver: KBox<HidDriver>,
    input_registered: bool,
    hid_registered: bool,
}
// SAFETY: Module exclusively owns registration. HID unregister drains callbacks before the
// driver allocation is freed, and the module stops its worker before freeing the input device.
unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}
impl Registration {
    pub(crate) fn new(module: &'static ThisModule) -> Result<Self> {
        let driver = KBox::new(
            HidDriver {
                bytes: [0; layout::PH_DRIVER_SIZE as usize],
            },
            GFP_KERNEL,
        )?;
        let input = unsafe { input_allocate_device() };
        if input.is_null() {
            return Err(ENOMEM);
        }
        let mut this = Self {
            input,
            driver,
            input_registered: false,
            hid_registered: false,
        };
        let base = input.cast::<u8>();
        // SAFETY: Fresh input_dev is exclusively owned; offset constants come from exact headers.
        unsafe {
            *base
                .add(layout::PH_INPUT_NAME as usize)
                .cast::<*const kernel::ffi::c_char>() = c"APS Turbo Button".as_char_ptr();
            *base
                .add(layout::PH_INPUT_PHYS as usize)
                .cast::<*const kernel::ffi::c_char>() = c"aps/turbo0".as_char_ptr();
            *base.add(layout::PH_INPUT_BUS as usize).cast::<u16>() = 0x19;
            *base.add(layout::PH_INPUT_VENDOR as usize).cast::<u16>() = 0x1025;
            *base.add(layout::PH_INPUT_PRODUCT as usize).cast::<u16>() = 0xf014;
            *base.add(layout::PH_INPUT_VERSION as usize).cast::<u16>() = 1;
            input_set_capability(input, 1, 184);
            input_set_capability(input, 1, 185);
        }
        let status = unsafe { input_register_device(input) };
        if status < 0 {
            return Err(Error::from_errno(status));
        }
        this.input_registered = true;
        INPUT.store(input, Ordering::Release);
        let base = this.driver.bytes.as_mut_ptr();
        // SAFETY: Aligned zeroed hid_driver storage has the header-verified size and offsets.
        // It stays at this allocation until hid_unregister_driver drains/removes every callback.
        unsafe {
            *base
                .add(layout::PH_DRIVER_NAME as usize)
                .cast::<*const kernel::ffi::c_char>() = c"aps-mode-button".as_char_ptr();
            *base.add(layout::PH_DRIVER_IDS as usize).cast::<*const Id>() = IDS.as_ptr();
            *base
                .add(layout::PH_DRIVER_PROBE as usize)
                .cast::<Option<unsafe extern "C" fn(*mut kernel::ffi::c_void, *const Id) -> i32>>(
                ) = Some(probe);
            *base
                .add(layout::PH_DRIVER_REMOVE as usize)
                .cast::<Option<unsafe extern "C" fn(*mut kernel::ffi::c_void)>>() = Some(remove);
            *base
                .add(layout::PH_DRIVER_RAW_EVENT as usize)
                .cast::<Option<
                    unsafe extern "C" fn(
                        *mut kernel::ffi::c_void,
                        *mut kernel::ffi::c_void,
                        *mut u8,
                        i32,
                    ) -> i32,
                >>() = Some(raw_event);
            *base
                .add(layout::PH_DRIVER_RESET_RESUME as usize)
                .cast::<Option<unsafe extern "C" fn(*mut kernel::ffi::c_void) -> i32>>() =
                Some(reset_resume);
        }
        let status = unsafe {
            __hid_register_driver(
                base.cast(),
                module.as_ptr().cast(),
                c"acer_phn16s".as_char_ptr(),
            )
        };
        if status < 0 {
            return Err(Error::from_errno(status));
        }
        this.hid_registered = true;
        ACTIVE.store(true, Ordering::Release);
        Ok(this)
    }
    pub(crate) fn detach(&mut self) {
        ACTIVE.store(false, Ordering::Release);
        if self.hid_registered {
            // HID core detaches/drains our callbacks and reprobes the generic fallback.
            unsafe { hid_unregister_driver(self.driver.bytes.as_mut_ptr().cast()) };
            self.hid_registered = false;
        }
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.detach();
        INPUT.store(core::ptr::null_mut(), Ordering::Release);
        unsafe {
            if self.input_registered {
                input_unregister_device(self.input)
            } else {
                input_free_device(self.input)
            }
        };
    }
}
