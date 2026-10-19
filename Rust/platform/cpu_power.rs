//! External CPU observation/control routing and exact callback/hardware lifetime.
//! Model policy, parameters, snapshots and actual restoration belong to CPUPOWER.
use crate::{
    arch::{cpu_lifecycle as life, runtime::RuntimeGuard},
    contract as c,
    driver::{api_admission as admission, heap},
    platform::{cpu_capabilities as catalog, cpu_registers::Session, monotonic},
    sched::task_context::Admission,
};
use core::{
    cell::UnsafeCell,
    mem, ptr,
    sync::atomic::{AtomicBool, Ordering},
};
#[path = "cpu_power/firmware.rs"]
pub(crate) mod firmware;
#[path = "cpu_power/hints.rs"]
pub(crate) mod hints;

#[derive(Clone, Copy)]
struct Binding {
    owner: u32,
    epoch: u64,
    cookie: u64,
    backend: c::CpuPowerBackend,
    performance: Option<c::CpuPerformanceBackend>,
    cppc: Option<c::CpuCppcBackend>,
}
struct Control {
    owner: u32,
    owner_epoch: u64,
    cpu_epoch: u64,
    token: u64,
    active: bool,
    _membership: life::ProbeLease,
}
struct Metadata {
    binding: Option<Binding>,
    next_cookie: u64,
    active: u32,
    closing: bool,
    controls: [Option<Control>; 64],
    next_control: u64,
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
    controls: [const { None }; 64],
    next_control: 1,
}));

