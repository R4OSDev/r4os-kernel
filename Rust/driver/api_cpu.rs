//! Optional R4D CPU observation mechanism. Interpretation stays external;
//! immutable metadata shares the existing canonical projections.
use crate::{
    arch::percpu,
    contract as c,
    driver::{api_admission as admission, irq_router},
    platform::{cpu_capabilities::LocalIrq, cpu_power, cpu_registers::Session, monotonic},
    program::r4dev,
};
use core::{mem, ptr};
unsafe fn valid<T>(out: *const T) -> bool {
    !out.is_null()
        && out.is_aligned()
        && unsafe {
            let header = out.cast::<u32>();
            header.read() == 1 && header.add(1).read() >= mem::size_of::<T>() as u32
        }
}
fn admitted() -> bool {
    irq_router::r4os_rust_irq_router_local(false) == 0 && cpu_power::owner_identity(false).is_some()
}
/// # Safety
/// Exclusive initialized resident table; only an owned R4D init scope queries it.
pub(crate) unsafe extern "C" fn query(out: *mut c::DriverCpuPowerApi) -> i32 {
    if out.is_null() || !out.is_aligned() {
        return c::cpu_capabilities_error_invalid;
    }
    let header = out.cast::<u32>();
    let capacity = unsafe { header.add(1).read() };
    if unsafe { header.read() } != 1 || capacity < 64 {
        return c::cpu_capabilities_error_invalid;
    }
    if !admitted() || admission::r4os_rust_driver_api_owner_scalar(1) == 0 {
        return c::cpu_capabilities_error_context;
    }
    let bytes = (capacity as usize).min(mem::size_of::<c::DriverCpuPowerApi>()) & !7;
    let value = c::DriverCpuPowerApi {
        size: bytes as u32,
        register_backend: register as *const () as usize as u64,
        unregister_backend: unregister as *const () as usize as u64,
        current_cpu: current as *const () as usize as u64,
        capabilities: capabilities as *const () as usize as u64,
        topology: topology as *const () as usize as u64,
        register_read: read as *const () as usize as u64,
        clock: clock as *const () as usize as u64,
        counters_read: counters as *const () as usize as u64,
        feature: feature as *const () as usize as u64,
        register_performance: register_performance as *const () as usize as u64,
        control_acquire: control_acquire as *const () as usize as u64,
        control_release: control_release as *const () as usize as u64,
        register_compare_write: compare_write as *const () as usize as u64,
        register_cppc: register_cppc as *const () as usize as u64,
        rank_publish: rank_publish as *const () as usize as u64,
        ..Default::default()
    };
    unsafe {
        ptr::copy_nonoverlapping(ptr::from_ref(&value).cast::<u8>(), out.cast::<u8>(), bytes)
    };
    c::cpu_capabilities_ok
}
unsafe extern "C" fn register(backend: *const c::CpuPowerBackend, cookie: *mut u64) -> i32 {
    if !unsafe { valid(backend) } || cookie.is_null() || !cookie.is_aligned() {
        return c::cpu_capabilities_error_invalid;
    }
    if !admission::callbacks_parallel() {
        return c::cpu_capabilities_error_context;
    }
    match cpu_power::register(unsafe { backend.read() }) {
        Ok(value) => {
            unsafe { cookie.write(value) };
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
extern "C" fn unregister(cookie: u64) -> i32 {
    cpu_power::unregister(cookie)
}
unsafe extern "C" fn register_performance(
    cookie: u64,
    backend: *const c::CpuPerformanceBackend,
) -> i32 {
    if !unsafe { valid(backend) } {
        return c::cpu_capabilities_error_invalid;
    }
    if !admission::callbacks_parallel() {
        return c::cpu_capabilities_error_context;
    }
    cpu_power::register_performance(cookie, unsafe { backend.read() })
}
unsafe extern "C" fn register_cppc(cookie: u64, backend: *const c::CpuCppcBackend) -> i32 {
    if !unsafe { valid(backend) } {
        return c::cpu_capabilities_error_invalid;
    }
    if !admission::callbacks_parallel() {
        return c::cpu_capabilities_error_context;
    }
    cpu_power::register_cppc(cookie, unsafe { backend.read() })
}
unsafe extern "C" fn rank_publish(input: *const c::CpuPerformanceHint, sequence: *mut u64) -> i32 {
    if !unsafe { valid(input) } || sequence.is_null() || !sequence.is_aligned() {
        return c::cpu_capabilities_error_invalid;
    }
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    let input = unsafe { input.read() };
    let _session = match Session::acquire(input.cpu_index, input.epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    match cpu_power::hints::publish(input) {
        Ok(value) => {
            unsafe { sequence.write(value) };
            0
        }
        Err(reason) => reason,
    }
}
unsafe extern "C" fn feature(
    cpu: u32,
    epoch: u64,
    ordinal: u32,
    out: *mut c::CpuFeatureInfo,
) -> i32 {
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    unsafe { r4dev::cpu_capabilities::feature(cpu, epoch, ordinal, out) }
}
unsafe extern "C" fn control_acquire(cpu: u32, epoch: u64, token: *mut u64) -> i32 {
    if token.is_null() || !token.is_aligned() {
        return c::cpu_capabilities_error_invalid;
    }
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    let _session = match Session::acquire(cpu, epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    match cpu_power::control_acquire(cpu, epoch) {
        Ok(value) => {
            unsafe { token.write(value) };
            0
        }
        Err(reason) => reason,
    }
}
extern "C" fn control_release(cpu: u32, epoch: u64, token: u64) -> i32 {
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    let _session = match Session::acquire(cpu, epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    cpu_power::control_release(cpu, epoch, token)
}
unsafe extern "C" fn compare_write(
    cpu: u32,
    epoch: u64,
    token: u64,
    id: u32,
    expected: u64,
    requested: u64,
    out: *mut c::CpuRegisterWriteInfo,
) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    let _control = match cpu_power::ControlCall::acquire(cpu, epoch, token) {
        Ok(control) => control,
        Err(reason) => return reason,
    };
    let session = match Session::acquire(cpu, epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    match session.compare_write(token, id, expected, requested) {
        Ok(value) => {
            unsafe { out.write(value) };
            0
        }
        Err(reason) => reason,
    }
}
extern "C" fn current() -> u32 {
    if admitted() {
        percpu::current_index()
    } else {
        c::cpu_power_cpu_none
    }
}
unsafe extern "C" fn capabilities(cpu: u32, out: *mut c::CpuCapabilitiesInfo) -> i32 {
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    unsafe { r4dev::cpu_capabilities::info(cpu, out) }
}
unsafe extern "C" fn topology(cpu: u32, out: *mut c::CpuTopologyInfo) -> i32 {
    if !admitted() {
        return c::cpu_topology_result_busy;
    }
    unsafe { r4dev::cpu_topology::info(cpu, out) }
}
unsafe extern "C" fn read(cpu: u32, epoch: u64, id: u32, out: *mut c::CpuRegisterInfo) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    let session = match Session::acquire(cpu, epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    match session.read(id) {
        Ok(value) => {
            unsafe { ptr::write(out, value) };
            c::cpu_capabilities_ok
        }
        Err(reason) => reason,
    }
}
unsafe extern "C" fn clock(out: *mut c::MonotonicClockInfo) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    // Preserve caller output on unavailable clock as on every refusal.
    let mut value = c::MonotonicClockInfo::default();
    let result = unsafe { monotonic::r4os_rust_monotonic_clock_api(&mut value) };
    if result == 1 {
        unsafe { ptr::write(out, value) };
    }
    result
}
unsafe extern "C" fn counters(cpu: u32, epoch: u64, out: *mut c::CpuPowerCounters) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    if !admitted() {
        return c::cpu_capabilities_error_context;
    }
    // Admission/membership is established with IF on. Only the two bounded
    // feature-gated recoverable reads and clock snapshots run with IF off;
    // no external module callback or caller output is touched in this scope.
    let session = match Session::acquire(cpu, epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    let value = {
        let _irq = LocalIrq::acquire();
        let before = monotonic::snapshot();
        if before.flags & c::monotonic_clock_flag_valid == 0
            || before.generation == 0
            || before.generation == u32::MAX
        {
            return c::cpu_capabilities_error_busy;
        }
        let aperf = match session.read(c::cpu_register_id_aperf) {
            Ok(value) => value,
            Err(reason) => return reason,
        };
        let mperf = match session.read(c::cpu_register_id_mperf) {
            Ok(value) => value,
            Err(reason) => return reason,
        };
        let after = monotonic::snapshot();
        if after.generation != before.generation
            || after.source != before.source
            || after.instant_ns < before.instant_ns
            || aperf.capture_sequence != mperf.capture_sequence
        {
            return c::cpu_capabilities_error_busy;
        }
        c::CpuPowerCounters {
            cpu_index: cpu,
            flags: c::cpu_register_flag_raw,
            epoch,
            aperf: aperf.value,
            mperf: mperf.value,
            captured_ns: after.instant_ns,
            clock_generation: after.generation,
            clock_source: after.source,
            capture_sequence: aperf.capture_sequence,
            ..Default::default()
        }
    };
    unsafe { ptr::write(out, value) };
    c::cpu_capabilities_ok
}
