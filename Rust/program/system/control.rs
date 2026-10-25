//! Original R4SYS CPU/time/log API and one-owner system transition ordering.
//! Device, display, network and boot-log peers retain their own assigned state;
//! this module owns orchestration and the sole reboot admission flag.
use super::super::registry_format_types::Bytes;
use super::{io, path, registry, registry_state};
use crate::{
    arch::{
        percpu,
        runtime::{self, RuntimeGuard},
        smp,
    },
    boot::foundation as log,
    contract as c,
    fs::page_cache,
    memory::heap,
    platform::{monotonic, time, timer},
    sched::{scheduler, sync_types::WaitQueue, sync_wait, task_context},
};
use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
static REBOOT: AtomicBool = AtomicBool::new(false);

/// # Safety
/// Nullable public output retains original validation. A nonnull output is
/// exclusive caller storage through the bounded capacity snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_cpu_capacity(out: *mut c::CpuCapacity) -> i32 {
    if out.is_null() {
        return c::thread_error_invalid;
    }
    let available = percpu::r4os_rust_percpu_control(15, 0, 0, 0) as u32;
    let configured = smp::r4os_rust_smp_control(6, 0) as u32;
    if available == 0 || available > configured {
        return c::thread_error_busy;
    }
    unsafe {
        out.write(c::CpuCapacity {
            available_cpus: available,
            configured_cpus: configured,
        })
    };
    c::thread_ok
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_sys_yield() {
    scheduler::yield_task();
}
/// # Safety
/// Existing schedulable task context; the local resident timer queue remains
/// alive until the original wait has completed or been cancelled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_sleep(ticks: u64) {
    let mut queue = WaitQueue::EMPTY;
    unsafe { sync_wait::r4os_rust_sync_timer(&mut queue, ticks, b"timer".as_ptr(), 5) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_sys_ticks() -> u64 {
    timer::tick_count()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_sys_seconds() -> u32 {
    time::seconds_since_midnight()
}
/// # Safety
/// Exclusive public output; the platform owns RTC/cache admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_time_state(out: *mut c::TimeState) {
    unsafe { time::r4os_rust_time_state(out) }
}
/// # Safety
/// Readable public input; the platform serializes the whole RTC mutation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_time_set(input: *const c::TimeState) -> i32 {
    unsafe { time::r4os_rust_time_set_state(input) }
}
/// # Safety
/// Original exclusive public monotonic snapshot output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_monotonic(out: *mut c::MonotonicClockInfo) -> i32 {
    unsafe { monotonic::r4os_rust_monotonic_clock_api(out) }
}
unsafe fn boot_info() -> c::BootLogInfo {
    unsafe {
        let mut info = MaybeUninit::uninit();
        sys_bootlog_info(info.as_mut_ptr());
        info.assume_init()
    }
}
/// # Safety
/// Exclusive public output may page fault. The log owner has already released
/// its coherent resident snapshot before this caller-storage publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_bootlog_info(out: *mut c::BootLogInfo) -> i32 {
    let info = unsafe { boot_info() };
    unsafe { out.write(info) };
    1
}
struct Allocation {
    pointer: *mut u8,
    len: usize,
}
impl Drop for Allocation {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            unsafe { heap::r4os_rust_heap_free(self.pointer, self.len) };
        }
    }
}
struct Unwind(task_context::Unwind);
impl Drop for Unwind {
    fn drop(&mut self) {
        let _runtime = RuntimeGuard::acquire();
        unsafe {
            assert!(task_context::leave(self.0));
        }
    }
}
/// # Safety
/// Original caller output and admitted task lifetime. The ring owner receives
/// resident private scratch only; the final caller copy occurs after release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_bootlog_read(
    offset: u32,
    out: *mut u8,
    capacity: u32,
) -> i32 {
    unsafe {
        if capacity == 0 {
            return 0;
        }
        let len = boot_info().length as usize;
        let offset = offset as usize;
        if offset >= len {
            return 0;
        }
        let max = (capacity as usize).min(len - offset);
        let token = {
            let _runtime = RuntimeGuard::acquire();
            task_context::enter()
        };
        if !token.admitted() {
            return -1;
        }
        let _unwind = Unwind(token);
        let mut small = MaybeUninit::<[u8; 4096]>::uninit();
        let allocated = Allocation {
            pointer: if max > 4096 {
                heap::r4os_rust_heap_alloc(max, 1)
            } else {
                ptr::null_mut()
            },
            len: max,
        };
        if max > 4096 && allocated.pointer.is_null() {
            return -1;
        }
        let resident = if max > 4096 {
            allocated.pointer
        } else {
            small.as_mut_ptr().cast()
        };
        let written = sys_bootlog_read(false, offset, resident, max);
        ptr::copy_nonoverlapping(resident, out, written);
        i32::try_from(written).expect("Boot log read result")
    }
}
unsafe fn log_registry() {
    unsafe {
        let mut performance = MaybeUninit::uninit();
        let mut generation = 0;
        let mut dirty = false;
        let mut pending = false;
        if !registry_state::r4os_rust_sys_registry_performance(
            performance.as_mut_ptr(),
            &mut generation,
            &mut dirty,
            &mut pending,
        ) {
            return;
        }
        let performance = performance.assume_init();
        log::puts(b"[R4SYS] registry generation=");
        log::dec(generation);
        for (label, value) in [
            (b" reads=".as_slice(), performance.read_calls),
            (b" cacheHits=", performance.cache_hits),
            (b" fileLoads=", performance.file_loads),
            (b" validations=", performance.validation_passes),
            (b" publications=", performance.publications),
            (b" commits=", performance.commits),
            (b" failures=", performance.commit_failures),
            (b" retries=", performance.atomic_retries),
        ] {
            log::puts(label);
            log::dec(value);
        }
        log::puts(b" dirty=");
        log::puts(if dirty { b"yes" } else { b"no" });
        log::puts(b" pending=");
        log::puts(if pending { b"yes" } else { b"no" });
        log::puts(b"\r\n");
    }
}
unsafe fn flush_registry() {
    unsafe {
        if registry::r4os_rust_sys_registry_flush() != c::registry_api_result_ok {
            log::puts(b"R4SYS shutdown: registry writeback flush failed\r\n");
        }
        log_registry();
    }
}
unsafe fn flush_page_cache() {
    if !unsafe { page_cache::r4os_rust_pagecache_flush_all() } {
        log::puts(b"R4SYS shutdown: page-cache flush failed\r\n");
    }
}
unsafe fn save_log(stage: &[u8]) {
    unsafe {
        log::puts(b"[RESET] checkpoint=");
        log::puts(stage);
        log::puts(b" kernel=");
        log::puts(sys_version().slice());
        log::puts(b" ticks=");
        log::dec(timer::tick_count());
        log::puts(b"\r\n");
        let len = boot_info().capacity as usize;
        let allocated = Allocation {
            pointer: heap::r4os_rust_heap_alloc(len, 1),
            len,
        };
        if allocated.pointer.is_null() {
            log::puts(b"[RESET] diagnostic allocation failed\r\n");
            return;
        }
        let count = sys_bootlog_read(true, 0, allocated.pointer, len);
        let mut path = [0u8; 19];
        path::write_z(&mut path, b"C:\\TEMP\\REBOOT.LOG");
        let written = io::r4os_rust_sys_file_write(path.as_ptr(), allocated.pointer, count as u32);
        let durable = page_cache::r4os_rust_pagecache_flush_all();
        if written != i32::try_from(count).expect("Reboot log result") || !durable {
            log::puts(b"[RESET] diagnostic write/flush failed\r\n");
        }
    }
}
/// # Safety
/// Original terminal system-halt caller; registry remains active until flush.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_halt() {
    let Some(transition) = super::cpu_power::prepare_immediate() else {
        return;
    };
    unsafe { halt_ready(transition) }
}
unsafe fn halt_ready(transition: crate::platform::cpu_power::SystemTransition) -> ! {
    log::puts(b"System halted.\r\n");
    unsafe { flush_registry() };
    transition.commit();
    runtime::halt_forever()
}
/// # Safety
/// Original explicit system reboot. One atomic owner orchestrates peer drains;
/// only genuine callbacks/driver/display acknowledgements admit warm reset.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_reboot() {
    unsafe {
        if REBOOT.swap(true, Ordering::AcqRel) {
            return;
        }
        let Some(transition) = super::cpu_power::prepare_immediate() else {
            REBOOT.store(false, Ordering::Release);
            return;
        };
        reboot_ready(transition)
    }
}
unsafe fn reboot_ready(transition: crate::platform::cpu_power::SystemTransition) -> ! {
    unsafe {
        log::puts(b"System reboot.\r\n");
        flush_registry();
        flush_page_cache();
        sys_power_transition(0, 0, Bytes::EMPTY);
        save_log(b"begin");
        let display =
            sys_power_transition(1, u64::from((timer::frequency() / 4).max(1)), Bytes::EMPTY);
        log::puts(if display {
            b"[RESET] display-drain=OK\r\n"
        } else {
            b"[RESET] display-drain=FAILED\r\n"
        });
        save_log(b"display-drained");
        let network = sys_power_transition(2, 0, Bytes::from_slice(b"reboot"));
        log::puts(if network {
            b"[RESET] network-drain=OK\r\n"
        } else {
            b"[RESET] network-drain=FAILED\r\n"
        });
        save_log(b"network-drained");
        let drivers = display && network && sys_power_transition(3, 0, Bytes::EMPTY);
        let released = drivers && sys_power_transition(4, 0, Bytes::EMPTY);
        log::puts(if drivers {
            b"[RESET] driver-shutdown=OK\r\n"
        } else {
            b"[RESET] driver-shutdown=FAILED-OR-SKIPPED\r\n"
        });
        log::puts(if released {
            b"[RESET] display-release=OK\r\n"
        } else {
            b"[RESET] display-release=FAILED-OR-SKIPPED\r\n"
        });
        if !display || !network || !drivers || !released {
            log::puts(
                b"[RESET][ERROR] device handoff unsafe; powering off instead of warm reset\r\n",
            );
            save_log(b"poweroff-device-handoff");
            crate::boot::log_route::serial_flush();
            transition.commit();
            crate::arch::terminal_power::poweroff();
        }
        save_log(b"warm-reset-ready");
        crate::boot::log_route::serial_flush();
        transition.commit();
        crate::arch::terminal_power::reboot();
    }
}
/// # Safety
/// Original explicit terminal poweroff caller, after required persistent flushes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_sys_poweroff() {
    let Some(transition) = super::cpu_power::prepare_immediate() else {
        return;
    };
    unsafe { poweroff_ready(transition) }
}
unsafe fn poweroff_ready(transition: crate::platform::cpu_power::SystemTransition) -> ! {
    unsafe {
        log::puts(b"System poweroff.\r\n");
        flush_registry();
        flush_page_cache();
        crate::boot::log_route::serial_flush();
        transition.commit();
        crate::arch::terminal_power::poweroff()
    }
}