// A terminal transition closes new hardware ownership before asking the
// external policy owner to return existing requests. No governor lives here.
static SYSTEM_TRANSITION: AtomicBool = AtomicBool::new(false);
pub(crate) struct SystemTransition {
    _membership: life::ProbeLease,
}
impl SystemTransition {
    pub(crate) fn acquire() -> Option<Self> {
        SYSTEM_TRANSITION
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        let Some(membership) = life::probe_lease() else {
            SYSTEM_TRANSITION.store(false, Ordering::Release);
            return None;
        };
        Some(Self {
            _membership: membership,
        })
    }
    pub(crate) fn returned(&self) -> bool {
        let _runtime = RuntimeGuard::acquire();
        let state = unsafe { &*STATE.0.get() };
        // New callbacks are closed except exact existing returns. Both their
        // resident controls and callback frames must have really completed.
        state.active == 0
            && state.controls.iter().all(Option::is_none)
            && firmware::returned_locked(&_runtime)
    }
    pub(crate) fn commit(self) {
        // Keep the admitted cohort and closed ownership through the terminal
        // call itself; releasing here would reopen a logical offline race.
        // Terminal stop does not wait for ProbeLease. Reset starts fresh state.
        mem::forget(self);
    }
}
impl Drop for SystemTransition {
    fn drop(&mut self) {
        SYSTEM_TRANSITION.store(false, Ordering::Release);
    }
}
pub(crate) fn transitioning() -> bool {
    SYSTEM_TRANSITION.load(Ordering::Acquire)
}
pub(crate) fn provider_matches_locked(_runtime: &RuntimeGuard, id: (u32, u64)) -> bool {
    unsafe { &*STATE.0.get() }
        .binding
        .is_some_and(|b| (b.owner, b.epoch) == id)
}
pub(crate) fn provider_cookie_locked(
    _runtime: &RuntimeGuard,
    owner: u32,
    epoch: u64,
) -> Option<u64> {
    unsafe { &*STATE.0.get() }
        .binding
        .filter(|b| (b.owner, b.epoch) == (owner, epoch))
        .map(|b| b.cookie)
}
pub(crate) fn direct_busy_locked(_runtime: &RuntimeGuard, mask: u64) -> bool {
    unsafe { &*STATE.0.get() }
        .controls
        .iter()
        .enumerate()
        .any(|(cpu, c)| c.is_some() && mask & (1u64 << cpu) != 0)
}

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
    if state.binding.is_some() || state.active != 0 || SYSTEM_TRANSITION.load(Ordering::Acquire) {
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
        performance: None,
        cppc: None,
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
    if state.active != 0
        || state.controls.iter().any(Option::is_some)
        || firmware::facade_live_locked(&_runtime, owner, epoch)
    {
        return c::cpu_capabilities_error_busy;
    }
    state.closing = true;
    state.binding = None;
    hints::clear(&_runtime);
    c::cpu_capabilities_ok
}
/// Rejected preparation does not leave a live provider silently closed.
pub(crate) fn prepare_close(owner: u32) -> bool {
    let _runtime = RuntimeGuard::acquire();
    if !firmware::prepare_close_locked(&_runtime, owner) {
        return false;
    }
    let state = unsafe { &mut *STATE.0.get() };
    if !state.binding.is_some_and(|b| b.owner == owner) {
        return true;
    }
    if state.active != 0 || state.controls.iter().any(Option::is_some) {
        firmware::cancel_close_locked(&_runtime, owner);
        return false;
    }
    state.closing = true;
    true
}
pub(crate) fn cancel_close(owner: u32) {
    let _runtime = RuntimeGuard::acquire();
    firmware::cancel_close_locked(&_runtime, owner);
    let state = unsafe { &mut *STATE.0.get() };
    if state.binding.is_some_and(|b| b.owner == owner) {
        state.closing = false;
    }
}
pub(crate) fn finish_close(owner: u32) -> bool {
    let _runtime = RuntimeGuard::acquire();
    if !firmware::finish_close_locked(&_runtime, owner) {
        return false;
    }
    let state = unsafe { &mut *STATE.0.get() };
    if !state.binding.is_some_and(|b| b.owner == owner) {
        return true;
    }
    if !state.closing || state.active != 0 || state.controls.iter().any(Option::is_some) {
        return false;
    }
    state.binding = None;
    hints::clear(&_runtime);
    true
}
pub(crate) fn register_performance(cookie: u64, backend: c::CpuPerformanceBackend) -> i32 {
    if backend.version != 1
        || backend.size < mem::size_of::<c::CpuPerformanceBackend>() as u32
        || backend.execute == 0
        || backend.flags != c::cpu_power_backend_flag_parallel
        || backend.reserved0 != 0
    {
        return c::cpu_capabilities_error_invalid;
    }
    let Some(identity) = lifecycle_identity() else {
        return c::cpu_capabilities_error_context;
    };
    if owner_identity(false) != Some(identity) {
        return c::cpu_capabilities_error_context;
    }
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    let Some(binding) = state.binding.as_mut() else {
        return c::cpu_capabilities_error_epoch;
    };
    if cookie == 0
        || (binding.owner, binding.epoch, binding.cookie) != (identity.0, identity.1, cookie)
    {
        return c::cpu_capabilities_error_epoch;
    }
    if state.closing
        || state.active != 0
        || binding.performance.is_some()
        || SYSTEM_TRANSITION.load(Ordering::Acquire)
    {
        return c::cpu_capabilities_error_busy;
    }
    binding.performance = Some(backend);
    0
}
pub(crate) fn register_cppc(cookie: u64, backend: c::CpuCppcBackend) -> i32 {
    if backend.version != 1
        || backend.size < mem::size_of::<c::CpuCppcBackend>() as u32
        || backend.execute == 0
        || backend.flags != c::cpu_power_backend_flag_parallel
        || backend.reserved0 != 0
    {
        return c::cpu_capabilities_error_invalid;
    }
    let Some(identity) = lifecycle_identity() else {
        return c::cpu_capabilities_error_context;
    };
    if owner_identity(false) != Some(identity) {
        return c::cpu_capabilities_error_context;
    }
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    let Some(binding) = state.binding.as_mut() else {
        return c::cpu_capabilities_error_epoch;
    };
    if cookie == 0
        || (binding.owner, binding.epoch, binding.cookie) != (identity.0, identity.1, cookie)
    {
        return c::cpu_capabilities_error_epoch;
    }
    if state.closing
        || state.active != 0
        || binding.cppc.is_some()
        || SYSTEM_TRANSITION.load(Ordering::Acquire)
    {
        return c::cpu_capabilities_error_busy;
    }
    binding.cppc = Some(backend);
    0
}
/// Called only with the caller's current-CPU Session already retained. This
/// independent resident membership survives callback return/caller cancellation.
pub(crate) fn control_acquire(cpu: u32, epoch: u64) -> Result<u64, i32> {
    let identity = owner_identity(false).ok_or(c::cpu_capabilities_error_context)?;
    let membership = life::probe_lease().ok_or(c::cpu_capabilities_error_busy)?;
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    if SYSTEM_TRANSITION.load(Ordering::Acquire) {
        return Err(c::cpu_capabilities_error_busy);
    }
    if cpu < 64 && firmware::busy_locked(&_runtime, 1u64 << cpu) {
        return Err(c::cpu_capabilities_error_busy);
    }
    if !state.binding.is_some_and(|b| {
        (b.owner, b.epoch) == identity && (b.performance.is_some() || b.cppc.is_some())
    }) || state.closing
    {
        return Err(c::cpu_capabilities_error_context);
    }
    let slot = state
        .controls
        .get_mut(cpu as usize)
        .ok_or(c::cpu_capabilities_error_index)?;
    if slot.is_some() {
        return Err(c::cpu_capabilities_error_busy);
    }
    let token = state.next_control;
    state.next_control = token.checked_add(1).ok_or(c::cpu_capabilities_error_busy)?;
    *slot = Some(Control {
        owner: identity.0,
        owner_epoch: identity.1,
        cpu_epoch: epoch,
        token,
        active: false,
        _membership: membership,
    });
    Ok(token)
}
pub(crate) fn control_release(cpu: u32, epoch: u64, token: u64) -> i32 {
    let Some(identity) = owner_identity(false) else {
        return c::cpu_capabilities_error_context;
    };
    let _runtime = RuntimeGuard::acquire();
    let state = unsafe { &mut *STATE.0.get() };
    let Some(slot) = state.controls.get_mut(cpu as usize) else {
        return c::cpu_capabilities_error_index;
    };
    let Some(control) = slot.as_ref() else {
        return c::cpu_capabilities_error_epoch;
    };
    if token == 0
        || (
            control.owner,
            control.owner_epoch,
            control.cpu_epoch,
            control.token,
        ) != (identity.0, identity.1, epoch, token)
    {
        return c::cpu_capabilities_error_epoch;
    }
    if control.active {
        return c::cpu_capabilities_error_busy;
    }
    *slot = None;
    0
}
pub(crate) struct ControlCall {
    cpu: u32,
    token: u64,
}
impl ControlCall {
    pub(crate) fn acquire(cpu: u32, epoch: u64, token: u64) -> Result<Self, i32> {
        let identity = owner_identity(false).ok_or(c::cpu_capabilities_error_context)?;
        let _runtime = RuntimeGuard::acquire();
        let state = unsafe { &mut *STATE.0.get() };
        let control = state
            .controls
            .get_mut(cpu as usize)
            .and_then(Option::as_mut)
            .ok_or(c::cpu_capabilities_error_epoch)?;
        if token == 0
            || (
                control.owner,
                control.owner_epoch,
                control.cpu_epoch,
                control.token,
            ) != (identity.0, identity.1, epoch, token)
        {
            return Err(c::cpu_capabilities_error_epoch);
        }
        if state.closing || control.active {
            return Err(c::cpu_capabilities_error_busy);
        }
        control.active = true;
        Ok(Self { cpu, token })
    }
}
impl Drop for ControlCall {
    fn drop(&mut self) {
        let _runtime = RuntimeGuard::acquire();
        let state = unsafe { &mut *STATE.0.get() };
        let control = state.controls[self.cpu as usize]
            .as_mut()
            .expect("live CPU control");
        assert!(control.token == self.token && control.active);
        control.active = false;
    }
}
struct Lease(Binding);
impl Lease {
    fn acquire(returning: Option<(u32, u64, u64)>) -> Result<Option<Self>, i32> {
        let binding = {
            let _runtime = RuntimeGuard::acquire();
            let state = unsafe { &mut *STATE.0.get() };
            let Some(binding) = state.binding else {
                return Ok(None);
            };
            if state.closing {
                return Err(c::cpu_capabilities_error_busy);
            }
            if SYSTEM_TRANSITION.load(Ordering::Acquire)
                && !returning.is_some_and(|(cpu, epoch, token)| {
                    state
                        .controls
                        .get(cpu as usize)
                        .and_then(Option::as_ref)
                        .is_some_and(|control| {
                            token != 0
                                && control.token == token
                                && control.cpu_epoch == epoch
                                && control.owner == binding.owner
                                && control.owner_epoch == binding.epoch
                        })
                })
            {
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
    let lease = match Lease::acquire(None) {
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
/// # Safety
/// Resident initialized fixed input and exclusive output/tail for this call.
/// A completed hardware attempt is represented by its receipt, not discarded
/// because its readback failed or because the caller later abandons the token.
pub(crate) unsafe extern "C" fn performance(
    request: *const c::CpuPerformanceRequest,
    out: *mut c::CpuPerformanceInfo,
) -> i32 {
    if request.is_null()
        || !request.is_aligned()
        || out.is_null()
        || !out.is_aligned()
        || unsafe {
            (*request).version != 1
                || (*request).size < mem::size_of::<c::CpuPerformanceRequest>() as u32
                || (*out).version != 1
                || (*out).size < mem::size_of::<c::CpuPerformanceInfo>() as u32
        }
    {
        return c::cpu_capabilities_error_invalid;
    }
    let request = unsafe { request.read() };
    let _session = match Session::acquire(request.cpu_index, request.epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    let returning = (request.operation == c::cpu_performance_op_release).then_some((
        request.cpu_index,
        request.epoch,
        request.lease,
    ));
    let lease = match Lease::acquire(returning) {
        Ok(Some(lease)) => lease,
        Ok(None) => return c::cpu_capabilities_error_unsupported,
        Err(reason) => return reason,
    };
    let Some(backend) = lease.0.performance else {
        return c::cpu_capabilities_error_unsupported;
    };
    let Some(_callback) = Callback::acquire(lease.0.owner) else {
        return c::cpu_capabilities_error_busy;
    };
    type Execute = unsafe extern "C" fn(
        usize,
        *const c::CpuPerformanceRequest,
        *mut c::CpuPerformanceInfo,
    ) -> i32;
    let execute: Execute = unsafe { mem::transmute(backend.execute as usize) };
    let mut value = c::CpuPerformanceInfo::default();
    let result = unsafe { execute(backend.context as usize, &request, &mut value) };
    if result != 0 {
        return result;
    }
    if value.version != 1
        || value.size != mem::size_of::<c::CpuPerformanceInfo>() as u32
        || value.cpu_index != request.cpu_index
        || value.epoch != request.epoch
    {
        return c::cpu_capabilities_error_invalid;
    }
    value.provider_generation = lease.0.cookie;
    unsafe { out.write(value) };
    0
}

/// # Safety
/// Same initialized resident input/output and complete receipt contract as performance.
pub(crate) unsafe extern "C" fn cppc(
    request: *const c::CpuPerformanceRequest,
    out: *mut c::CpuCppcInfo,
) -> i32 {
    if request.is_null()
        || !request.is_aligned()
        || out.is_null()
        || !out.is_aligned()
        || unsafe {
            (*request).version != 1
                || (*request).size < mem::size_of::<c::CpuPerformanceRequest>() as u32
                || (*out).version != 1
                || (*out).size < mem::size_of::<c::CpuCppcInfo>() as u32
        }
    {
        return c::cpu_capabilities_error_invalid;
    }
    let request = unsafe { request.read() };
    let _session = match Session::acquire(request.cpu_index, request.epoch) {
        Ok(session) => session,
        Err(reason) => return reason,
    };
    let returning = (request.operation == c::cpu_performance_op_release).then_some((
        request.cpu_index,
        request.epoch,
        request.lease,
    ));
    let lease = match Lease::acquire(returning) {
        Ok(Some(lease)) => lease,
        Ok(None) => return c::cpu_capabilities_error_unsupported,
        Err(reason) => return reason,
    };
    let Some(backend) = lease.0.cppc else {
        return c::cpu_capabilities_error_unsupported;
    };
    let Some(_callback) = Callback::acquire(lease.0.owner) else {
        return c::cpu_capabilities_error_busy;
    };
    type Execute =
        unsafe extern "C" fn(usize, *const c::CpuPerformanceRequest, *mut c::CpuCppcInfo) -> i32;
    let execute: Execute = unsafe { mem::transmute(backend.execute as usize) };
    let mut value = c::CpuCppcInfo::default();
    let result = unsafe { execute(backend.context as usize, &request, &mut value) };
    if result != 0 {
        return result;
    }
    if value.version != 1
        || value.size != mem::size_of::<c::CpuCppcInfo>() as u32
        || value.cpu_index != request.cpu_index
        || value.epoch != request.epoch
    {
        return c::cpu_capabilities_error_invalid;
    }
    value.provider_generation = lease.0.cookie;
    unsafe { out.write(value) };
    0
}
