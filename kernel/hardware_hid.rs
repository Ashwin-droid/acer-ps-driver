// SPDX-License-Identifier: GPL-2.0-only
//! Private HID transport. Userspace cannot choose devices, report IDs, or payloads.
use crate::control::Light;
use kernel::{bindings, prelude::*};
#[allow(dead_code, non_camel_case_types, unreachable_pub)]
mod layout {
    include!("hid_layout.rs");
}

extern "C" {
    static hid_bus_type: bindings::bus_type;
    fn hid_hw_raw_request(
        hid: *mut kernel::ffi::c_void,
        reportnum: u8,
        buffer: *mut u8,
        length: usize,
        report_type: i32,
        request_type: i32,
    ) -> i32;
}

#[derive(Clone, Copy)]
enum Target {
    Lighting,
    Timeout,
}
impl Target {
    fn ids(self) -> (u32, u32) {
        match self {
            Self::Lighting => (0x0cf2, 0x5130),
            Self::Timeout => (0x1025, 0x174b),
        }
    }
    fn descriptor(self) -> &'static [u8] {
        match self {
            Self::Lighting => include_bytes!("../research/firmware/enek5130-descriptor.bin"),
            Self::Timeout => include_bytes!("../research/firmware/acer-ec-hid-descriptor.bin"),
        }
    }
}

// Offsets are generated from the exact installed headers, not guessed struct layouts.
unsafe fn from_device(device: *mut bindings::device) -> *mut u8 {
    unsafe { device.cast::<u8>().sub(layout::PH_HID_DEV as usize) }
}

unsafe extern "C" fn matches(
    device: *mut bindings::device,
    data: *const kernel::ffi::c_void,
) -> i32 {
    // SAFETY: HID bus supplies a live embedded device, and bus_find_device keeps data alive
    // through the synchronous enumeration. Identity fields are immutable after registration.
    let target = unsafe { *data.cast::<Target>() };
    let hid = unsafe { from_device(device) };
    let bus = unsafe { *hid.add(layout::PH_HID_BUS as usize).cast::<u16>() };
    let vendor = unsafe { *hid.add(layout::PH_HID_VENDOR as usize).cast::<u32>() };
    let product = unsafe { *hid.add(layout::PH_HID_PRODUCT as usize).cast::<u32>() };
    i32::from(bus == 0x18 && (vendor, product) == target.ids())
}

struct Hid {
    device: *mut bindings::device,
    target: Target,
}
impl Hid {
    fn open(target: Target) -> Result<Self> {
        // SAFETY: Static HID bus, valid synchronous match callback and stack-owned identity.
        let device = unsafe {
            bindings::bus_find_device(
                &raw const hid_bus_type,
                core::ptr::null_mut(),
                (&target as *const Target).cast(),
                Some(matches),
            )
        };
        if device.is_null() {
            return Err(ENODEV);
        }
        // SAFETY: bus_find_device returned a referenced device. Locking prevents unbind and
        // descriptor teardown throughout validation and synchronous feature transfers.
        unsafe { bindings::mutex_lock(core::ptr::addr_of_mut!((*device).mutex)) };
        let this = Self { device, target };
        if unsafe { (*device).driver.is_null() } {
            return Err(ENODEV);
        }
        let hid = unsafe { from_device(device) };
        let length = unsafe { *hid.add(layout::PH_HID_RSIZE as usize).cast::<u32>() } as usize;
        let descriptor = unsafe { *hid.add(layout::PH_HID_RDESC as usize).cast::<*const u8>() };
        if length != target.descriptor().len() || descriptor.is_null() {
            return Err(ENODEV);
        }
        // SAFETY: Live HID descriptor is protected by the device lock, with exact checked length.
        if unsafe { core::slice::from_raw_parts(descriptor, length) } != target.descriptor() {
            return Err(ENODEV);
        }
        Ok(this)
    }

    fn report<const N: usize>(&self, input: [u8; N], write: bool) -> Result<[u8; N]> {
        // Only internal call sites can reach this function. Explicit report/length pairs also
        // prevent future callers from accidentally turning it into a generic HID endpoint.
        let valid = match self.target {
            Target::Lighting => matches!(
                (input[0], N, write),
                (0xa1, 5, false) | (0xa2, 2, true) | (0xa3, 9, false) | (0xa4, 11, true)
            ),
            Target::Timeout => input[0] == 0xa0 && N == 65,
        };
        if !valid {
            return Err(EINVAL);
        }
        // Some HID transports require DMA-capable memory. KBox supplies a kernel allocation;
        // never pass a stack buffer to the low-level driver.
        let mut buffer = KBox::new(input, GFP_KERNEL)?;
        let hid = unsafe { from_device(self.device) };
        // SAFETY: Device stays referenced and locked. Exact-size allocated buffer lives through
        // the synchronous call. HID_FEATURE_REPORT=2, GET_REPORT=1, SET_REPORT=9.
        let transferred = unsafe {
            hid_hw_raw_request(
                hid.cast(),
                input[0],
                buffer.as_mut_ptr(),
                N,
                2,
                if write { 9 } else { 1 },
            )
        };
        if transferred < 0 {
            return Err(Error::from_errno(transferred));
        }
        if transferred as usize != N {
            return Err(EIO);
        }
        Ok(*buffer)
    }

