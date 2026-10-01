// SPDX-License-Identifier: GPL-2.0-only
use crate::{control::Fans, protocol::*};
use kernel::{bindings, prelude::*};

enum Request {
    CpuRpm,
    GpuRpm,
    Profile,
    SetProfile(u8),
    Mux,
    SetMux(crate::mux::Mode),
    BootEffects,
    SetBootEffects(crate::boot_policy::Effects),
    Fans,
    SetFans(u8),
    Speed(bool),
    SetSpeed(bool, u8),
}

fn execute(request: Request) -> Result<u64> {
    let (method, payload, input_len, output_len) = match request {
        Request::CpuRpm => (5, 0x0201u64, 4, 8),
        Request::GpuRpm => (5, 0x0601, 4, 8),
        Request::Profile => (0x17, 0x0b, 4, 8),
        Request::Mux => (0x17, 2, 4, 8),
        Request::SetMux(mode) => (0x16, mode.payload(), 8, 4),
        Request::BootEffects => (0x17, 6, 4, 8),
        Request::SetBootEffects(value) => (0x16, value.payload(), 8, 4),
        Request::SetProfile(mode) if valid_profile(mode as u64) => {
            (0x16, 0x0b | ((mode as u64) << 8), 8, 4)
        }
        Request::SetProfile(_) => return Err(EINVAL),
        Request::Fans => (0x0f, 9, 4, 8),
        Request::SetFans(mode) => (0x0e, fan_behavior(mode).ok_or(EINVAL)?, 8, 4),
        Request::Speed(gpu) => (0x11, if gpu { 4 } else { 1 }, 4, 8),
        Request::SetSpeed(gpu, percent) => (0x10, fan_speed(gpu, percent).ok_or(EINVAL)?, 8, 4),
    };
    let result=firmware_buffer(method,&payload.to_le_bytes()[..input_len],output_len);
    if matches!(method,0x0e|0x10|0x16) { unsafe { bindings::msleep(30) }; }
    let bytes=result?;
    if bytes[0]!=0 { return Err(EIO); }
    Ok(u64::from_le_bytes(bytes)>>8)
}

fn firmware_buffer(method:u64,payload:&[u8],output_len:usize)->Result<[u8;8]> {
    if !matches!(payload.len(),4|8) || !matches!(output_len,4|8) { return Err(EINVAL); }
    let mut input=[0u8;8];
    input[..payload.len()].copy_from_slice(payload);
    // SAFETY: Zeroed union storage is valid; every active variant is initialized below.
    let mut args: [bindings::acpi_object; 3] = unsafe { core::mem::zeroed() };
    args[0].integer = bindings::acpi_object__bindgen_ty_1 { type_: 1, value: 0 };
    args[1].integer = bindings::acpi_object__bindgen_ty_1 {
        type_: 1,
        value: method,
    };
    args[2].buffer = bindings::acpi_object__bindgen_ty_3 {
        type_: 3,
        length: payload.len() as u32,
        pointer: input.as_mut_ptr(),
    };
    let mut list = bindings::acpi_object_list {
        count: 3,
        pointer: args.as_mut_ptr(),
    };
    let mut output = bindings::acpi_buffer {
        length: u64::MAX,
        pointer: core::ptr::null_mut(),
    };
    // SAFETY: Static ACPI path, bounded buffers, initialized input objects alive for
    // this synchronous call. ACPICA allocates output for ACPI_ALLOCATE_BUFFER.
    let status = unsafe {
        bindings::acpi_evaluate_object(
            core::ptr::null_mut(),
            c"\\_SB.PC00.WMID.WMBH".as_char_ptr().cast_mut(),
            &mut list,
            &mut output,
        )
    };
    let result = (|| {
        if status != 0
            || output.pointer.is_null()
            || output.length < core::mem::size_of::<bindings::acpi_object>() as u64
        {
            return Err(EIO);
        }
        // SAFETY: ACPICA returned an aligned object of the checked minimum length.
        let object = unsafe { &*output.pointer.cast::<bindings::acpi_object>() };
        if unsafe { object.type_ } != 3 {
            return Err(EIO);
        }
        let buffer = unsafe { object.buffer };
        if buffer.length as usize != output_len || buffer.pointer.is_null() {
            return Err(EIO);
        }
        let mut bytes = [0u8; 8];
        // SAFETY: ACPICA's returned buffer has exactly 4 or 8 bytes from the fixed operation table.
        bytes[..output_len]
            .copy_from_slice(unsafe { core::slice::from_raw_parts(buffer.pointer, output_len) });
        Ok(bytes)
    })();
    // SAFETY: Output is NULL or ACPICA's kernel allocation, released exactly once.
    unsafe { bindings::kfree(output.pointer) };
    result
}

