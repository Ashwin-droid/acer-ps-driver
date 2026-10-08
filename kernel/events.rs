// SPDX-License-Identifier: GPL-2.0-only
//! Callback-triggered work plus one-shot RGB deadlines. No private thread/polling.
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, Ordering},
};
use kernel::{bindings, prelude::*};

struct Key(UnsafeCell<bindings::lock_class_key>);
// SAFETY: Keys have static addresses; only the kernel lockdep helpers access them.
unsafe impl Sync for Key {}
static EVENT_KEY: Key = Key(UnsafeCell::new(unsafe { core::mem::zeroed() }));
static DELAY_KEY: Key = Key(UnsafeCell::new(unsafe { core::mem::zeroed() }));
static TIMER_KEY: Key = Key(UnsafeCell::new(unsafe { core::mem::zeroed() }));

static ENGINE: AtomicPtr<Engine> = AtomicPtr::new(core::ptr::null_mut());
static STOPPING: AtomicBool = AtomicBool::new(true);
static PAUSED: AtomicBool = AtomicBool::new(false);
static LID_DIRTY: AtomicBool = AtomicBool::new(false);
static RUNS: AtomicU64 = AtomicU64::new(0);
static TIMER_RUNS: AtomicU64 = AtomicU64::new(0);
struct Engine {
    immediate: UnsafeCell<bindings::work_struct>,
    delayed: UnsafeCell<bindings::delayed_work>,
}
// SAFETY: Work objects stay in one allocation until all producers and queued
// work are drained. Kernel workqueue APIs synchronize their internal fields.
unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}
pub(crate) struct Registration {
    engine: KBox<Engine>,
}

unsafe extern "C" fn immediate(_: *mut bindings::work_struct) {
    RUNS.fetch_add(1, Ordering::Relaxed);
    if accepting() {
        crate::run_work();
    }
}
unsafe extern "C" fn delayed(_: *mut bindings::work_struct) {
    TIMER_RUNS.fetch_add(1, Ordering::Relaxed);
    queue();
}
pub(crate) fn accepting() -> bool {
    !STOPPING.load(Ordering::Acquire) && !PAUSED.load(Ordering::Acquire)
}
pub(crate) fn queue() {
    if !accepting() {
        return;
    }
    let engine = ENGINE.load(Ordering::Acquire);
    if engine.is_null() {
        return;
    }
    // SAFETY: Callers are registered HID/input/PM/sysfs callbacks or our work
    // items. Unload stops and drains every producer before freeing Engine.
    // queue_work_on is valid in atomic context and does not perform device I/O.
    unsafe {
        bindings::queue_work_on(
            bindings::wq_misc_consts_WORK_CPU_UNBOUND as i32,
            bindings::system_unbound_wq,
            (*engine).immediate.get(),
        );
    }
}
pub(crate) fn lid_changed() {
    LID_DIRTY.store(true, Ordering::Release);
    queue();
}
pub(crate) fn take_lid_change() -> bool {
    LID_DIRTY.swap(false, Ordering::AcqRel)
}
pub(crate) fn pause() {
    PAUSED.store(true, Ordering::Release);
}
pub(crate) fn resume() {
    LID_DIRTY.store(true, Ordering::Release);
    PAUSED.store(false, Ordering::Release);
    queue();
}
// Called with driver State locked, so competing setters cannot move an older
// deadline over a newer one. Non-sync cancellation cannot deadlock with work
// waiting for that same mutex. A racing expired timer can cause one harmless
// extra event pass, never recurring idle activity.
pub(crate) fn arm(deadline: Option<u64>, now: u64) {
    let engine = ENGINE.load(Ordering::Acquire);
    if engine.is_null() {
        return;
    }
    unsafe {
        let work = (*engine).delayed.get();
        if let Some(due) = deadline.filter(|_| accepting()) {
            let ms = due.saturating_sub(now).saturating_add(999_999) / 1_000_000;
            let ticks = bindings::__msecs_to_jiffies(ms.min(u32::MAX as u64) as u32);
            bindings::mod_delayed_work_on(
                bindings::wq_misc_consts_WORK_CPU_UNBOUND as i32,
                bindings::system_unbound_wq,
                work,
                ticks,
            );
        } else {
            bindings::cancel_delayed_work(work);
        }
    }
}
pub(crate) fn stop() {
    STOPPING.store(true, Ordering::Release);
}
pub(crate) fn counters() -> (u64, u64) {
    (
        RUNS.load(Ordering::Relaxed),
        TIMER_RUNS.load(Ordering::Relaxed),
    )
}
impl Registration {
    pub(crate) fn new() -> Result<Self> {
        let engine = KBox::new(
            Engine {
                immediate: UnsafeCell::new(unsafe { core::mem::zeroed() }),
                delayed: UnsafeCell::new(unsafe { core::mem::zeroed() }),
            },
            GFP_KERNEL,
        )?;
        // SAFETY: Fresh stable allocations, unpublished until initialization.
        // This is INIT_WORK / INIT_DELAYED_WORK via the target kernel helpers,
        // including debug-object initialization and static lockdep class keys.
        unsafe {
            bindings::init_work_with_key(
                engine.immediate.get(),
                Some(immediate),
                false,
                c"aps-events".as_char_ptr(),
                EVENT_KEY.0.get(),
            );
            let dwork = engine.delayed.get();
            bindings::init_work_with_key(
                core::ptr::addr_of_mut!((*dwork).work),
                Some(delayed),
                false,
                c"aps-deadline".as_char_ptr(),
                DELAY_KEY.0.get(),
            );
            bindings::timer_init_key(
                core::ptr::addr_of_mut!((*dwork).timer),
                Some(bindings::delayed_work_timer_fn),
                bindings::TIMER_IRQSAFE,
                c"aps-deadline-timer".as_char_ptr(),
                TIMER_KEY.0.get(),
            );
        }
        ENGINE.store((&*engine as *const Engine).cast_mut(), Ordering::Release);
        PAUSED.store(false, Ordering::Release);
        STOPPING.store(false, Ordering::Release);
        Ok(Self { engine })
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        stop();
        // SAFETY: Module teardown has drained every external producer. No State
        // mutex is held. STOPPING prevents callbacks from rearming either item.
        unsafe {
            bindings::cancel_delayed_work_sync(self.engine.delayed.get());
            bindings::cancel_work_sync(self.engine.immediate.get());
        }
        ENGINE.store(core::ptr::null_mut(), Ordering::Release);
    }
}
