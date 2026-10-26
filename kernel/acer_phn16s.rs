// SPDX-License-Identifier: GPL-2.0-only
//! Model-locked sysfs controls. Raw signed firmware probes only in dev builds.
use core::{
    fmt::Write,
    sync::atomic::{AtomicPtr, Ordering},
};
use kernel::{
    bindings,
    fs::File,
    miscdevice::{MiscDevice, MiscDeviceOptions, MiscDeviceRegistration},
    new_mutex,
    prelude::*,
    sync::Mutex,
};
mod acpi_controls;
#[path = "../src/charge_policy.rs"]
mod charge_policy;
#[path = "../src/control.rs"]
mod control;
#[path = "../src/interface.rs"]
mod interface;
mod hardware_hid;
#[path = "../src/protocol.rs"]
mod protocol;
mod sense_key;
mod events;
#[path = "../src/event_policy.rs"]
mod event_policy;
mod sleep;
#[path = "../src/sleep_state.rs"]
mod sleep_state;
#[path = "../src/logo_recovery.rs"]
mod logo_recovery;
#[path = "../src/lid_state.rs"]
mod lid_state;
#[path = "../src/mux.rs"]
mod mux;
#[path = "../src/boot_policy.rs"]
mod boot_policy;
mod turbo_button;
#[cfg(aps_dev)]
mod dev_probe;

module! { type: AcerModule, name: "acer_phn16s", authors: ["Ashwin Mhaskar"],
description: "Bounded PHN16S-71 sysfs controls in Rust", license: "GPL", }

extern "C" {
    fn dmi_get_system_info(field: i32) -> *const kernel::ffi::c_char;
    // Exported by the kernel ACPI button driver; evaluates the registered lid's _LID.
    fn acpi_lid_open() -> i32;
}
fn matches_dmi(field: i32, expected: &CStr) -> bool {
    // SAFETY: Kernel returns static NUL-terminated DMI text or NULL.
    let p = unsafe { dmi_get_system_info(field) };
    !p.is_null() && unsafe { CStr::from_char_ptr(p) } == expected
}

struct State {
    #[cfg(aps_dev)]
    developer: dev_probe::Developer,
    last_light_ns: u64,
    button_mode: control::ButtonMode,
    button_error: i32,
    turbo_led: Option<control::TurboLed>,
    turbo_led_manual: control::TurboLed,
    logo: Option<control::Logo>,
    sleep_logo: control::Logo,
    keyboard_zones: [Option<control::Light>; 4],
    generation: u64,
    sleep: sleep_state::SleepState,
    logo_recovery: logo_recovery::LogoRecovery,
    lid: lid_state::LidState,
    mux_request: Option<(mux::Mode, i32)>,
    mux_last_ns: Option<u64>,
    boot_last_ns: Option<u64>,
}
static STATE: AtomicPtr<Mutex<State>> = AtomicPtr::new(core::ptr::null_mut());
fn now() -> u64 {
    unsafe { bindings::ktime_get_boot_fast_ns() }
}

struct Anchor;
#[vtable]
impl MiscDevice for Anchor {
    type Ptr = KBox<Self>;
    // The misc device is only a sysfs anchor. No ioctl/read/write control bypass exists.
    fn open(_: &File, _: &MiscDeviceRegistration<Self>) -> Result<Self::Ptr> {
        Err(ENODEV)
    }
}