/// # Safety
/// Canonical optional R4SYS call in an ordinary BSP task with IF/fault
/// admission. The explicit wait budget is bounded; refusal preserves resources
/// and returns without stopping peers. Success becomes terminal and never returns.
pub(crate) unsafe extern "C" fn power_transition(action: u32, timeout_ticks: u64) -> i32 {
    if action > c::power_transition_action_poweroff {
        return c::power_transition_error_invalid;
    }
    if !crate::arch::cpu_lifecycle::caller_context() {
        return c::power_transition_error_context;
    }
    let Some(_caller) = task_context::Admission::acquire(true) else {
        return c::power_transition_error_context;
    };
    let transition = match super::cpu_power::prepare(timeout_ticks) {
        Ok(value) => value,
        Err(reason) => return reason,
    };
    unsafe {
        match action {
            c::power_transition_action_halt => halt_ready(transition),
            c::power_transition_action_reboot => {
                if REBOOT.swap(true, Ordering::AcqRel) {
                    return c::power_transition_error_busy;
                }
                reboot_ready(transition)
            }
            c::power_transition_action_poweroff => poweroff_ready(transition),
            _ => unreachable!(),
        }
    }
}

unsafe fn sys_bootlog_info(out: *mut c::BootLogInfo) {
    unsafe {
        let mut raw = core::mem::MaybeUninit::uninit();
        crate::boot::log::r4os_rust_bootlog_info(raw.as_mut_ptr());
        let v = raw.assume_init();
        out.write(c::BootLogInfo {
            capacity: crate::boot::log::CAPACITY.try_into().unwrap(),
            length: v.length.try_into().unwrap(),
            flags: v.flags,
            reserved: 0,
            total_written: v.total_written,
            dropped_bytes: v.dropped_bytes,
        });
    }
}

unsafe fn sys_bootlog_read(snapshot: bool, offset: usize, out: *mut u8, len: usize) -> usize {
    unsafe { crate::boot::log::r4os_rust_bootlog_read(if snapshot { 0 } else { offset }, out, len) }
}

unsafe fn sys_version() -> Bytes {
    let text = crate::build_config::config::KERNEL_VERSION_TEXT;
    Bytes {
        data: text.as_ptr(),
        len: text.len(),
    }
}

unsafe fn sys_power_transition(operation: u8, ticks: u64, reason: Bytes) -> bool {
    unsafe {
        match operation {
            0 => {
                crate::arch::terminal_power::r4os_rust_terminal_power_status();
                true
            }
            1 => crate::display::manager::begin_system_transition(ticks),
            2 => crate::net::cleanup::r4os_rust_net_cleanup_transition(
                crate::sched::sync_types::Text {
                    pointer: reason.data,
                    len: reason.len,
                },
            ),
            3 => crate::loader::r4d::r4os_rust_r4d_transition(),
            4 => crate::display::manager::system_transition_quiesced(),
            _ => panic!("Invalid system transition"),
        }
    }
}
