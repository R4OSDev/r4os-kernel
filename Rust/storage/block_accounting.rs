//! Original per-device and shared controller/IRQ accounting.
//! Every mutation and snapshot uses the short storage metadata owner. No
//! guard or Rust reference survives a backend call, wake, free or wait.
use crate::memory::owner::Guard;
use crate::storage::block_requests::Snapshot;
use core::{cell::UnsafeCell, ptr};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Sense {
    pub valid: bool,
    pub opcode: u8,
    pub key: u8,
    pub asc: u8,
    pub ascq: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub next_request_id: u64,
    pub completions: u64,
    pub busy_rejections: u64,
    pub timeout_failures: u64,
    pub active_request: Snapshot,
    pub last_request: Snapshot,
    pub queued_requests: u64,
    pub dequeued_requests: u64,
    pub queue_full_waits: u64,
    pub queue_full_rejections: u64,
    pub completion_waits: u64,
    pub completion_timeouts: u64,
    pub completion_total_ticks: u64,
    pub completion_max_ticks: u64,
    pub completion_last_ticks: u64,
    pub completion_signals: u64,
    pub worker_requests: u64,
    pub worker_completions: u64,
    pub boot_inline_requests: u64,
    pub boot_inline_completions: u64,
    pub queue_high_water: u32,
    pub read_ops: u64,
    pub read_sectors: u64,
    pub read_failures: u64,
    pub write_ops: u64,
    pub write_sectors: u64,
    pub write_failures: u64,
    pub flush_ops: u64,
    pub flush_failures: u64,
    pub backend_recoveries: u64,
    pub backend_recovery_failures: u64,
    pub direct_requests: u64,
    pub direct_bytes: u64,
    pub bounce_allocations: u64,
    pub bounce_bytes: u64,
    pub bounce_copy_bytes: u64,
    pub direct_timeout_waits: u64,
    pub async_submissions: u64,
    pub async_completions: u64,
    pub async_cancel_requests: u64,
    pub async_resets: u64,
    pub duplicate_completions: u64,
    pub late_completions: u64,
    pub in_flight_high_water: u32,
    pub last_sense: Sense,
    pub last_error: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RuntimeSummary {
    pub worker_started: u32,
    pub worker_task_id: u32,
    pub worker_wakeups: u64,
    pub worker_runs: u64,
    pub worker_idle_waits: u64,
    pub worker_queue_scans: u64,
    pub worker_runtime_requests: u64,
    pub worker_runtime_completions: u64,
    pub boot_inline_requests: u64,
    pub boot_inline_completions: u64,
    pub completion_signals: u64,
    pub controller_count: u32,
    pub worker_count: u32,
    pub worker_start_failures: u64,
    pub worker_parallel_active: u32,
    pub worker_parallel_active_max: u32,
    pub direct_requests: u64,
    pub direct_bytes: u64,
    pub bounce_allocations: u64,
    pub bounce_bytes: u64,
    pub bounce_copy_bytes: u64,
    pub direct_timeout_waits: u64,
    pub async_submissions: u64,
    pub async_completions: u64,
    pub async_cancel_requests: u64,
    pub async_resets: u64,
    pub duplicate_completions: u64,
    pub late_completions: u64,
    pub in_flight: u32,
    pub in_flight_high_water: u32,
}
struct Runtime(UnsafeCell<RuntimeSummary>);
// Shared lane/IRQ data is serialized; reset retains the exclusive boot boundary.
unsafe impl Sync for Runtime {}
static RUNTIME: Runtime = Runtime(UnsafeCell::new(RuntimeSummary {
    worker_started: 0,
    worker_task_id: 0,
    worker_wakeups: 0,
    worker_runs: 0,
    worker_idle_waits: 0,
    worker_queue_scans: 0,
    worker_runtime_requests: 0,
    worker_runtime_completions: 0,
    boot_inline_requests: 0,
    boot_inline_completions: 0,
    completion_signals: 0,
    controller_count: 0,
    worker_count: 0,
    worker_start_failures: 0,
    worker_parallel_active: 0,
    worker_parallel_active_max: 0,
    direct_requests: 0,
    direct_bytes: 0,
    bounce_allocations: 0,
    bounce_bytes: 0,
    bounce_copy_bytes: 0,
    direct_timeout_waits: 0,
    async_submissions: 0,
    async_completions: 0,
    async_cancel_requests: 0,
    async_resets: 0,
    duplicate_completions: 0,
    late_completions: 0,
    in_flight: 0,
    in_flight_high_water: 0,
}));
struct Sequence(UnsafeCell<u64>);
unsafe impl Sync for Sequence {}
static SEQUENCE: Sequence = Sequence(UnsafeCell::new(0));
fn change(old: u64, op: u8, value: u64) -> u64 {
    match op {
        0 => value,
        1 => old.wrapping_add(value),
        2 => old.saturating_add(value),
        3 => old.saturating_sub(value),
        4 => old.checked_add(value).expect("Block accounting overflow"),
        5 => old.max(value),
        _ => unreachable!(),
    }
}
/// # Safety
/// Exclusive BSP reset, before any worker, IRQ or live request admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_runtime_reset() {
    unsafe {
        ptr::write_bytes(RUNTIME.0.get(), 0, 1);
        SEQUENCE.0.get().write(0);
    }
}
/// # Safety
/// Exclusive aligned C output; no shared reference to canonical state escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_runtime_snapshot(out: *mut RuntimeSummary) {
    let _owner = Guard::acquire(1);
    unsafe {
        out.write(RUNTIME.0.get().read());
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_next_handle(device: u8, slot: usize) -> u64 {
    let _owner = Guard::acquire(1);
    let mut next = unsafe { SEQUENCE.0.get().read() }.wrapping_add(1) & 0x0000_ffff_ffff_ffff;
    if next == 0 {
        next = 1;
    }
    unsafe {
        SEQUENCE.0.get().write(next);
    }
    (next << 16) | ((u64::from(device) + 1) << 8) | (slot as u64 + 1)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_runtime_change(op: u8, field: u8, value: u64) -> u64 {
    let _owner = Guard::acquire(1);
    let data = RUNTIME.0.get();
    match field {
        0 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_started);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        1 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_task_id);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        2 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_wakeups);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        3 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_runs);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        4 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_idle_waits);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        5 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_queue_scans);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        6 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_runtime_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        7 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_runtime_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        8 => unsafe {
            let p = ptr::addr_of_mut!((*data).boot_inline_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        9 => unsafe {
            let p = ptr::addr_of_mut!((*data).boot_inline_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        10 => unsafe {
            let p = ptr::addr_of_mut!((*data).completion_signals);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        11 => unsafe {
            let p = ptr::addr_of_mut!((*data).controller_count);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        12 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_count);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        13 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_start_failures);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        14 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_parallel_active);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        15 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_parallel_active_max);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        16 => unsafe {
            let p = ptr::addr_of_mut!((*data).direct_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        17 => unsafe {
            let p = ptr::addr_of_mut!((*data).direct_bytes);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        18 => unsafe {
            let p = ptr::addr_of_mut!((*data).bounce_allocations);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        19 => unsafe {
            let p = ptr::addr_of_mut!((*data).bounce_bytes);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        20 => unsafe {
            let p = ptr::addr_of_mut!((*data).bounce_copy_bytes);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        21 => unsafe {
            let p = ptr::addr_of_mut!((*data).direct_timeout_waits);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        22 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_submissions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        23 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        24 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_cancel_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        25 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_resets);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        26 => unsafe {
            let p = ptr::addr_of_mut!((*data).duplicate_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        27 => unsafe {
            let p = ptr::addr_of_mut!((*data).late_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        28 => unsafe {
            let p = ptr::addr_of_mut!((*data).in_flight);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        29 => unsafe {
            let p = ptr::addr_of_mut!((*data).in_flight_high_water);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        _ => unreachable!(),
    }
}
/// # Safety
/// Stable initialized C stats, admitted device lifetime and disjoint caller data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_stats_change(
    data: *mut Stats,
    op: u8,
    field: u8,
    value: u64,
) -> u64 {
    let _owner = Guard::acquire(1);
    match field {
        0 => unsafe {
            let p = ptr::addr_of_mut!((*data).next_request_id);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        1 => unsafe {
            let p = ptr::addr_of_mut!((*data).completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        2 => unsafe {
            let p = ptr::addr_of_mut!((*data).busy_rejections);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        3 => unsafe {
            let p = ptr::addr_of_mut!((*data).timeout_failures);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        4 => unsafe {
            let p = ptr::addr_of_mut!((*data).queued_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        5 => unsafe {
            let p = ptr::addr_of_mut!((*data).dequeued_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        6 => unsafe {
            let p = ptr::addr_of_mut!((*data).queue_full_waits);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        7 => unsafe {
            let p = ptr::addr_of_mut!((*data).queue_full_rejections);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        8 => unsafe {
            let p = ptr::addr_of_mut!((*data).completion_waits);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        9 => unsafe {
            let p = ptr::addr_of_mut!((*data).completion_timeouts);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        10 => unsafe {
            let p = ptr::addr_of_mut!((*data).completion_total_ticks);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        11 => unsafe {
            let p = ptr::addr_of_mut!((*data).completion_max_ticks);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        12 => unsafe {
            let p = ptr::addr_of_mut!((*data).completion_last_ticks);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        13 => unsafe {
            let p = ptr::addr_of_mut!((*data).completion_signals);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        14 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        15 => unsafe {
            let p = ptr::addr_of_mut!((*data).worker_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        16 => unsafe {
            let p = ptr::addr_of_mut!((*data).boot_inline_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        17 => unsafe {
            let p = ptr::addr_of_mut!((*data).boot_inline_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        18 => unsafe {
            let p = ptr::addr_of_mut!((*data).queue_high_water);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        19 => unsafe {
            let p = ptr::addr_of_mut!((*data).read_ops);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        20 => unsafe {
            let p = ptr::addr_of_mut!((*data).read_sectors);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        21 => unsafe {
            let p = ptr::addr_of_mut!((*data).read_failures);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        22 => unsafe {
            let p = ptr::addr_of_mut!((*data).write_ops);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        23 => unsafe {
            let p = ptr::addr_of_mut!((*data).write_sectors);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        24 => unsafe {
            let p = ptr::addr_of_mut!((*data).write_failures);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        25 => unsafe {
            let p = ptr::addr_of_mut!((*data).flush_ops);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        26 => unsafe {
            let p = ptr::addr_of_mut!((*data).flush_failures);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        27 => unsafe {
            let p = ptr::addr_of_mut!((*data).backend_recoveries);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        28 => unsafe {
            let p = ptr::addr_of_mut!((*data).backend_recovery_failures);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        29 => unsafe {
            let p = ptr::addr_of_mut!((*data).direct_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        30 => unsafe {
            let p = ptr::addr_of_mut!((*data).direct_bytes);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        31 => unsafe {
            let p = ptr::addr_of_mut!((*data).bounce_allocations);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        32 => unsafe {
            let p = ptr::addr_of_mut!((*data).bounce_bytes);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        33 => unsafe {
            let p = ptr::addr_of_mut!((*data).bounce_copy_bytes);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        34 => unsafe {
            let p = ptr::addr_of_mut!((*data).direct_timeout_waits);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        35 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_submissions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        36 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        37 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_cancel_requests);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        38 => unsafe {
            let p = ptr::addr_of_mut!((*data).async_resets);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        39 => unsafe {
            let p = ptr::addr_of_mut!((*data).duplicate_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        40 => unsafe {
            let p = ptr::addr_of_mut!((*data).late_completions);
            let next = change(p.read(), op, value);
            p.write(next);
            next
        },
        41 => unsafe {
            let p = ptr::addr_of_mut!((*data).in_flight_high_water);
            let next = {
                let v = u32::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        42 => unsafe {
            let p = ptr::addr_of_mut!((*data).last_error);
            let next = {
                let v = u8::try_from(value).expect("Block accounting field width");
                let old = p.read();
                match op {
                    0 => v,
                    1 => old.wrapping_add(v),
                    2 => old.saturating_add(v),
                    3 => old.saturating_sub(v),
                    4 => old.checked_add(v).expect("Block accounting overflow"),
                    5 => old.max(v),
                    _ => unreachable!(),
                }
            };
            p.write(next);
            u64::from(next)
        },
        _ => unreachable!(),
    }
}
/// # Safety
/// Input is stable admitted device stats; output is exclusive disjoint C storage.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_stats_snapshot(data: *const Stats, out: *mut Stats) {
    let _owner = Guard::acquire(1);
    unsafe {
        out.write(data.read());
    }
}
/// # Safety
/// Exclusive boot registration initialization, before the device is published.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_stats_reset(data: *mut Stats) {
    unsafe {
        ptr::write_bytes(data, 0, 1);
    }
}
/// # Safety
/// Stable admitted stats and initialized disjoint C snapshot; no loan escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_stats_request(
    data: *mut Stats,
    last: bool,
    request: *const Snapshot,
) {
    let value = unsafe { request.read() };
    let _owner = Guard::acquire(1);
    unsafe {
        if last {
            ptr::addr_of_mut!((*data).last_request).write(value);
        } else {
            ptr::addr_of_mut!((*data).active_request).write(value);
        }
    }
}
/// # Safety
/// Stable admitted stats; sense scalars retain the original exact values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_stats_sense(
    data: *mut Stats,
    opcode: u8,
    key: u8,
    asc: u8,
    ascq: u8,
) {
    let _owner = Guard::acquire(1);
    unsafe {
        ptr::addr_of_mut!((*data).last_sense).write(Sense {
            valid: true,
            opcode,
            key,
            asc,
            ascq,
        });
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_runtime_active(start: bool) {
    let _owner = Guard::acquire(1);
    let p = RUNTIME.0.get();
    unsafe {
        if start {
            (*p).in_flight = (*p).in_flight.saturating_add(1);
            (*p).in_flight_high_water = (*p).in_flight_high_water.max((*p).in_flight);
        } else {
            (*p).in_flight = (*p).in_flight.saturating_sub(1);
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_runtime_execution(mode: u8, completed: bool) {
    let _owner = Guard::acquire(1);
    let p = RUNTIME.0.get();
    unsafe {
        match (mode, completed) {
            (0, false) => (*p).boot_inline_requests = (*p).boot_inline_requests.wrapping_add(1),
            (0, true) => {
                (*p).boot_inline_completions = (*p).boot_inline_completions.wrapping_add(1)
            }
            (1, false) => {
                (*p).worker_runtime_requests = (*p).worker_runtime_requests.wrapping_add(1);
                (*p).worker_parallel_active = (*p).worker_parallel_active.saturating_add(1);
                (*p).worker_parallel_active_max = (*p)
                    .worker_parallel_active_max
                    .max((*p).worker_parallel_active);
            }
            (1, true) => {
                (*p).worker_runtime_completions = (*p).worker_runtime_completions.wrapping_add(1);
                (*p).worker_parallel_active = (*p).worker_parallel_active.saturating_sub(1);
            }
            _ => unreachable!(),
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<Sense>() == 5);
    assert!(core::mem::size_of::<Stats>() == 408);
    assert!(core::mem::size_of::<RuntimeSummary>() == 208);
};
