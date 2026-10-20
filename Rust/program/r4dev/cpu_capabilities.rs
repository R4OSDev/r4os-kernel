//! Canonical optional CPU observations and explicitly requested local work.
//! Providers copy fixed values only after all refusal paths have completed.
use crate::{
    contract as c,
    platform::{
        cpu_capabilities as catalog, cpu_capabilities_features as features, cpu_registers::Session,
    },
};
use core::{mem, ptr};
unsafe fn valid<T>(out: *mut T) -> bool {
    // Every new fixed record deliberately starts with the same two u32s.
    if out.is_null() || !out.is_aligned() {
        return false;
    }
    let header = out.cast::<u32>();
    unsafe {
        header.read() == c::cpu_capabilities_version
            && header.add(1).read() >= mem::size_of::<T>() as u32
    }
}
/// # Safety
/// Exclusive initialized fixed output/capacity; cached copies only.
pub(crate) unsafe extern "C" fn pmu_info(index: u32, out: *mut c::CpuPmuInfo) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    match crate::platform::cpu_pmu::info(index) {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// The caller retains a live aligned typed callback slot, resident pure work
/// and context, and exclusive output through this synchronous call. No Rust
/// reference to the slot/output survives the module callback.
pub(crate) unsafe extern "C" fn pmu_measure(
    index: u32,
    epoch: u64,
    event: u32,
    iterations: u32,
    work: *const c::PmuWorkFn,
    context: u64,
    out: *mut c::CpuPmuResult,
) -> i32 {
    if !unsafe { valid(out) } || work.is_null() || !work.is_aligned() {
        return c::cpu_capabilities_error_invalid;
    }
    // Read raw bits before creating a nonnull Rust function-pointer value.
    if unsafe { work.cast::<u64>().read() } == 0 {
        return c::cpu_capabilities_error_invalid;
    }
    let callback = unsafe { work.read() };
    match crate::platform::cpu_pmu::measure(index, epoch, event, iterations, callback, context) {
        Ok(measured) => {
            unsafe { out.write(measured.value) };
            0
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Exclusive fixed output; exact original identity, no hardware operation.
pub(crate) unsafe extern "C" fn pmu_result(
    index: u32,
    epoch: u64,
    generation: u64,
    out: *mut c::CpuPmuResult,
) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    match crate::platform::cpu_pmu::result(index, epoch, generation) {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Exclusive initialized fixed output/capacity. No CPU operation or callback.
pub(crate) unsafe extern "C" fn microcode(index: u32, out: *mut c::CpuMicrocodeInfo) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    match crate::arch::microcode::snapshot(index) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Exclusive initialized fixed record; resident metadata only, no CPU work.
pub(crate) unsafe extern "C" fn machine_info(index: u32, out: *mut c::CpuMachineInfo) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    match crate::arch::machine_check::info(index) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Same fixed record/tail contract; exact original capture identity required.
pub(crate) unsafe extern "C" fn machine_bank(
    index: u32,
    epoch: u64,
    sequence: u64,
    ordinal: u32,
    out: *mut c::CpuMachineBank,
) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    match crate::arch::machine_check::bank(index, epoch, sequence, ordinal) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Exclusive initialized resident fixed record; no hardware instruction/query
/// occurs beyond a copy of canonical metadata and idle software counters.
pub(crate) unsafe extern "C" fn idle(index: u32, out: *mut c::CpuIdleInfo) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    match crate::arch::cpu_idle::info(index) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            0
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Exclusive resident initialized header/capacity; output/tail unchanged on error.
pub(crate) unsafe extern "C" fn info(index: u32, out: *mut c::CpuCapabilitiesInfo) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    match catalog::info(index) {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Same exclusive fixed-header contract as info. No caller borrow is retained.
pub(crate) unsafe extern "C" fn feature(
    index: u32,
    epoch: u64,
    ordinal: u32,
    out: *mut c::CpuFeatureInfo,
) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    let value = catalog::at_epoch(index, epoch).and_then(|s| features::feature(&s, index, ordinal));
    match value {
        Ok(value) => {
            unsafe {
                out.write(value);
            }
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Same fixed-header contract. Task admission pins output through publication;
/// no arbitrary register ID, kernel pointer or request survives this call.
pub(crate) unsafe extern "C" fn read(
    index: u32,
    epoch: u64,
    id: u32,
    out: *mut c::CpuRegisterInfo,
) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    let session = match Session::acquire(index, epoch) {
        Ok(s) => s,
        Err(e) => return e,
    };
    match session.read(id) {
        Ok(value) => {
            unsafe {
                ptr::write(out, value);
            }
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
/// # Safety
/// Same fixed-header/task contract. Only explicit callers trigger four bounded
/// reserved-ID probes; real failures are reported rather than swallowed.
pub(crate) unsafe extern "C" fn check(
    index: u32,
    epoch: u64,
    out: *mut c::ProgramKernelCpuRegisterCheck,
) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    let session = match Session::acquire(index, epoch) {
        Ok(s) => s,
        Err(e) => return e,
    };
    match session.check() {
        Ok(value) => {
            unsafe {
                ptr::write(out, value);
            }
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