struct Attribute(bindings::device_attribute);
// SAFETY: Immutable static metadata; callbacks never mutate attribute objects.
unsafe impl Sync for Attribute {}
const fn attr(name: &'static CStr, writable: bool) -> Attribute {
    Attribute(bindings::device_attribute {
        attr: bindings::attribute {
            name: name.as_ptr().cast(),
            mode: if writable { 0o660 } else { 0o440 },
        },
        #[cfg(not(aps_attr_alternatives))]
        show: Some(show),
        #[cfg(not(aps_attr_alternatives))]
        store: if writable { Some(store) } else { None },
        #[cfg(aps_attr_alternatives)]
        __bindgen_anon_1: bindings::device_attribute__bindgen_ty_1 {
            show: Some(show),
            #[cfg(aps_attr_cfi)]
            show_const: None,
        },
        #[cfg(aps_attr_alternatives)]
        __bindgen_anon_2: bindings::device_attribute__bindgen_ty_2 {
            store: if writable { Some(store) } else { None },
            #[cfg(aps_attr_cfi)]
            store_const: None,
        },
    })
}
const fn root_control(name: &'static CStr) -> Attribute {
    let mut attribute = attr(name, true);
    attribute.0.attr.mode = 0o640;
    attribute
}
static ATTRS: &[Attribute] = &[
    attr(c"battery_capabilities", false),
    root_control(c"battery_charge_limit"),
    root_control(c"battery_calibration"),
    attr(c"cpu_fan_rpm", false),
    attr(c"gpu_fan_rpm", false),
    attr(c"performance_mode", true),
    attr(c"fan_control", true),
    attr(c"keyboard_lighting_capabilities", false),
    attr(c"keyboard_lighting", true),
    attr(c"keyboard_backlight_timeout_seconds", true),
    attr(c"turbo_button_mode", true),
    attr(c"turbo_button_presses", false),
    attr(c"turbo_button_error", false),
    attr(c"turbo_lighting", true),
    attr(c"turbo_lighting_capabilities", false),
    attr(c"predatorsense_button_presses", false),
    attr(c"cover_lighting", true),
    attr(c"cover_lighting_capabilities", false),
    attr(c"dev_mode", false),
    attr(c"sleep_status", false),
    attr(c"sleep_cover_lighting", true),
    attr(c"lid_state", false),
    attr(c"lid_status", false),
    attr(c"work_status", false),
    root_control(c"gpu_mux_mode"),
    root_control(c"boot_effects"),
    attr(c"gpu_mux_request", false),
    #[cfg(aps_dev)]
    attr(c"dev_challenge", false),
    #[cfg(aps_dev)]
    attr(c"dev_probe", true),
    attr(c"preferences_generation", false),
];

struct Output {
    bytes: [u8; interface::MAX_TEXT],
    len: usize,
}
impl Output {
    fn new() -> Self {
        Self {
            bytes: [0; interface::MAX_TEXT],
            len: 0,
        }
    }
}
impl Write for Output {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(core::fmt::Error)?;
        let target = self.bytes.get_mut(self.len..end).ok_or(core::fmt::Error)?;
        target.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
fn profile_name(value: u64) -> Result<&'static str> {
    match value {
        0 => Ok("quiet"),
        1 => Ok("balanced"),
        4 => Ok("performance"),
        5 => Ok("turbo"),
        6 => Ok("eco"),
        _ => Err(EIO),
    }
}