    fn lighting_info(&self) -> Result<(u8, u32)> {
        let list = self.report([0xa1, 0, 0, 0, 0], false)?;
        if list[0] != 0xa1 || list[1] > 3 || !list[2..2 + list[1] as usize].contains(&0x21) {
            return Err(ENODEV);
        }
        self.report([0xa2, 0x21], true)?;
        unsafe { bindings::msleep(20) };
        let caps = self.report([0xa3, 0, 0, 0, 0, 0, 0, 0, 0], false)?;
        if caps[0] != 0xa3 || caps[1] != 0x21 || caps[4] != 4 {
            return Err(ENODEV);
        }
        Ok((
            caps[4],
            u32::from_le_bytes([caps[5], caps[6], caps[7], caps[8]]),
        ))
    }

    fn turbo_info(&self) -> Result<(u8, u8, u32)> {
        let list = self.report([0xa1, 0, 0, 0, 0], false)?;
        if list[0] != 0xa1 || list[1] > 3 {
            return Err(EIO);
        }
        let targets = &list[2..2 + list[1] as usize];
        // Vendor constructors name 0x64 TurboKeyLED and 0x65 ModeKeyLED. No generic target ID.
        let target = if targets.contains(&0x64) {
            0x64
        } else if targets.contains(&0x65) {
            0x65
        } else {
            return Err(ENODEV);
        };
        self.report([0xa2, target], true)?;
        unsafe { bindings::msleep(20) };
        let caps = self.report([0xa3, 0, 0, 0, 0, 0, 0, 0, 0], false)?;
        if caps[0] != 0xa3 || caps[1] != target || caps[4] > 1 {
            return Err(ENODEV);
        }
        Ok((
            target,
            1,
            u32::from_le_bytes([caps[5], caps[6], caps[7], caps[8]]),
        ))
    }

    fn logo_info(&self) -> Result<u32> {
        let list = self.report([0xa1, 0, 0, 0, 0], false)?;
        if list[0] != 0xa1 || list[1] > 3 || !list[2..2 + list[1] as usize].contains(&0x83) {
            return Err(ENODEV);
        }
        self.report([0xa2, 0x83], true)?;
        unsafe { bindings::msleep(20) };
        let caps = self.report([0xa3, 0, 0, 0, 0, 0, 0, 0, 0], false)?;
        if caps[0] != 0xa3 || caps[1] != 0x83 || caps[4] != 1 { return Err(ENODEV); }
        Ok(u32::from_le_bytes([caps[5], caps[6], caps[7], caps[8]]))
    }

    fn timeout_request(&self, setting: Option<(u16, u16)>) -> Result<[u8; 65]> {
        let mut packet = [0u8; 65];
        packet[0] = 0xa0;
        packet[1..9].copy_from_slice(
            &if setting.is_some() {
                0x0001_0201_000a_a000u64
            } else {
                0x0000_0202_000a_a000u64
            }
            .to_le_bytes(),
        );
        if let Some((brightness, seconds)) = setting {
            if !matches!(seconds, 0 | 30) {
                return Err(EINVAL);
            }
            packet[9..11].copy_from_slice(&brightness.to_le_bytes());
            packet[11..13].copy_from_slice(&seconds.to_le_bytes());
        }
        self.report(packet, true)?;
        unsafe { bindings::msleep(20) };
        packet.fill(0);
        packet[0] = 0xa0;
        let reply = self.report(packet, false)?;
        if reply[0] != 0xa0
            || reply[1..3] != [0, 0xe0]
            || reply[3..5] != [10, 0]
            || (setting.is_none() && reply[6] != 2)
        {
            return Err(EIO);
        }
        Ok(reply)
    }

    fn timeout_value(&self) -> Result<(u16, u16)> {
        let reply = self.timeout_request(None)?;
        Ok((
            u16::from_le_bytes([reply[9], reply[10]]),
            u16::from_le_bytes([reply[11], reply[12]]),
        ))
    }
}
impl Drop for Hid {
    fn drop(&mut self) {
        // SAFETY: Exactly one acquired device lock and reference are owned by this guard.
        unsafe {
            bindings::mutex_unlock(core::ptr::addr_of_mut!((*self.device).mutex));
            bindings::put_device(self.device);
        }
    }
}

