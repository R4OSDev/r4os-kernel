//! External CPU observation routing and exact callback lifetime only. Model
//! policy, samples and unit decoding belong to CPUPOWER.R4D.
use crate::{
    arch::runtime::RuntimeGuard,
    contract as c,
    driver::{api_admission as admission, heap},
    platform::{cpu_capabilities as catalog, cpu_registers::Session, monotonic},
    sched::task_context::Admission,
};
use core::{cell::UnsafeCell, mem, ptr};

#[derive(Clone, Copy)]
struct Binding {
    owner: u32,
    epoch: u64,
    cookie: u64,
    backend: c::CpuPowerBackend,
}
struct Metadata {
    binding: Option<Binding>,
    next_cookie: u64,
    active: u32,
    closing: bool,
}
struct Storage(UnsafeCell<Metadata>);
// All metadata transitions/copies use RuntimeGuard. No borrowed metadata or
// runtime owner survives a module callback, clock access or caller wait.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(Metadata {
    binding: None,
    next_cookie: 1,
    active: 0,
    closing: false,
}));

pub(crate) fn owner_identity(allow_closing: bool) -> Option<(u32, u64)> {
    let owner = admission::r4os_rust_driver_api_runtime_owner(false);
    if owner == 0 {
        return None;
    }
    let mut stats = c::DriverHeapStats::default();
    if unsafe { heap::r4os_rust_driver_heap_stats(owner, &mut stats) } != 0
        || stats.owner_epoch == 0
        || (!allow_closing && stats.closing != 0)
    {
        return None;
    }
    Some((owner, stats.owner_epoch))
}
fn lifecycle_identity() -> Option<(u32, u64)> {
    if admission::r4os_rust_driver_api_owner_scalar(1) == 0 {
        return None;
    }
    let identity = owner_identity(true)?;
    (admission::r4os_rust_driver_api_owner_scalar(0) == identity.0).then_some(identity)
}
pub(crate) fn register(backend: c::CpuPowerBackend) -> Result<u64, i32> {
    if backend.version != c::cpu_power_version
        || backend.size < mem::size_of::<c::CpuPowerBackend>() as u32
        || backend.flags != c::cpu_power_backend_flag_parallel
        || backend.reserved0 != 0
        || backend.snapshot == 0
        || backend.capture == 0
    {
        return Err(c::cpu_capabilities_error_invalid);
    }
    let (owner, epoch) = lifecycle_identity().ok_or(c::cpu_capabilities_error_context)?;
    if owner_identity(false) != Some((owner, epoch)) {
        return Err(c::cpu_capabilities_error_context);
    }
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    if state.binding.is_some() || state.active != 0 {
        return Err(c::cpu_capabilities_error_busy);
    }
    let cookie = state.next_cookie;
    state.next_cookie = cookie
        .checked_add(1)
        .ok_or(c::cpu_capabilities_error_busy)?;
    state.binding = Some(Binding {
        owner,
        epoch,
        cookie,
        backend,
    });
    state.closing = false;
    Ok(cookie)
}
pub(crate) fn unregister(cookie: u64) -> i32 {
    let Some((owner, epoch)) = lifecycle_identity() else {
        return c::cpu_capabilities_error_context;
    };
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    let Some(binding) = state.binding else {
        return c::cpu_capabilities_error_epoch;
    };
    if cookie == 0 || (binding.owner, binding.epoch, binding.cookie) != (owner, epoch, cookie) {
        return c::cpu_capabilities_error_epoch;
    }
    if state.active != 0 {
        return c::cpu_capabilities_error_busy;
    }
    state.closing = true;
    state.binding = None;
    c::cpu_capabilities_ok
}
/// Rejected preparation does not leave a live provider silently closed.
pub(crate) fn prepare_close(owner: u32) -> bool {
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    if !state.binding.is_some_and(|b| b.owner == owner) {
        return true;
    }
    if state.active != 0 {
        return false;
    }
    state.closing = true;
    true
}
pub(crate) fn cancel_close(owner: u32) {
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    if state.binding.is_some_and(|b| b.owner == owner) {
        state.closing = false;
    }
}
pub(crate) fn finish_close(owner: u32) -> bool {
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    if !state.binding.is_some_and(|b| b.owner == owner) {
        return true;
    }
    if !state.closing || state.active != 0 {
        return false;
    }
    state.binding = None;
    true
}
struct Lease(Binding);
impl Lease {
    fn acquire() -> Result<Option<Self>, i32> {
        let binding = {
            let _runtime = RuntimeGuard::acquire();
            let state = unsafe { &mut *STATE.0.get() };
            let Some(binding) = state.binding else {
                return Ok(None);
            };
            if state.closing {
                return Err(c::cpu_capabilities_error_busy);
            }
            state.active = state
                .active
                .checked_add(1)
                .ok_or(c::cpu_capabilities_error_busy)?;
            binding
        };
        let lease = Self(binding);
        let mut stats = c::DriverHeapStats::default();
        if unsafe { heap::r4os_rust_driver_heap_stats(binding.owner, &mut stats) } != 0
            || stats.owner_epoch != binding.epoch
            || stats.closing != 0
        {
            return Err(c::cpu_capabilities_error_busy);
        }
        Ok(Some(lease))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _runtime = RuntimeGuard::acquire();
        let state = unsafe { &mut *STATE.0.get() };
        assert!(state.binding.is_some_and(|b| b.cookie == self.0.cookie));
        state.active = state
            .active
            .checked_sub(1)
            .expect("CPU provider call lease");
    }
}
struct Callback(admission::CallbackToken);
impl Callback {
    fn acquire(owner: u32) -> Option<Self> {
        let mut token = mem::MaybeUninit::uninit();
        if unsafe { admission::r4os_rust_driver_api_storage_enter(owner, token.as_mut_ptr()) } {
            Some(Self(unsafe { token.assume_init() }))
        } else {
            None
        }
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        unsafe { admission::r4os_rust_driver_api_storage_leave(&self.0) };
    }
}
/// # Safety
/// Exclusive initialized resident output/tail for the synchronous call.
unsafe fn valid(out: *const c::CpuPowerInfo) -> bool {
    !out.is_null()
        && out.is_aligned()
        && unsafe {
            (*out).version == c::cpu_power_version
                && (*out).size >= mem::size_of::<c::CpuPowerInfo>() as u32
        }
}
type Observe = unsafe extern "C" fn(usize, u32, u64, *mut c::CpuPowerInfo) -> i32;
unsafe fn observe(cpu: u32, epoch: u64, out: *mut c::CpuPowerInfo, capture: bool) -> i32 {
    if !unsafe { valid(out) } {
        return c::cpu_capabilities_error_invalid;
    }
    let Some(_admission) = Admission::acquire(true) else {
        return c::cpu_capabilities_error_context;
    };
    let cap = match catalog::info(cpu) {
        Ok(cap) => cap,
        Err(reason) => return reason,
    };
    if epoch == 0 || cap.epoch != epoch {
        return c::cpu_capabilities_error_epoch;
    }
    let _session = if capture {
        match Session::acquire(cpu, epoch) {
            Ok(session) => Some(session),
            Err(reason) => return reason,
        }
    } else {
        None
    };
    let clock = monotonic::snapshot();
    if clock.flags & c::monotonic_clock_flag_valid == 0
        || clock.generation == 0
        || clock.generation == u32::MAX
    {
        return c::cpu_capabilities_error_busy;
    }
    let lease = match Lease::acquire() {
        Ok(Some(lease)) => lease,
        Ok(None) if !capture => {
            let value = c::CpuPowerInfo {
                cpu_index: cpu,
                epoch,
                family: cap.family,
                model: cap.model,
                stepping: cap.stepping,
                vendor_name: cap.vendor_name,
                ..Default::default()
            };
            unsafe { ptr::write(out, value) };
            return c::cpu_capabilities_ok;
        }
        Ok(None) => return c::cpu_capabilities_error_unsupported,
        Err(reason) => return reason,
    };
    let Some(_callback) = Callback::acquire(lease.0.owner) else {
        return c::cpu_capabilities_error_busy;
    };
    let address = if capture {
        lease.0.backend.capture
    } else {
        lease.0.backend.snapshot
    };
    let function: Observe = unsafe { mem::transmute(address) };
    let mut value = c::CpuPowerInfo::default();
    let result = unsafe { function(lease.0.backend.context as usize, cpu, epoch, &mut value) };
    if result != c::cpu_capabilities_ok {
        return result;
    }
    if value.version != c::cpu_power_version
        || value.size != mem::size_of::<c::CpuPowerInfo>() as u32
        || value.cpu_index != cpu
        || value.epoch != epoch
    {
        return c::cpu_capabilities_error_invalid;
    }
    let latest = match catalog::info(cpu) {
        Ok(latest) if latest.epoch == epoch => latest,
        _ => return c::cpu_capabilities_error_epoch,
    };
    if capture && crate::arch::percpu::current_index() != cpu {
        return c::cpu_capabilities_error_context;
    }
    let after = monotonic::snapshot();
    if capture && (after.generation != clock.generation || after.source != clock.source) {
        return c::cpu_capabilities_error_busy;
    }
    if value.valid_mask != 0
        && (latest.flags & c::cpu_capabilities_flag_schedulable == 0
            || value.clock_generation != after.generation
            || value.clock_source != after.source
            || value.captured_ns == 0
            || value.captured_ns > after.instant_ns
            || value.sequence == 0)
    {
        value.valid_mask = 0;
        value.state = c::cpu_power_state_stale;
        value.flags |= c::cpu_power_flag_stale;
    }
    value.provider_generation = lease.0.cookie;
    value.flags |= c::cpu_power_flag_installed;
    unsafe { ptr::write(out, value) };
    c::cpu_capabilities_ok
}
/// # Safety
/// Same initialized exclusive output contract as the canonical R4DEV slot.
pub(crate) unsafe extern "C" fn info(cpu: u32, epoch: u64, out: *mut c::CpuPowerInfo) -> i32 {
    unsafe { observe(cpu, epoch, out, false) }
}
/// # Safety
/// Same output contract; explicit admitted local task/CPU epoch is required.
pub(crate) unsafe extern "C" fn capture(cpu: u32, epoch: u64, out: *mut c::CpuPowerInfo) -> i32 {
    unsafe { observe(cpu, epoch, out, true) }
}