fn read_control(name: &[u8], state: &State, out: &mut Output) -> Result<()> {
    if matches!(name,b"battery_capabilities"|b"battery_charge_limit"|b"battery_calibration") {
        if state.sleep.sleeping { return Err(EBUSY); }
        let status=acpi_controls::battery_status()?;
        return match name {
            b"battery_capabilities" => writeln!(out,"limit_80={} calibration={} adaptive={}",
                status.capabilities & 1 != 0,status.capabilities & 2 != 0,status.capabilities & 4 != 0),
            b"battery_charge_limit" => {
                if status.capabilities & 1 == 0 { return Err(Error::from_errno(-(bindings::EOPNOTSUPP as i32))); }
                writeln!(out,"{}",if status.limited {80} else {100})
            }
            _ => {
                if status.capabilities & 2 == 0 { return Err(Error::from_errno(-(bindings::EOPNOTSUPP as i32))); }
                writeln!(out,"{}",if status.calibrating {"on"} else {"off"})
            }
        }.map_err(|_| EOVERFLOW);
    }
    if name == b"work_status" {
        let (runs, timer_runs) = events::counters();
        return writeln!(out, "runs={} timer_runs={}", runs, timer_runs).map_err(|_| EOVERFLOW);
    }
    if name == b"boot_effects" {
        if state.sleep.sleeping { return Err(EBUSY); }
        return writeln!(out, "{}", acpi_controls::boot_effects()?.name()).map_err(|_| EOVERFLOW);
    }
    if name == b"gpu_mux_request" {
        return match state.mux_request {
            None => writeln!(out, "none"),
            Some((mode, 0)) => writeln!(out, "{} accepted; restart required", mode.name()),
            Some((mode, error)) => writeln!(out, "{} uncertain; error={}; check firmware selection", mode.name(), error),
        }.map_err(|_| EOVERFLOW);
    }
    if name == b"gpu_mux_mode" {
        if state.sleep.sleeping { return Err(EBUSY); }
        return writeln!(out, "{}", acpi_controls::mux_mode()?.name()).map_err(|_| EOVERFLOW);
    }
    if name == b"lid_state" {
        return writeln!(out, "{}", state.lid.name()).map_err(|_| EOVERFLOW);
    }
    if name == b"lid_status" {
        return writeln!(out, "{} changes={} attempts={} error={}", state.lid.name(),
            state.lid.changes, state.lid.attempts, state.lid.error).map_err(|_| EOVERFLOW);
    }
    if name == b"sleep_status" {
        return writeln!(out, "{} cycles={} attempts={} error={}", state.sleep.phase(),
            state.sleep.cycles, state.sleep.attempts, state.sleep.error).map_err(|_| EOVERFLOW);
    }
    // Cached intent remains readable, but never query suspended HID/ACPI devices.
    if state.sleep.sleeping && matches!(name, b"performance_mode" | b"fan_control" |
        b"cpu_fan_rpm" | b"gpu_fan_rpm" | b"keyboard_lighting_capabilities" |
        b"keyboard_backlight_timeout_seconds" | b"turbo_lighting_capabilities" | b"cover_lighting_capabilities") {
        return Err(EBUSY);
    }
    if name==b"dev_mode" { return writeln!(out,"{}",u8::from(cfg!(aps_dev))).map_err(|_|EOVERFLOW); }
    #[cfg(aps_dev)]
    if name==b"dev_challenge" {
        for byte in state.developer.nonce { write!(out,"{:02x}",byte).map_err(|_|EOVERFLOW)?; }
        return writeln!(out," {} {}",state.developer.sequence,now()).map_err(|_|EOVERFLOW);
    }
    #[cfg(aps_dev)]
    if name==b"dev_probe" {
        writeln!(out,"sequence={} error={} active={}",state.developer.sequence,
            state.developer.last_error,u8::from(state.developer.active)).map_err(|_|EOVERFLOW)?;
        for byte in &state.developer.response[..state.developer.response_len] {
            write!(out,"{:02x}",byte).map_err(|_|EOVERFLOW)?;
        }
        return writeln!(out).map_err(|_|EOVERFLOW);
    }
    let format_result = match name {
        b"sleep_cover_lighting" => writeln!(out, "{}", interface::Cover(state.sleep_logo)),
        b"cover_lighting_capabilities" => writeln!(out, "83 1 {:08x}", hardware_hid::logo_info()?),
        b"cover_lighting" => match state.logo {
            Some(setting) => writeln!(out, "{}", interface::Cover(setting)),
            None => writeln!(out, "undefined"),
        },
        b"preferences_generation" => writeln!(out, "{}", state.generation),
        b"predatorsense_button_presses" => writeln!(out, "{}", turbo_button::sense_presses()),
        b"cpu_fan_rpm" => writeln!(out, "{}", acpi_controls::rpm(false)?),
        b"gpu_fan_rpm" => writeln!(out, "{}", acpi_controls::rpm(true)?),
        b"performance_mode" => writeln!(out, "{}", profile_name(acpi_controls::profile()?)?),
        b"fan_control" => {
            match acpi_controls::fan_setting()? {
                control::Fans::Auto => writeln!(out, "auto"),
                control::Fans::Maximum => writeln!(out, "maximum"),
                control::Fans::Manual(cpu, gpu) => writeln!(out, "manual {} {}", cpu, gpu),
            }
        }
        b"keyboard_lighting_capabilities" => {
            let (zones, effects) = hardware_hid::lighting_info()?;
            writeln!(out, "{} {:08x}", zones, effects)
        }
        b"keyboard_backlight_timeout_seconds" => writeln!(out, "{}", hardware_hid::timeout()?),
        b"turbo_lighting_capabilities" => {
            let (t, z, e) = hardware_hid::turbo_info()?;
            writeln!(out, "{:02x} {} {:08x}", t, z, e)
        }
        b"turbo_lighting" => match state.turbo_led {
            Some(setting) => writeln!(out, "{}", interface::Turbo { setting, manual: state.turbo_led_manual }),
            None => writeln!(out, "undefined"),
        },
        b"turbo_button_mode" => writeln!(
            out,
            "{}",
            match state.button_mode {
                control::ButtonMode::PowerModes => "power modes",
                control::ButtonMode::Hotkey => "hotkey",
            }
        ),
        b"turbo_button_presses" => writeln!(out, "{}", turbo_button::presses()),
        b"turbo_button_error" => writeln!(out, "{}", state.button_error),
        b"keyboard_lighting" => writeln!(out, "{}", interface::Keyboard(state.keyboard_zones)),
        _ => return Err(EINVAL),
    };
    format_result.map_err(|_| EOVERFLOW)
}

