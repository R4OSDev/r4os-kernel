//! Original boot-phase/completion and immutable Limine inventory projections.
use super::fixed_z;
use crate::{
    boot::{info, performance},
    contract as c,
};
use core::mem;

fn phase_name(phase: u8) -> &'static [u8] {
    match phase {
        0 => b"unknown",
        1 => b"entry",
        2 => b"cpu",
        3 => b"timer",
        4 => b"driver",
        5 => b"input",
        6 => b"memory",
        7 => b"storage",
        8 => b"module",
        9 => b"platform",
        10 => b"loader",
        11 => b"irq",
        12 => b"service",
        13 => b"runtime",
        14 => b"audio",
        15 => b"network",
        16 => b"usb",
        17 => b"driver-policy",
        18 => b"shell",
        19 => b"task-runtime",
        _ => unreachable!(),
    }
}
/// # Safety
/// Exclusive resident canonical phase record until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_boot_phase(
    index: u32,
    out: *mut c::ProgramBootPhasePerformanceInfo,
) -> i32 {
    let Some(p) = performance::phase_at(index) else {
        unsafe { out.write(Default::default()) };
        return 0;
    };
    let mut value = c::ProgramBootPhasePerformanceInfo {
        index,
        phase: u32::from(p.phase),
        first_tick: p.first_tick,
        last_tick: p.last_tick,
        total_ticks: p.total_ticks,
        transitions: p.transitions,
        ..Default::default()
    };
    fixed_z(&mut value.name, phase_name(p.phase));
    unsafe { out.write(value) };
    1
}
/// # Safety
/// Exclusive resident canonical clock-phase record until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_boot_phase_clock(
    index: u32,
    out: *mut c::ProgramBootPhaseClockInfo,
) -> i32 {
    let Some(p) = performance::phase_at(index) else {
        unsafe { out.write(Default::default()) };
        return 0;
    };
    let boot = performance::snapshot();
    let mut value = c::ProgramBootPhaseClockInfo {
        index,
        phase: u32::from(p.phase),
        clock_flags: if p.timing_valid {
            boot.clock_flags
        } else {
            boot.clock_flags & !c::monotonic_clock_flag_valid
        },
        clock_source: boot.clock_source,
        clock_generation: boot.clock_generation,
        transitions: p.transitions,
        first_ns: p.first_ns,
        last_ns: p.last_ns,
        total_ns: p.total_ns,
        unavailable_spans: p.timing_unavailable_spans,
        ..Default::default()
    };
    fixed_z(&mut value.name, phase_name(p.phase));
    unsafe { out.write(value) };
    1
}
/// # Safety
/// Exclusive resident canonical boot-completion record until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_performance_boot_summary(
    out: *mut c::ProgramBootPerformanceInfo,
) -> i32 {
    let boot = performance::snapshot();
    let terminal = matches!(boot.state, 2..=4);
    let ready = matches!(boot.state, 2 | 3);
    let mut flags = 0;
    if boot.initialized {
        flags |= c::boot_performance_flag_initialized
    }
    if terminal {
        flags |= c::boot_performance_flag_completed | c::boot_performance_flag_frozen
    }
    if ready {
        flags |= c::boot_performance_flag_ready
    }
    if boot.state == 3 {
        flags |= c::boot_performance_flag_fallback
    }
    if boot.state == 4 {
        flags |= c::boot_performance_flag_failed
    }
    if boot.timing_valid {
        flags |= c::boot_performance_flag_timing_valid
    }
    unsafe {
        out.write(c::ProgramBootPerformanceInfo {
            version: c::boot_performance_version,
            size: c::boot_performance_size,
            state: boot.state,
            completion_reason: boot.completion_reason,
            flags,
            current_phase: u32::from(boot.current_phase),
            phase_count: boot.phase_count,
            timing_span_count: boot.timing_span_count,
            timing_unavailable_spans: boot.timing_unavailable_spans,
            timing_dropped_spans: boot.timing_dropped_spans,
            clock_flags: boot.clock_flags,
            clock_source: boot.clock_source,
            clock_generation: boot.clock_generation,
            configured_attempts: boot.configured_attempts,
            fallback_attempts: boot.fallback_attempts,
            launch_failures: boot.launch_failures,
            shell_instance_id: boot.shell_instance_id,
            boot_start_tick: boot.boot_start_tick,
            boot_end_tick: boot.now_tick,
            total_ticks: boot.total_ticks,
            boot_start_ns: if boot.timing_valid && boot.now_ns >= boot.total_ns {
                boot.now_ns - boot.total_ns
            } else {
                0
            },
            boot_end_ns: boot.now_ns,
            total_ns: boot.total_ns,
            clock_resolution_ns: boot.clock_resolution_ns,
            transition_count: boot.transition_count,
            ..Default::default()
        })
    };
    i32::from(boot.initialized)
}
/// # Safety
/// Exclusive resident canonical boot inventory output. Boot owner is immutable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_boot_info_summary(out: *mut c::BootInfoSummary) -> i32 {
    let source = unsafe { &*info::r4os_rust_boot_info() };
    // Native bit32 is software-graphics-once. Public bit32 instead means EDID.
    let mut value = c::BootInfoSummary {
        flags: source.flags & 31,
        memory_map_count: source.memory_count.min(u32::MAX as usize) as u32,
        max_memory_map_entries: 128,
        hhdm_offset: if source.flags & 4 != 0 {
            source.hhdm
        } else {
            0
        },
        rsdp_address: if source.flags & 16 != 0 {
            source.rsdp
        } else {
            0
        },
        ..Default::default()
    };
    fixed_z(&mut value.bootloader_name, b"Limine");
    if !source.framebuffer.is_null() {
        let f = unsafe { source.framebuffer.read() };
        value.framebuffer_address = f.address.addr() as u64;
        value.framebuffer_width = f.width;
        value.framebuffer_height = f.height;
        value.framebuffer_pitch = f.pitch;
        value.framebuffer_bpp = f.bpp;
        value.framebuffer_memory_model = f.memory_model;
        value.framebuffer_red_mask_size = f.red_mask_size;
        value.framebuffer_red_mask_shift = f.red_mask_shift;
        value.framebuffer_green_mask_size = f.green_mask_size;
        value.framebuffer_green_mask_shift = f.green_mask_shift;
        value.framebuffer_blue_mask_size = f.blue_mask_size;
        value.framebuffer_blue_mask_shift = f.blue_mask_shift;
        value.edid_size = f.edid_size;
        value.edid_address = f.edid.addr() as u64;
        if !f.edid.is_null() && f.edid_size != 0 {
            value.flags |= c::boot_info_flag_has_edid
        }
    }
    unsafe { out.write(value) };
    1
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_r4dev_boot_info_memory_count() -> u32 {
    unsafe {
        (*info::r4os_rust_boot_info())
            .memory_count
            .min(u32::MAX as usize) as u32
    }
}
/// # Safety
/// Exclusive resident canonical output; the immutable boot map stays resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_boot_info_memory_entry(
    index: u32,
    out: *mut c::BootInfoMemoryEntry,
) -> i32 {
    let source = unsafe { &*info::r4os_rust_boot_info() };
    if index as usize >= source.memory_count {
        unsafe { out.write(Default::default()) };
        return 0;
    }
    let entry = unsafe { source.memory.add(index as usize).read() };
    unsafe {
        out.write(c::BootInfoMemoryEntry {
            base: entry.base,
            length: entry.length,
            kind: entry.kind,
            ..Default::default()
        })
    };
    1
}
const _: () = {
    assert!(
        c::boot_performance_state_uninitialized == 0
            && c::boot_performance_state_running == 1
            && c::boot_performance_state_ready == 2
            && c::boot_performance_state_fallback_ready == 3
            && c::boot_performance_state_failed == 4
    );
    assert!(
        c::boot_completion_reason_none == 0
            && c::boot_completion_reason_configured_shell_ready == 1
            && c::boot_completion_reason_terminal_fallback_ready == 2
            && c::boot_completion_reason_recovery_fallback_ready == 3
            && c::boot_completion_reason_no_shell == 4
            && c::boot_completion_reason_fatal_error == 5
            && c::boot_completion_reason_shell_exited_before_ready == 6
    );
    assert!(mem::size_of::<c::ProgramBootPerformanceInfo>() == c::boot_performance_size as usize);
};
