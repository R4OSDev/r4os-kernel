//! Cached hints and mandatory exact CPU/OS/task/IF admission only. The SDK/
//! library owns every memory/hash algorithm and every SIMD instruction.
use super::{
    cpu_acceleration_window::{self as window, Hardware},
    cpu_capabilities::{self as catalog, LocalIrq, Snapshot},
    cpu_capabilities_features as features, cpu_topology, monotonic,
};
use crate::{
    arch::{cpu_lifecycle as life, percpu},
    contract as c,
    sched::task_context::Admission,
};
const FEATURES: [(u32, u64); 6] = [
    (c::cpu_feature_id_sha, c::cpu_accel_feature_sha as u64),
    (c::cpu_feature_id_erms, c::cpu_accel_feature_erms as u64),
    (c::cpu_feature_id_fsrm, c::cpu_accel_feature_fsrm as u64),
    (c::cpu_feature_id_bmi1, c::cpu_accel_feature_bmi1 as u64),
    (c::cpu_feature_id_bmi2, c::cpu_accel_feature_bmi2 as u64),
    (c::cpu_feature_id_adx, c::cpu_accel_feature_adx as u64),
];
fn masks(s: &Snapshot, index: u32) -> (u64, u64) {
    let (mut hardware, mut os) = (0, 0);
    for (id, mask) in FEATURES {
        if let Some(f) = features::by_id(s, index, id)
            && f.hardware_state == c::cpu_feature_hardware_present
        {
            hardware |= mask;
            if matches!(
                f.os_state,
                c::cpu_feature_os_enabled | c::cpu_feature_os_not_applicable
            ) {
                os |= mask;
            }
        }
    }
    (hardware, os)
}
fn baseline_ready(s: &Snapshot, index: u32) -> bool {
    // The online/schedulable owner already qualified the full common module
    // baseline. Recheck the SIMD state actually needed by this userland leaf.
    features::usable(s, index, c::cpu_feature_id_sse2)
        && features::usable(s, index, c::cpu_feature_id_avx2)
        && s.os.cr0 & ((1 << 2) | (1 << 3)) == 0
}
fn virtual_cpu(s: &Snapshot) -> bool {
    catalog::bit(&s.raw, 1, 0, 2, 31) == Some(true)
}
pub(crate) fn info() -> Result<c::CpuAccelerationInfo, i32> {
    let index = percpu::current_index();
    let epoch = cpu_topology::epoch(index);
    let s = catalog::at_epoch(index, epoch)?;
    let (hardware, os) = masks(&s, index);
    let configured = cpu_topology::count();
    let (mut common, mut known) = (63u64, 0u32);
    for cpu in 0..configured {
        if let Ok(peer) = catalog::snapshot(cpu) {
            common &= masks(&peer, cpu).0;
            known += 1;
        }
    }
    let mut flags = 0;
    if virtual_cpu(&s) {
        flags |= c::cpu_accel_info_flag_virtual;
    }
    if percpu::is_online(index) && percpu::is_schedulable(index) {
        flags |= c::cpu_accel_info_flag_online;
    }
    if known == configured && configured != 0 {
        flags |= c::cpu_accel_info_flag_common_complete;
    } else {
        common = 0;
    }
    if baseline_ready(&s, index) {
        flags |= c::cpu_accel_info_flag_baseline_ready;
    }
    Ok(c::CpuAccelerationInfo {
        cpu_index: index,
        flags,
        epoch: s.epoch,
        capture_sequence: s.sequence,
        hardware_mask: hardware,
        os_mask: os,
        common_hardware_mask: common,
        xcr0: s.os.xcr0,
        signature: s.raw.get(1, 0).map_or(0, |r| r.eax),
        configured_cpus: configured,
        common_known_count: known,
        feature_count: features::count(),
        ..Default::default()
    })
}
struct Native {
    index: u32,
    epoch: u64,
    clock_source: u8,
    clock_generation: u32,
    callback: c::CpuPureWorkFn,
    context: u64,
}
impl Hardware for Native {
    fn current(&self) -> bool {
        catalog::flags() & (1 << 9) == 0
            && percpu::current_index() == self.index
            && cpu_topology::epoch(self.index) == self.epoch
            && percpu::is_online(self.index)
            && percpu::is_schedulable(self.index)
            && !life::is_terminal()
    }
    fn now(&self) -> Option<u64> {
        if !self.current() {
            return None;
        }
        let stamp = monotonic::capture();
        if stamp.kind != self.clock_source || stamp.generation != self.clock_generation {
            return None;
        }
        monotonic::resolve(stamp)
    }
    fn work(&self, iterations: u32) -> i32 {
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        // Trusted pure resident kind-specific work, no API/owner/blocked page.
        // No Rust mutable reference to caller data or callback spans this call.
        let result = unsafe { (self.callback)(self.context, iterations) };
        core::sync::atomic::compiler_fence(core::sync::atomic::Ordering::SeqCst);
        result
    }
}
pub(crate) struct Work {
    pub value: c::CpuAccelerationWork,
    // Return physical membership before releasing its exact task/module.
    _probe: life::ProbeLease,
    _admission: Admission,
}
pub(crate) fn work(
    kind: u32,
    iterations: u32,
    callback: c::CpuPureWorkFn,
    context: u64,
) -> Result<Work, i32> {
    if kind > c::cpu_accel_work_kind_sha256
        || iterations == 0
        || iterations > c::cpu_accel_max_iterations
    {
        return Err(c::cpu_capabilities_error_invalid);
    }
    if catalog::flags() & (1 << 9) == 0 || !crate::memory::owner::fault_allowed() {
        return Err(c::cpu_capabilities_error_context);
    }
    let index = percpu::current_index();
    let epoch = cpu_topology::epoch(index);
    let s = catalog::at_epoch(index, epoch)?;
    if !baseline_ready(&s, index)
        || kind == c::cpu_accel_work_kind_sha256
            && !features::usable(&s, index, c::cpu_feature_id_sha)
    {
        return Err(c::cpu_capabilities_error_unsupported);
    }
    let admission = Admission::acquire(true).ok_or(c::cpu_capabilities_error_context)?;
    let probe = life::probe_lease().ok_or(c::cpu_capabilities_error_busy)?;
    crate::memory::owner::prepare_stack();
    let clock = monotonic::capture();
    if !matches!(clock.kind, 1 | 2) {
        return Err(c::cpu_capabilities_error_unsupported);
    }
    let original_if = catalog::flags() & (1 << 9);
    if original_if == 0 {
        return Err(c::cpu_capabilities_error_context);
    }
    let irq = LocalIrq::acquire();
    let native = Native {
        index,
        epoch,
        clock_source: clock.kind,
        clock_generation: clock.generation,
        callback,
        context,
    };
    if !native.current() {
        return Err(c::cpu_capabilities_error_context);
    }
    let value = c::CpuAccelerationWork {
        cpu_index: index,
        epoch,
        capture_sequence: s.sequence,
        work_kind: kind,
        iterations,
        flags: if virtual_cpu(&s) {
            c::cpu_accel_work_flag_virtual
        } else {
            0
        },
        granted_mask: if kind == c::cpu_accel_work_kind_sha256 {
            c::cpu_accel_feature_sha as u64
        } else {
            0
        },
        xcr0: s.os.xcr0,
        original_if,
        clock_source: u32::from(clock.kind),
        clock_generation: clock.generation,
        ..Default::default()
    };
    let mut value = window::run(value, &native);
    drop(irq);
    value.final_if = catalog::flags() & (1 << 9);
    if value.final_if == value.original_if {
        value.flags |= c::cpu_accel_work_flag_if_preserved;
    } else {
        value.flags &= !c::cpu_accel_work_flag_identity_valid;
        value.status = c::cpu_accel_work_status_identity_changed;
    }
    Ok(Work {
        value,
        _probe: probe,
        _admission: admission,
    })
}
const _: () = {
    assert!(core::mem::size_of::<c::CpuAccelerationInfo>() == 128);
    assert!(core::mem::size_of::<c::CpuAccelerationWork>() == 128);
};