fn write_control(name: &[u8], input: &[u8], state: &mut State) -> Result<()> {
    // Parsers reject unsupported, malformed and out-of-range requests before any hardware I/O.
    match name {
        b"battery_charge_limit" | b"battery_calibration" => {
            let name=core::str::from_utf8(name).map_err(|_| EINVAL)?;
            let setting=charge_policy::Operation::parse(name,input).ok_or(EINVAL)?;
            acpi_controls::set_battery(setting)
        }
        b"sleep_cover_lighting" => {
            // Configuration only: never touch the awake logo or its cache.
            state.sleep_logo = interface::Cover::parse(input).ok_or(EINVAL)?;
            Ok(())
        }
        b"boot_effects" => {
            let value = boot_policy::Effects::parse(input).ok_or(EINVAL)?;
            let stamp = now();
            if state.boot_last_ns.is_some_and(|last| stamp.saturating_sub(last) < 2_000_000_000) { return Err(EBUSY); }
            state.boot_last_ns = Some(stamp);
            let result = acpi_controls::set_boot_effects(value);
            pr_info!("acer_phn16s: boot animation/sound {} result={}\n", value.name(), result.as_ref().err().map_or(0, |e| e.to_errno()));
            result
        }
        b"gpu_mux_mode" => {
            let setting = mux::Mode::parse(input).ok_or(EINVAL)?;
            let stamp = now();
            if state.mux_last_ns.is_some_and(|last| stamp.saturating_sub(last) < 2_000_000_000) {
                return Err(EBUSY);
            }
            // Fail closed if the model's known GPU getter is unavailable or
            // returns an unrecognized value. This read cannot select overdrive.
            acpi_controls::mux_mode()?;
            state.mux_last_ns = Some(stamp);
            let result = acpi_controls::set_mux(setting);
            let error = result.as_ref().err().map_or(0, |error| error.to_errno());
            state.mux_request = Some((setting, error));
            pr_info!("acer_phn16s: MUX request {} result={}; restart required on success; no automatic rollback\n", setting.name(), error);
            result
        }
        b"cover_lighting" => {
            let setting = if control::logo_reapply(input) {
                #[cfg(aps_dev)]
                if state.developer.active { return Err(EBUSY); }
                // Called under the same mutex as ordinary writes: replay the
                // latest user choice, never a stale userspace snapshot.
                state.logo.ok_or(EAGAIN)?
            } else {
                interface::Cover::parse(input).ok_or(EINVAL)?
            };
            hardware_hid::set_logo(setting)?;
            state.logo = Some(setting);
            Ok(())
        }
        b"performance_mode" => set_profile_with_lid(control::profile(input).ok_or(EINVAL)?, state),
        b"fan_control" => {
            let setting = control::fans(input).ok_or(EINVAL)?;
            acpi_controls::set_fans(setting)
        }
        b"keyboard_lighting" => {
            let setting = interface::Keyboard::parse(input).filter(|s| s.writable()).ok_or(EINVAL)?;
            require_open_lid(state)?;
            let stamp = now();
            if stamp.saturating_sub(state.last_light_ns) < 10_000_000 {
                return Err(EBUSY);
            }
            for report in sleep_state::keyboard_reports(setting.0).into_iter().flatten() {
                let result = hardware_hid::set_light(report);
                for (zone, stored) in state.keyboard_zones.iter_mut().enumerate() {
                    if report.zones & (1 << zone) != 0 {
                        // A failed transport may have partly applied. Do not
                        // claim the old intent is known for these zones.
                        *stored = if result.is_ok() { Some(control::Light { zones: 1 << zone, ..report }) } else { None };
                    }
                }
                if let Err(error) = result {
                    state.last_light_ns = stamp;
                    state.generation = state.generation.wrapping_add(1);
                    return Err(error);
                }
            }
            state.last_light_ns = stamp;
            Ok(())
        }
        b"keyboard_backlight_timeout_seconds" => {
            let setting = match core::str::from_utf8(input).map_err(|_| EINVAL)?.trim() { "0" => false, "30" => true, _ => return Err(EINVAL) };
            require_open_lid(state)?;
            hardware_hid::set_timeout(setting)
        }
        b"turbo_lighting" => {
            let setting = interface::Turbo::parse(input, state.turbo_led_manual).ok_or(EINVAL)?;
            require_open_lid(state)?;
            hardware_hid::set_turbo_led(setting.setting)?;
            state.turbo_led = Some(setting.setting);
            state.turbo_led_manual = setting.manual;
            Ok(())
        }
        b"turbo_button_mode" => {
            state.button_mode = control::button_mode(input).ok_or(EINVAL)?;
            state.button_error = 0;
            Ok(())
        }
        _ => Err(EINVAL),
    }
}

