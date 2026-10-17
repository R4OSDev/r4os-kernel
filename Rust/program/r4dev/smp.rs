//! Passive remote-TLB/clock facts and explicit owned functional SMP work.
use crate::{
    arch::{percpu, smp_probe, tlb},
    contract as c,
    platform::clocksource,
    sched::task_context::Admission,
};
use core::mem;
/// # Safety
/// Caller retains exclusive resident v1 header/capacity through return. No
/// worker receives this output pointer or any caller-owned borrowed storage.
pub unsafe extern "C" fn check(out: *mut c::ProgramKernelSmpCheck) -> i32 {
    if out.is_null()
        || unsafe { (*out).version } != c::kernel_smp_check_version
        || unsafe { (*out).size } < mem::size_of::<c::ProgramKernelSmpCheck>() as u32
    {
        return -1;
    }
    let Some(admission) = Admission::acquire(true) else {
        unsafe {
            out.write(c::ProgramKernelSmpCheck::default());
        }
        return 0;
    };
    let report = smp_probe::check(&admission);
    unsafe {
        out.write(report);
    }
    1
}
/// # Safety
/// Caller supplies an exclusive resident v1 snapshot header and its capacity.
/// Every owner load is passive; this is not an atomic all-owner transaction.
pub unsafe extern "C" fn snapshot(out: *mut c::ProgramSmpTlbSnapshot) -> i32 {
    if out.is_null()
        || unsafe { (*out).version } != c::smp_tlb_snapshot_version
        || unsafe { (*out).size } < mem::size_of::<c::ProgramSmpTlbSnapshot>() as u32
    {
        return -1;
    }
    let online = percpu::r4os_rust_percpu_control(16, 0, 0, 0);
    let scheduled = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
    let clock = clocksource::snapshot();
    let mut raw = mem::MaybeUninit::uninit();
    unsafe {
        tlb::r4os_rust_tlb_stats(raw.as_mut_ptr());
    }
    let stats = unsafe { raw.assume_init() };
    let mut flags = 0;
    if clock.source != 0 {
        flags |= c::smp_tlb_snapshot_flag_clock_available;
    }
    if tlb::registered() {
        flags |= c::smp_tlb_snapshot_flag_tlb_registered;
    }
    if tlb::isolated_injection_active() {
        flags |= c::smp_tlb_snapshot_flag_isolated_injection;
    }
    let report = c::ProgramSmpTlbSnapshot {
        flags,
        online_count: online.count_ones(),
        schedulable_count: scheduled.count_ones(),
        clock_source: clock.source,
        clock_generation: clock.generation,
        clock_flags: clock.flags,
        clock_fallback_reason: clock.fallback_reason,
        online_mask: online,
        schedulable_mask: scheduled,
        clock_registered_mask: clock.registered_cpu_mask,
        clock_frequency_hz: clock.frequency_hz,
        clock_resolution_ns: clock.resolution_ns,
        max_cpu_skew_ns: clock.max_cpu_skew_ns,
        calibration_error_ppm: clock.calibration_error_ppm,
        tlb_requests: stats.requests,
        tlb_successes: stats.successes,
        tlb_timeouts: stats.timeouts,
        tlb_expected_timeouts: stats.expected_timeouts,
        tlb_ipis_sent: stats.ipis_sent,
        tlb_acknowledgements: stats.acknowledgements,
        tlb_offline_exclusions: stats.offline_exclusions,
        tlb_local_page_flushes: stats.local_page_flushes,
        tlb_local_full_reloads: stats.local_full_reloads,
        tlb_max_wait_spins: stats.max_wait_spins,
        tlb_generation: stats.generation,
        ..Default::default()
    };
    unsafe {
        out.write(report);
    }
    1
}