pub(crate) fn lighting_info() -> Result<(u8, u32)> {
    Hid::open(Target::Lighting)?.lighting_info()
}

#[cfg(aps_dev)]
pub(crate) fn raw_probe(p: &crate::dev_probe::wire::Probe) -> Result<([u8;65],usize)> {
    if !p.valid() || !matches!(p.operation,1|2) { return Err(EINVAL); }
    let target=if p.operation==1 { Target::Lighting } else { Target::Timeout };
    let hid=Hid::open(target)?;
    let write=p.action==1;
    let mut result=[0;65];
    // The contents are intentionally raw; descriptor-derived lengths and the
    // exact two model-locked devices remain enforced below and by Hid::report.
    match p.length {
        2 => result[..2].copy_from_slice(&hid.report::<2>(p.data[..2].try_into().map_err(|_|EINVAL)?,write)?),
        5 => result[..5].copy_from_slice(&hid.report::<5>(p.data[..5].try_into().map_err(|_|EINVAL)?,write)?),
        9 => result[..9].copy_from_slice(&hid.report::<9>(p.data[..9].try_into().map_err(|_|EINVAL)?,write)?),
        11 => result[..11].copy_from_slice(&hid.report::<11>(p.data[..11].try_into().map_err(|_|EINVAL)?,write)?),
        65 => result=hid.report(p.data,write)?,
        _ => return Err(EINVAL),
    }
    Ok((result,p.length as usize))
}

pub(crate) fn set_light(setting: Light) -> Result<()> {
    let hid = Hid::open(Target::Lighting)?;
    let (zones, effects) = hid.lighting_info()?;
    // Validate against both the compiled allow-list and the actual controller capability mask.
    if !crate::control::valid_light(setting)
        || setting.zones >= (1u16 << zones)
        || effects & (1 << (setting.mode - 1)) == 0
    {
        return Err(EINVAL);
    }
    let mut packet = crate::protocol::keyboard_report(
        setting.mode,
        setting.brightness,
        setting.speed,
        setting.rgb,
        setting.zones,
    )
    .ok_or(EINVAL)?;
    packet[5] = setting.direction;
    hid.report(packet, true)?;
    Ok(())
}

pub(crate) fn turbo_info() -> Result<(u8, u8, u32)> {
    Hid::open(Target::Lighting)?.turbo_info()
}

pub(crate) fn logo_info() -> Result<u32> { Hid::open(Target::Lighting)?.logo_info() }
pub(crate) fn set_logo(setting: crate::control::Logo) -> Result<()> {
    let report = crate::protocol::logo_report(setting).ok_or(EINVAL)?;
    let hid = Hid::open(Target::Lighting)?;
    let effects = hid.logo_info()?;
    if effects & (1 << (report[2] - 1)) == 0 {
        return Err(Error::from_errno(-(bindings::EOPNOTSUPP as i32)));
    }
    hid.report(report, true)?;
    Ok(())
}
pub(crate) fn set_turbo_led(setting: crate::control::TurboLed) -> Result<()> {
    let hid = Hid::open(Target::Lighting)?;
    let (target, _, effects) = hid.turbo_info()?;
    let (mode, brightness, rgb) = match setting {
        crate::control::TurboLed::Auto => (6, 100, [0; 3]),
        crate::control::TurboLed::Off => (1, 0, [0; 3]),
        crate::control::TurboLed::Rgb(rgb) => (2, 100, rgb),
    };
    if effects & (1 << (mode - 1)) == 0 {
        return Err(EINVAL);
    }
    // Vendor HotkeyLED SetMode uses a zero zone mask for the single LED target.
    hid.report(
        [
            0xa4, target, mode, brightness, 0, 0, rgb[0], rgb[1], rgb[2], 0, 0,
        ],
        true,
    )?;
    Ok(())
}

pub(crate) fn timeout() -> Result<u16> {
    Ok(Hid::open(Target::Timeout)?.timeout_value()?.1)
}

pub(crate) fn set_timeout(enabled: bool) -> Result<()> {
    let hid = Hid::open(Target::Timeout)?;
    let (brightness, previous) = hid.timeout_value()?;
    if !matches!(previous, 0 | 30) {
        return Err(EIO);
    }
    let requested = if enabled { 30 } else { 0 };
    let result = hid
        .timeout_request(Some((brightness, requested)))
        .and_then(|_| {
            if hid.timeout_value()? != (brightness, requested) {
                return Err(EIO);
            }
            Ok(())
        });
    if result.is_err() && hid.timeout_request(Some((brightness, previous))).is_err() {
        pr_err!("acer_phn16s: timeout rollback failed\n");
    }
    result
}