unsafe extern "C" fn show(
    _: *mut bindings::device,
    attribute: *mut bindings::device_attribute,
    buffer: *mut kernel::ffi::c_char,
) -> isize {
    let result = (|| {
        let pointer = STATE.load(Ordering::Acquire);
        if pointer.is_null() {
            return Err(ENODEV);
        }
        // SAFETY: Removal drains active sysfs callbacks before freeing state. Attribute is a
        // static entry registered by this module, with a static NUL-terminated name.
        let state = unsafe { &*pointer }.lock();
        let name = unsafe { CStr::from_char_ptr((*attribute).attr.name) };
        let mut output = Output::new();
        read_control(name.to_bytes(), &state, &mut output)?;
        // SAFETY: sysfs supplies PAGE_SIZE bytes; formatter produces at most 512.
        unsafe { core::ptr::copy_nonoverlapping(output.bytes.as_ptr(), buffer.cast(), output.len) };
        Ok(output.len as isize)
    })();
    result.unwrap_or_else(|error: Error| error.to_errno() as isize)
}

unsafe extern "C" fn store(
    _: *mut bindings::device,
    attribute: *mut bindings::device_attribute,
    buffer: *const kernel::ffi::c_char,
    count: usize,
) -> isize {
    let result = (|| {
        #[cfg(aps_dev)]
        let maximum=(dev_probe::wire::TEXT_LEN+1).max(interface::MAX_TEXT);
        #[cfg(not(aps_dev))]
        let maximum=interface::MAX_TEXT;
        if count == 0 || count > maximum {
            return Err(EINVAL);
        }
        let pointer = STATE.load(Ordering::Acquire);
        if pointer.is_null() {
            return Err(ENODEV);
        }
        // SAFETY: sysfs supplies count initialized bytes, bounded independently above.
        // Active callbacks keep registration and state alive through removal.
        let input = unsafe { core::slice::from_raw_parts(buffer.cast(), count) };
        let name = unsafe { CStr::from_char_ptr((*attribute).attr.name) };
        // Enforce administrative privilege independently of sysfs modes, ACLs,
        // or a writable fd passed from root. capable() checks the initial user
        // namespace, so container/user-namespace root is not sufficient.
        if matches!(name.to_bytes(), b"gpu_mux_mode" | b"boot_effects" | b"battery_charge_limit" | b"battery_calibration")
            && !unsafe { bindings::capable(bindings::CAP_SYS_ADMIN as i32) } {
            return Err(EPERM);
        }
        let mut state = unsafe { &*pointer }.lock();
        // Includes signed development probes. Nothing may relight the controller
        // between PREPARE and userspace freezing, or access suspended devices.
        if state.sleep.sleeping { return Err(EBUSY); }
        #[cfg(aps_dev)]
        if name.to_bytes()==b"dev_probe" {
            let result=run_probe(input,&mut state);
            state.developer.last_error=result.as_ref().err().map_or(0,|e|e.to_errno());
            schedule_pending(&state);
            result?;
            return Ok(count as isize);
        }
        if count>128 && name.to_bytes()!=b"keyboard_lighting" { return Err(EINVAL); }
        let result = write_control(name.to_bytes(), input, &mut state);
        schedule_pending(&state);
        result?;
        // Boot GPU selection belongs to firmware, never the lighting/profile
        // preference file. Do not cause aps-state to capture/replay it.
        if !matches!(name.to_bytes(), b"gpu_mux_mode" | b"boot_effects" | b"battery_charge_limit" | b"battery_calibration") {
            state.generation = state.generation.wrapping_add(1);
        }
        Ok(count as isize)
    })();
    result.unwrap_or_else(|error: Error| error.to_errno() as isize)
}

