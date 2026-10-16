//! Existing instrumentation availability, named locks, stack groups, Work,
//! PCI, input and IRQ timing. Baseline zero timings retain their mode flags.
use super::{providers, snapshot};
use crate::{
    arch::runtime,
    boot::instrumentation as mode,
    contract as c,
    driver::{irq_router, work},
    input::{i8042, keyboard},
    memory::owner,
    platform::{monotonic, pci_inventory},
    program::{diagnostics, stack},
    sched::task::{task_registry, task_resources, task_types::Role},
};
use core::mem;

/// # Safety
/// Caller owns the header and advertised canonical instrumentation output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_instrumentation(
    out: *mut c::ProgramInstrumentationInfo,
) -> i32 {
    unsafe {
        if (*out).version < 1
            || (*out).size < mem::size_of::<c::ProgramInstrumentationInfo>() as u32
        {
            return -1;
        }
        let clock = monotonic::snapshot();
        out.write(c::ProgramInstrumentationInfo {
            mode: u32::from(mode::DETAILED),
            service_lock_stride: if mode::DETAILED { 64 } else { 0 },
            work_critical_stride: if mode::DETAILED {
                mode::WORK_CRITICAL_STRIDE
            } else {
                0
            },
            lock_count: (1 + owner::CLASS_NAMES.len()) as u32,
            stack_group_count: (task_registry::ROLE_COUNT + stack::PROFILE_COUNT) as u32,
            clock_source: clock.source,
            clock_generation: clock.generation,
            clock_flags: clock.flags,
            clock_resolution_ns: clock.resolution_ns,
            ..Default::default()
        });
    }
    1
}
/// # Safety
/// Caller owns the header and advertised canonical lock-performance record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_lock(
    index: u32,
    out: *mut c::ProgramLockPerformanceInfo,
) -> i32 {
    unsafe {
        if (*out).version < 1
            || (*out).size < mem::size_of::<c::ProgramLockPerformanceInfo>() as u32
        {
            return -1;
        }
        if index as usize > owner::CLASS_NAMES.len() {
            return 0;
        }
        let mut value = c::ProgramLockPerformanceInfo {
            index,
            flags: u32::from(mode::DETAILED),
            ..Default::default()
        };
        if index == 0 {
            let s = snapshot(runtime::r4os_rust_runtime_stats);
            value.flags |= 2;
            value.acquisitions = s.acquisitions;
            value.nested_acquisitions = s.nested_acquisitions;
            value.collisions = s.collisions;
            value.cpu_collisions = s.cpu_collisions;
            value.wait_spins = s.wait_spins;
            value.max_wait_spins = s.max_wait_spins;
            value.hold_cycles = s.hold_cycles;
            value.max_hold_cycles = s.max_hold_cycles;
            value.name[..7].copy_from_slice(b"runtime");
        } else {
            let class = (index - 1) as u8;
            let s = owner::stats_for_class(class);
            value.flags |= 4;
            value.acquisitions = s.acquisitions;
            value.nested_acquisitions = s.nested_acquisitions;
            value.collisions = s.collisions;
            value.wait_spins = s.wait_spins;
            value.max_wait_spins = s.max_wait_spins;
            value.hold_cycles = s.hold_cycles;
            value.max_hold_cycles = s.max_hold_cycles;
            value.order_violations = s.order_violations;
            let name = owner::CLASS_NAMES[class as usize];
            value.name[..name.len()].copy_from_slice(name);
        }
        out.write(value);
    }
    1
}
/// # Safety
/// Caller owns the header and advertised canonical stack-performance output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_stack(
    index: u32,
    out: *mut c::ProgramStackPerformanceInfo,
) -> i32 {
    unsafe {
        if (*out).version < 1
            || (*out).size < mem::size_of::<c::ProgramStackPerformanceInfo>() as u32
        {
            return -1;
        }
        if index as usize >= task_registry::ROLE_COUNT + stack::PROFILE_COUNT {
            return 0;
        }
        let mut value = c::ProgramStackPerformanceInfo::default();
        if (index as usize) < task_registry::ROLE_COUNT {
            let (role, name): (Role, &[u8]) = match index {
                0 => (Role::Input, b"input"),
                1 => (Role::ShortCompletion, b"short_completion"),
                2 => (Role::Interactive, b"interactive"),
                3 => (Role::Batch, b"batch"),
                _ => unreachable!(),
            };
            let s = task_resources::telemetry_for(role);
            value.flags = if mode::DETAILED { 3 } else { 0 };
            value.created = s.created;
            value.released = s.released;
            value.high_water_total_bytes = s.high_water_total_bytes;
            value.high_water_max_bytes = s.high_water_max_bytes;
            value.create_cycles_total = s.create_cycles_total;
            value.create_cycles_max = s.create_cycles_max;
            value.release_cycles_total = s.release_cycles_total;
            value.release_cycles_max = s.release_cycles_max;
            if s.created != 0 {
                value.reserve_max_bytes = task_resources::STACK_SIZE + task_resources::GUARD_SIZE;
                value.initial_commit_max_bytes = task_resources::STACK_SIZE
            }
            if s.released != 0 {
                value.committed_max_bytes = task_resources::STACK_SIZE
            }
            value.name[..name.len()].copy_from_slice(name);
        } else {
            let _ = stack::r4os_rust_program_stack_telemetry(
                index - task_registry::ROLE_COUNT as u32,
                &mut value,
            );
        }
        value.index = index;
        out.write(value);
    }
    1
}
/// # Safety
/// Exclusive resident canonical per-driver Work-performance output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_driver_work(
    selected: u32,
    out: *mut c::ProgramDriverWorkPerformanceInfo,
) -> i32 {
    if selected > work::OWNER_CAPACITY {
        unsafe {
            out.write(c::ProgramDriverWorkPerformanceInfo {
                selected_owner: selected,
                ..Default::default()
            })
        };
        return -1;
    }
    unsafe { work::r4os_rust_driver_work_performance(selected, out) };
    1
}
/// # Safety
/// The existing PCI owner validates the caller-owned canonical output header.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_pci_inventory(
    out: *mut c::ProgramPciInventoryPerformanceInfo,
) -> i32 {
    unsafe { pci_inventory::r4os_rust_pci_inventory_performance(out) }
}
/// # Safety
/// Exclusive resident canonical input-performance output to completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_input(
    out: *mut c::ProgramInputPerformanceInfo,
) -> i32 {
    unsafe {
        let controller = snapshot(i8042::r4os_rust_input_i8042_stats);
        let keys = snapshot(keyboard::r4os_rust_input_keyboard_stats);
        let mut value = c::ProgramInputPerformanceInfo {
            keyboard_queue_capacity: keys.queue_capacity,
            keyboard_queue_pending: keys.queue_pending,
            keyboard_queue_high_water: keys.queue_high_water,
            i8042_irq1_count: controller.irq1_count,
            i8042_irq12_count: controller.irq12_count,
            i8042_byte_count: controller.byte_count,
            i8042_keyboard_byte_count: controller.keyboard_byte_count,
            i8042_mouse_byte_count: controller.mouse_byte_count,
            i8042_keyboard_bytes_on_irq12: controller.keyboard_bytes_on_irq12,
            i8042_mouse_bytes_on_irq1: controller.mouse_bytes_on_irq1,
            i8042_drain_limit_hits: controller.drain_limit_hits,
            keyboard_push_attempts: keys.push_attempt_count,
            keyboard_accepted: keys.decoded_count,
            keyboard_dropped: keys.dropped_count,
            ..Default::default()
        };
        if providers::installed() {
            diagnostics::r4os_rust_program_diagnostic_input(&mut value)
        }
        out.write(value);
    }
    1
}
/// # Safety
/// Exclusive resident canonical IRQ timing output until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_irq_timing(
    irq: u32,
    out: *mut c::ProgramIrqTimingInfo,
) -> i32 {
    if irq as usize >= irq_router::MAX_IRQS {
        unsafe { out.write(Default::default()) };
        return 0;
    }
    unsafe {
        let mut legacy = c::IrqStats::default();
        let mut timing = core::mem::MaybeUninit::<irq_router::Timing>::uninit();
        let _ = irq_router::r4os_rust_irq_router_stats(
            irq as u8,
            false,
            (&mut legacy as *mut c::IrqStats).cast(),
        );
        let _ = irq_router::r4os_rust_irq_router_stats(irq as u8, true, timing.as_mut_ptr().cast());
        let timing = timing.assume_init();
        let clock = monotonic::snapshot();
        let mut coverage = c::performance_irq_coverage_dispatch
            | c::performance_irq_coverage_external_handler
            | c::performance_irq_coverage_delivery_unavailable;
        if clock.flags & c::monotonic_clock_flag_irq_independent != 0 {
            coverage |= c::performance_irq_coverage_irq_safe_clock
        }
        if timing.mixed_generation {
            coverage |= c::performance_irq_coverage_mixed_generation
        }
        if !mode::DETAILED {
            coverage &= !(c::performance_irq_coverage_dispatch
                | c::performance_irq_coverage_external_handler);
            coverage |= c::performance_irq_coverage_instrumentation_disabled
        }
        out.write(c::ProgramIrqTimingInfo {
            irq: irq.try_into().expect("original IRQ range"),
            registered: legacy.registered,
            shared: legacy.shared,
            masked: legacy.masked,
            coverage_flags: coverage,
            clock_flags: clock.flags,
            clock_source: clock.source,
            clock_generation: if timing.clock_generation != 0 {
                timing.clock_generation
            } else {
                clock.generation
            },
            unavailable_samples: timing.unavailable_samples,
            dispatch_samples: timing.dispatch_samples,
            handler_samples: timing.handler_samples,
            observer_reads: timing.observer_reads,
            delivery_samples: 0,
            dispatch_total_ns: timing.dispatch_total_ns,
            dispatch_max_ns: timing.dispatch_max_ns,
            dispatch_last_ns: timing.dispatch_last_ns,
            handler_total_ns: timing.handler_total_ns,
            handler_max_ns: timing.handler_max_ns,
            handler_last_ns: timing.handler_last_ns,
            ..Default::default()
        });
    }
    1
}