#[cfg(aps_dev)]
pub(crate) fn raw_probe(p:&crate::dev_probe::wire::Probe)->Result<([u8;65],usize)> {
    if p.operation!=3 || !p.valid() { return Err(EINVAL); }
    let raw=firmware_buffer(p.method as u64,&p.data[..p.length as usize],p.output_length as usize)?;
    let mut out=[0;65]; out[..p.output_length as usize].copy_from_slice(&raw[..p.output_length as usize]);
    // Return the raw status byte too. Do not interpret undocumented replies.
    Ok((out,p.output_length as usize))
}

pub(crate) fn rpm(gpu: bool) -> Result<u64> {
    execute(if gpu {
        Request::GpuRpm
    } else {
        Request::CpuRpm
    })
}
pub(crate) fn profile() -> Result<u64> {
    profile_from_firmware(execute(Request::Profile)?).ok_or(EIO)
}
pub(crate) fn mux_mode() -> Result<crate::mux::Mode> {
    // AcerAgentService's GPU_MODE getter decodes byte 1 after the status byte.
    crate::mux::Mode::from_firmware(execute(Request::Mux)? as u8).ok_or(EIO)
}
pub(crate) fn set_mux(mode: crate::mux::Mode) -> Result<()> {
    // Firmware owns persistence. Do not reboot, touch PRIME, or assume a failed
    // reply means the nonvolatile setting stayed unchanged. No blind rollback.
    execute(Request::SetMux(mode))?;
    Ok(())
}
pub(crate) fn boot_effects() -> Result<crate::boot_policy::Effects> {
    crate::boot_policy::Effects::decode(execute(Request::BootEffects)? as u8).ok_or(EIO)
}
pub(crate) fn set_boot_effects(value: crate::boot_policy::Effects) -> Result<()> {
    boot_effects()?;
    execute(Request::SetBootEffects(value))?;
    Ok(())
}
pub(crate) fn fan_state() -> Result<u64> {
    execute(Request::Fans)
}
pub(crate) fn fan_setting() -> Result<Fans> {
    let state = fan_state()?;
    match (state & 3, (state >> 6) & 3) {
        (1, 1) => Ok(Fans::Auto),
        (2, 2) => Ok(Fans::Maximum),
        (3, 3) => {
            let cpu = execute(Request::Speed(false))?;
            let gpu = execute(Request::Speed(true))?;
            if !(40..=100).contains(&cpu) || !(40..=100).contains(&gpu) { return Err(EIO); }
            Ok(Fans::Manual(cpu as u8, gpu as u8))
        }
        _ => Err(EIO),
    }
}
pub(crate) fn auto() -> Result<()> {
    execute(Request::SetFans(1))?;
    if execute(Request::Fans)? & 0xc3 != 0x41 {
        return Err(EIO);
    }
    Ok(())
}

pub(crate) fn set_profile(mode: u8) -> Result<()> {
    if !valid_profile(mode as u64) {
        return Err(EINVAL);
    }
    // A well-formed firmware reply can contain a boot/default mode that is not
    // one of the documented setter values. Do not replay that value as a write,
    // or let it prevent selecting an explicitly allow-listed mode. Transport,
    // status and buffer validation errors still abort before any write.
    let raw_previous = execute(Request::Profile)?;
    let previous = profile_from_firmware(raw_previous);
    if previous.is_none() {
        pr_info!("acer_phn16s: firmware profile {} is not a writable mode; selecting validated mode {}\n", raw_previous, mode);
    }
    if execute(Request::SetProfile(mode)).is_err()
        || !matches!(profile(),Ok(value) if value == mode as u64)
    {
        if let Some(previous) = previous {
            if execute(Request::SetProfile(previous as u8)).is_err()
                || !matches!(profile(), Ok(value) if value == previous)
            {
                pr_err!("acer_phn16s: profile rollback failed\n");
            }
        } else {
            pr_err!("acer_phn16s: profile selection failed; unknown firmware mode cannot be safely replayed\n");
        }
        return Err(EIO);
    }
    Ok(())
}

pub(crate) fn set_fans(setting: Fans) -> Result<()> {
    let mode = match setting {
        Fans::Auto => 1,
        Fans::Maximum => 2,
        Fans::Manual(_, _) => 3,
    };
    if let Fans::Manual(cpu, gpu) = setting {
        if fan_speed(false, cpu).is_none() || fan_speed(true, gpu).is_none() {
            return Err(EINVAL);
        }
    }
    let result = (|| {
        execute(Request::SetFans(mode))?;
        if let Fans::Manual(cpu, gpu) = setting {
            execute(Request::SetSpeed(false, cpu))?;
            execute(Request::SetSpeed(true, gpu))?;
            if execute(Request::Speed(false))? != cpu as u64
                || execute(Request::Speed(true))? != gpu as u64
            {
                return Err(EIO);
            }
        }
        let state = fan_state()?;
        if state & 3 != mode as u64 || (state >> 6) & 3 != mode as u64 {
            return Err(EIO);
        }
        Ok(())
    })();
    if result.is_err() && auto().is_err() {
        pr_err!("acer_phn16s: fan rollback failed\n");
    }
    result
}