#[cfg(aps_dev)]
fn run_probe(input:&[u8],state:&mut State)->Result<()> {
    require_open_lid(state)?;
    let probe=state.developer.authenticate(input,now())?;
    if probe.operation==0 { return Ok(()); }
    if probe.operation==4 {
        state.developer.active=false;
        // Ending a dev session does not mark raw writes as persistent changes.
        pr_info!("acer_phn16s: developer session ended\n");
        return Ok(());
    }
    // Even apparent reads can have undocumented side effects. Freeze automatic
    // preference capture until the developer explicitly finishes the session.
    state.developer.active=true;
    pr_info!("acer_phn16s: signed raw probe seq={} transport={} action={} method={} len={}\n",
        probe.sequence,probe.operation,probe.action,probe.method,probe.length);
    let result=match probe.operation {
        1|2=>hardware_hid::raw_probe(&probe),
        3=>acpi_controls::raw_probe(&probe),
        _=>Err(EINVAL),
    };
    let (response,len)=result?;
    state.developer.response=response; state.developer.response_len=len;
    Ok(())
}

fn sleep_event(prepare: bool, suspend: bool) {
    let pointer = STATE.load(Ordering::Acquire);
    if pointer.is_null() { return; }
    // SAFETY: PM registration is removed and drained before STATE is freed.
    let mut state = unsafe { &*pointer }.lock();
    if prepare {
        if !state.sleep.prepare() { return; }
        events::pause();
        turbo_button::discard_presses();
        events::arm(None, now());
        state.logo_recovery.cancel();
        // These temporary hardware writes deliberately bypass cache/generation
        // updates. Userspace persistence continues to see the intended colors.
        let lighting = sleep_state::SleepLighting::new(state.sleep_logo, suspend);
        let mut error = 0;
        for (component, result) in [
            ("keyboard", hardware_hid::set_light(lighting.keyboard)),
            ("cover_lighting", hardware_hid::set_logo(lighting.logo)),
            ("turbo", hardware_hid::set_turbo_led(lighting.turbo)),
        ] {
            if let Err(e) = result {
                error = e.to_errno();
                pr_err!("acer_phn16s: sleep {} setting failed: {}\n", component, error);
                if component == "cover_lighting" && lighting.logo != control::Logo::Off {
                    if let Err(fallback) = hardware_hid::set_logo(control::Logo::Off) {
                        pr_err!("acer_phn16s: sleep logo off fallback failed: {}\n", fallback.to_errno());
                    }
                }
            }
        }
        state.sleep.error = error;
        if state.keyboard_zones.iter().any(Option::is_none)
            || state.logo.is_none() || state.turbo_led.is_none() {
            pr_warn!("acer_phn16s: sleep: uncached RGB settings cannot be recovered; only configured settings will resume\n");
        }
        let logo_action = if lighting.logo == control::Logo::Off { "off" } else { "configured sleep effect" };
        pr_info!("acer_phn16s: sleep lighting submitted: logo={}, internal LEDs off, error={}\n", logo_action, error);
    } else if state.sleep.resume(now()) {
        turbo_button::discard_presses();
        events::resume();
        pr_info!("acer_phn16s: wake RGB restore queued after 2s settling\n");
    }
    schedule_pending(&state);
}

fn schedule_pending(state: &State) {
    events::arm(event_policy::deadline(state.sleep.sleeping, state.sleep.deadline(),
        state.lid.deadline(), state.lid.allowed(), state.logo_recovery.deadline(),
        state.logo.is_some()), now());
}

fn refresh_lid(state: &mut State) {
    // Called only with State locked and outside PM suspension. The ACPI core
    // owns the lid handle and serializes its lifetime. No input-event injection.
    let value = unsafe { acpi_lid_open() };
    if state.lid.observe(value, now()) {
        pr_info!("acer_phn16s: lid {} (sensor result={})\n", state.lid.name(), value);
    }
}

fn require_open_lid(state: &mut State) -> Result<()> {
    refresh_lid(state);
    match state.lid.open {
        Some(true) => Ok(()),
        Some(false) => Err(Error::from_errno(-(bindings::EHOSTDOWN as i32))),
        None => Err(EAGAIN),
    }
}

fn set_profile_with_lid(profile: u8, state: &mut State) -> Result<()> {
    let result = acpi_controls::set_profile(profile);
    let actual = if result.is_ok() { Some(u64::from(profile)) }
        else { acpi_controls::profile().ok() };
    state.logo_recovery.profile_changed(actual, now());
    refresh_lid(state);
    if !state.lid.allowed() {
        // Performance writes can relight the Turbo LED even on a reported error.
        let error = apply_internal_rgb(state);
        state.lid.completed(now(), error);
        // Firmware also overlays a delayed mode indication. Reassert off later.
        state.lid.request(now().saturating_add(2_000_000_000));
    }
    result
}

fn apply_internal_rgb(state: &State) -> i32 {
    let mut error = 0;
    let reports = if state.lid.allowed() {
        sleep_state::keyboard_reports(state.keyboard_zones)
    } else {
        [Some(control::Light { mode: 1, brightness: 0, speed: 0, direction: 0,
            zones: 15, rgb: [0; 3] }), None, None, None]
    };
    for setting in reports.into_iter().flatten() {
        if let Err(e) = hardware_hid::set_light(setting) {
            error = e.to_errno();
            pr_err!("acer_phn16s: keyboard lid/restore write failed: {}\n", error);
        }
    }
    let turbo = if state.lid.allowed() { state.turbo_led } else { Some(control::TurboLed::Off) };
    if let Some(setting) = turbo {
        if let Err(e) = hardware_hid::set_turbo_led(setting) {
            error = e.to_errno();
            pr_err!("acer_phn16s: Turbo lid/restore write failed: {}\n", error);
        }
    }
    error
}

fn restore_rgb(state: &mut State) {
    refresh_lid(state);
    let mut error = 0;
    if let Err(e) = hardware_hid::set_logo(sleep_state::awake_logo(state.logo)) {
        error = e.to_errno();
        pr_err!("acer_phn16s: wake logo restore failed: {}\n", error);
    }
    // Firmware can overlay the Turbo LED during power transitions. Restore it
    // last, after the settling delay, using the current auto/manual choice.
    let internal_error = apply_internal_rgb(state);
    if internal_error != 0 { error = internal_error; }
    state.lid.completed(now(), internal_error);
    state.sleep.completed(now(), error);
    pr_info!("acer_phn16s: wake RGB restore attempt={} status={} error={}\n",
        state.sleep.attempts, state.sleep.phase(), error);
}

fn recover_eco_logo(state: &mut State) {
    #[cfg(aps_dev)]
    if state.developer.active {
        state.logo_recovery.cancel();
        return;
    }
    let busy = state.sleep.sleeping || state.sleep.phase() == "restoring";
    let Some(setting) = state.logo_recovery.setting(now(), state.logo, busy) else { return; };
    // Confirm the current mode before replay, including changes outside APS.
    let result = match acpi_controls::profile() {
        Ok(6) => hardware_hid::set_logo(setting),
        Ok(_) => { state.logo_recovery.cancel(); return; }
        Err(error) => Err(error),
    };
    state.logo_recovery.completed(now(), result.is_ok());
    // Reapplication must not change preferences or advance their generation.
    pr_info!("acer_phn16s: Eco logo recovery attempt={} error={}\n",
        state.logo_recovery.attempts, result.err().map_or(0, |e| e.to_errno()));
}

fn run_work() {
    let pointer = STATE.load(Ordering::Acquire);
    if pointer.is_null() { return; }
    // SAFETY: Work is drained before clearing/freeing the pinned State.
    let mut guard = unsafe { &*pointer }.lock();
    if !events::accepting() || guard.sleep.sleeping {
        turbo_button::discard_presses();
        return;
    }
    if events::take_lid_change() { refresh_lid(&mut guard); }
    for _ in 0..8 {
        if turbo_button::take_sense_press() {
            if let Err(error) = turbo_button::emit_f15() {
                pr_err!("acer_phn16s: PredatorSense key action failed: {}\n", error.to_errno());
            }
        }
        if turbo_button::take_press() {
            let result = match guard.button_mode {
                control::ButtonMode::Hotkey => turbo_button::emit_f14(),
                control::ButtonMode::PowerModes => {
                    acpi_controls::profile().and_then(|current| {
                        let next = control::next_profile(current, turbo_button::on_ac()).ok_or(EIO)?;
                        set_profile_with_lid(next, &mut guard)
                    })
                }
            };
            guard.button_error = match result {
                Ok(()) => {
                    if guard.button_mode == control::ButtonMode::PowerModes {
                        guard.generation = guard.generation.wrapping_add(1);
                    }
                    0
                }
                Err(error) => error.to_errno(),
            };
            if guard.button_error != 0 {
                pr_err!("acer_phn16s: Turbo button action failed: {}\n", guard.button_error);
            }
        }
    }
    if guard.sleep.ready(now()) {
        restore_rgb(&mut guard);
    } else if guard.lid.ready(now())
        && (!guard.lid.allowed() || guard.sleep.phase() != "restoring") {
        let error = apply_internal_rgb(&guard);
        guard.lid.completed(now(), error);
        pr_info!("acer_phn16s: lid {} lighting applied, error={}\n", guard.lid.name(), error);
    }
    recover_eco_logo(&mut guard);
    schedule_pending(&guard);
    if turbo_button::pending() { events::queue(); }
}

struct AcerModule {
    device: Pin<KBox<MiscDeviceRegistration<Anchor>>>,
    state: Pin<KBox<Mutex<State>>>,
    events: Option<events::Registration>,
    created: usize,
    button: Option<turbo_button::Registration>,
    sense: Option<sense_key::Registration>,
    sleep: Option<sleep::Registration>,
}
impl kernel::Module for AcerModule {
    fn init(this_module: &'static ThisModule) -> Result<Self> {
        if !matches_dmi(6, c"Acer")
            || !matches_dmi(7, c"Predator PHN16S-71")
            || !matches_dmi(14, c"FTYPE_ARX")
            || !matches_dmi(2, c"V1.28")
        {
            return Err(ENODEV);
        }
        let state = KBox::pin_init(
            new_mutex!(State {
                #[cfg(aps_dev)]
                developer: dev_probe::Developer::new(),
                last_light_ns: 0,
                button_mode: control::ButtonMode::PowerModes,
                button_error: 0,
                turbo_led: None,
                turbo_led_manual: control::TurboLed::Rgb([255; 3]),
                logo: None,
                sleep_logo: control::Logo::Off,
                keyboard_zones: [None; 4],
                generation: 0,
                sleep: sleep_state::SleepState::default(),
                logo_recovery: logo_recovery::LogoRecovery::default(),
                lid: lid_state::LidState::default(),
                mux_request: None,
                mux_last_ns: None,
                boot_last_ns: None,
            }),
            GFP_KERNEL,
        )?;
        let device = KBox::try_pin_init(
            MiscDeviceRegistration::register(MiscDeviceOptions {
                name: c"acer-phn16s",
            }),
            GFP_KERNEL,
        )?;
        let mut module = Self {
            device,
            state,
            events: None,
            created: 0,
            button: None,
            sense: None,
            sleep: None,
        };
        let pointer = module.state.as_ref().get_ref() as *const Mutex<State> as *mut Mutex<State>;
        STATE.store(pointer, Ordering::Release);
        module.events = Some(events::Registration::new()?);
        module.sleep = Some(sleep::Registration::new()?);
        module.button = Some(turbo_button::Registration::new(this_module)?);
        module.sense = Some(sense_key::Registration::new()?);
        for attribute in ATTRS {
            // SAFETY: Device is initialized and stable; attributes remain static until removal.
            let rc = unsafe {
                bindings::device_create_file((*module.device.as_raw()).this_device, &attribute.0)
            };
            if rc < 0 {
                return Err(Error::from_errno(rc));
            }
            module.created += 1;
        }
        events::lid_changed();
        pr_info!("acer_phn16s: typed sysfs controls ready; event-driven key/lid work active\n");
        #[cfg(aps_dev)]
        pr_warn!("acer_phn16s: DEVELOPMENT BUILD: signed raw firmware calls enabled\n");
        Ok(module)
    }
}
impl Drop for AcerModule {
    fn drop(&mut self) {
        // Serialize STOPPING with running work and deadline updates.
        { let _state = self.state.lock(); events::stop(); }
        // Drain PM callbacks before tearing down the worker, devices or state.
        // No State lock may be held while unregister waits for callbacks.
        drop(self.sleep.take());
        for attribute in ATTRS[..self.created].iter().rev() {
            // SAFETY: Remove exactly the registered files; kernel drains active callbacks.
            unsafe {
                bindings::device_remove_file((*self.device.as_raw()).this_device, &attribute.0)
            };
        }
        drop(self.sense.take());
        if let Some(button) = &mut self.button { button.detach(); }
        // Producers are drained; work must stop before the input device is freed.
        drop(self.events.take());
        drop(self.button.take());
        STATE.store(core::ptr::null_mut(), Ordering::Release);
    }
}
