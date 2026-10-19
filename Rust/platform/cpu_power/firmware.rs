//! Optional firmware transport routing. Interpretation and register work stay
//! at CPUPOWER/ACPIPLAT. Every wait retains exact task, module and CPU leases.
use super::{Callback, owner_identity};
use crate::{
    arch::{cpu_lifecycle as life, runtime::RuntimeGuard},
    contract as c,
    driver::{api_admission as admission, heap, irq_router},
    platform::cpu_capabilities as catalog,
    sched::task_context::Admission,
};
use core::{cell::UnsafeCell, mem};
#[derive(Clone, Copy)]
struct Binding {
    owner: u32,
    epoch: u64,
    cookie: u64,
    backend: c::CpuFirmwareBackend,
    closing: bool,
    active: u32,
}
struct Domain {
    cpu: u32,
    epoch: u64,
    mask: u64,
    token: u64,
    facade: u64,
    transport: u64,
    _membership: life::ProbeLease,
}
struct State {
    bindings: [Option<Binding>; 2],
    domains: [Option<Domain>; 64],
    next_cookie: u64,
    next_token: u64,
}
struct Storage(UnsafeCell<State>);
// All metadata copies/transitions use the existing short Runtime owner. No
// reference or owner guard survives callback execution, AML or caller waits.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    bindings: [None; 2],
    domains: [const { None }; 64],
    next_cookie: 1,
    next_token: 1,
}));
fn identity() -> Option<(u32, u64)> {
    if irq_router::r4os_rust_irq_router_local(false) != 0 {
        return None;
    }
    let owner = admission::r4os_rust_driver_api_runtime_owner(true);
    let mut info = c::DriverHeapStats::default();
    if owner == 0
        || unsafe { heap::r4os_rust_driver_heap_stats(owner, &mut info) } != 0
        || info.owner_epoch == 0
        || info.closing != 0
    {
        return None;
    }
    Some((owner, info.owner_epoch))
}
pub(crate) fn register(role: u32, backend: c::CpuFirmwareBackend) -> Result<u64, i32> {
    if role > 1
        || backend.version != 1
        || backend.size < mem::size_of::<c::CpuFirmwareBackend>() as u32
        || backend.execute == 0
        || backend.flags != c::cpu_power_backend_flag_parallel
        || backend.reserved0 != 0
    {
        return Err(c::cpu_capabilities_error_invalid);
    }
    if !admission::callbacks_parallel() || admission::r4os_rust_driver_api_owner_scalar(1) == 0 {
        return Err(c::cpu_capabilities_error_context);
    }
    let id = identity().ok_or(c::cpu_capabilities_error_context)?;
    let runtime = RuntimeGuard::acquire();
    if role == 1 && !super::provider_matches_locked(&runtime, id) {
        return Err(c::cpu_capabilities_error_context);
    }
    let s = unsafe { &mut *STATE.0.get() };
    if s.bindings[role as usize].is_some()
        || super::transitioning()
        || s.domains.iter().any(Option::is_some)
    {
        return Err(c::cpu_capabilities_error_busy);
    }
    if s.bindings[(1 - role) as usize].is_some_and(|b| b.owner == id.0) {
        return Err(c::cpu_capabilities_error_context);
    }
    let cookie = s.next_cookie;
    s.next_cookie = cookie
        .checked_add(1)
        .ok_or(c::cpu_capabilities_error_busy)?;
    s.bindings[role as usize] = Some(Binding {
        owner: id.0,
        epoch: id.1,
        cookie,
        backend,
        closing: false,
        active: 0,
    });
    Ok(cookie)
}
pub(crate) extern "C" fn unregister(role: u32, cookie: u64) -> i32 {
    if role > 1 {
        return c::cpu_capabilities_error_invalid;
    }
    if admission::r4os_rust_driver_api_owner_scalar(1) == 0 {
        return c::cpu_capabilities_error_context;
    }
    let Some(id) = identity() else {
        return c::cpu_capabilities_error_context;
    };
    let _runtime = RuntimeGuard::acquire();
    let s = unsafe { &mut *STATE.0.get() };
    let Some(b) = s.bindings[role as usize] else {
        return c::cpu_capabilities_error_epoch;
    };
    if (b.owner, b.epoch, b.cookie) != (id.0, id.1, cookie) || cookie == 0 {
        return c::cpu_capabilities_error_epoch;
    }
    if b.active != 0 || s.domains.iter().any(Option::is_some) {
        return c::cpu_capabilities_error_busy;
    }
    s.bindings[role as usize] = None;
    0
}
pub(crate) fn prepare_close_locked(_runtime: &RuntimeGuard, owner: u32) -> bool {
    let s = unsafe { &mut *STATE.0.get() };
    if s.bindings
        .iter()
        .flatten()
        .any(|b| b.owner == owner && (b.active != 0 || s.domains.iter().any(Option::is_some)))
    {
        return false;
    }
    for b in s.bindings.iter_mut().flatten().filter(|b| b.owner == owner) {
        b.closing = true;
    }
    true
}
pub(crate) fn cancel_close_locked(_runtime: &RuntimeGuard, owner: u32) {
    for b in unsafe { &mut *STATE.0.get() }
        .bindings
        .iter_mut()
        .flatten()
        .filter(|b| b.owner == owner)
    {
        b.closing = false;
    }
}
pub(crate) fn finish_close_locked(_runtime: &RuntimeGuard, owner: u32) -> bool {
    let s = unsafe { &mut *STATE.0.get() };
    if s.bindings.iter().flatten().any(|b| {
        b.owner == owner && (!b.closing || b.active != 0 || s.domains.iter().any(Option::is_some))
    }) {
        return false;
    }
    for b in &mut s.bindings {
        if b.is_some_and(|b| b.owner == owner) {
            *b = None;
        }
    }
    true
}
pub(crate) fn facade_live_locked(_runtime: &RuntimeGuard, owner: u32, epoch: u64) -> bool {
    unsafe { &*STATE.0.get() }.bindings[1].is_some_and(|b| (b.owner, b.epoch) == (owner, epoch))
}
pub(crate) fn busy_locked(_runtime: &RuntimeGuard, mask: u64) -> bool {
    unsafe { &*STATE.0.get() }
        .domains
        .iter()
        .flatten()
        .any(|d| d.mask & mask != 0)
}
pub(crate) fn returned_locked(_runtime: &RuntimeGuard) -> bool {
    let s = unsafe { &*STATE.0.get() };
    s.domains.iter().all(Option::is_none) && s.bindings.iter().flatten().all(|b| b.active == 0)
}
pub(crate) fn control(
    operation: u32,
    cpu: u32,
    epoch: u64,
    mask: u64,
    token: u64,
) -> Result<u64, i32> {
    if cpu >= 64
        || mask == 0
        || mask & (1u64 << cpu) == 0
        || operation == 1 && token != 0
        || !matches!(operation, 1 | 2)
    {
        return Err(c::cpu_capabilities_error_invalid);
    }
    let id = identity().ok_or(c::cpu_capabilities_error_context)?;
    let membership = if operation == 1 {
        Some(life::probe_lease().ok_or(c::cpu_capabilities_error_busy)?)
    } else {
        None
    };
    if operation == 1 {
        for c in 0..64 {
            if mask & (1u64 << c) != 0 {
                let cap = catalog::info(c)?;
                if cap.flags & c::cpu_capabilities_flag_schedulable == 0
                    || c == cpu && cap.epoch != epoch
                {
                    return Err(c::cpu_capabilities_error_epoch);
                }
            }
        }
    }
    let runtime = RuntimeGuard::acquire();
    let s = unsafe { &mut *STATE.0.get() };
    let (Some(transport), Some(facade)) = (s.bindings[0], s.bindings[1]) else {
        return Err(c::cpu_capabilities_error_unsupported);
    };
    if (transport.owner, transport.epoch) != id {
        return Err(c::cpu_capabilities_error_context);
    }
    if operation == 2 {
        let Some(d) = s.domains[cpu as usize].as_ref() else {
            return Err(c::cpu_capabilities_error_epoch);
        };
        if (d.cpu, d.epoch, d.mask, d.token, d.facade, d.transport)
            != (cpu, epoch, mask, token, facade.cookie, transport.cookie)
            || token == 0
        {
            return Err(c::cpu_capabilities_error_epoch);
        }
        s.domains[cpu as usize] = None;
        return Ok(0);
    }
    if transport.closing
        || facade.closing
        || transport.active == 0
        || facade.active == 0
        || super::transitioning()
        || s.domains.iter().flatten().any(|d| d.mask & mask != 0)
        || super::direct_busy_locked(&runtime, mask)
    {
        return Err(c::cpu_capabilities_error_busy);
    }
    let value = s.next_token;
    s.next_token = value.checked_add(1).ok_or(c::cpu_capabilities_error_busy)?;
    s.domains[cpu as usize] = Some(Domain {
        cpu,
        epoch,
        mask,
        token: value,
        facade: facade.cookie,
        transport: transport.cookie,
        _membership: membership.expect("new firmware membership"),
    });
    Ok(value)
}
struct Lease {
    role: usize,
    binding: Binding,
}
impl Lease {
    fn acquire(role: usize, request: &c::CpuPerformanceRequest) -> Result<Self, i32> {
        let id = if role == 0 {
            Some(owner_identity(false).ok_or(c::cpu_capabilities_error_context)?)
        } else {
            None
        };
        let _runtime = RuntimeGuard::acquire();
        let s = unsafe { &mut *STATE.0.get() };
        let Some(b) = s.bindings[role] else {
            return Err(c::cpu_capabilities_error_unsupported);
        };
        if let Some(id) = id
            && !s.bindings[1].is_some_and(|f| (f.owner, f.epoch) == id && f.active > 0)
        {
            return Err(c::cpu_capabilities_error_context);
        }
        let returning = request.operation == c::cpu_performance_op_release
            && s.domains
                .get(request.cpu_index as usize)
                .and_then(Option::as_ref)
                .is_some_and(|d| {
                    d.token == request.lease && d.epoch == request.epoch && request.lease != 0
                });
        if b.closing || super::transitioning() && !returning {
            return Err(c::cpu_capabilities_error_busy);
        }
        s.bindings[role].as_mut().expect("firmware binding").active = b
            .active
            .checked_add(1)
            .ok_or(c::cpu_capabilities_error_busy)?;
        Ok(Self { role, binding: b })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _runtime = RuntimeGuard::acquire();
        let b = unsafe { &mut *STATE.0.get() }.bindings[self.role]
            .as_mut()
            .expect("live firmware provider");
        assert!(b.cookie == self.binding.cookie && b.active > 0);
        b.active -= 1;
    }
}
unsafe fn execute(
    role: usize,
    request: *const c::CpuPerformanceRequest,
    out: *mut c::CpuFirmwareInfo,
) -> i32 {
    if request.is_null()
        || !request.is_aligned()
        || out.is_null()
        || !out.is_aligned()
        || unsafe {
            (*request).version != 1
                || (*request).size < 64
                || (*out).version != 1
                || (*out).size < 192
        }
    {
        return c::cpu_capabilities_error_invalid;
    }
    let request = unsafe { request.read() };
    if request.cpu_index >= 64
        || request.reserved0 != 0
        || request.operation > c::cpu_performance_op_enable
    {
        return c::cpu_capabilities_error_invalid;
    }
    if catalog::flags() & 0x200 == 0 || irq_router::r4os_rust_irq_router_local(false) != 0 {
        return c::cpu_capabilities_error_context;
    }
    let Some(_task) = Admission::acquire(true) else {
        return c::cpu_capabilities_error_context;
    };
    let Some(_membership) = life::probe_lease() else {
        return c::cpu_capabilities_error_busy;
    };
    let cap = match catalog::info(request.cpu_index) {
        Ok(v) => v,
        Err(e) => return e,
    };
    if cap.epoch != request.epoch || request.epoch == 0 {
        return c::cpu_capabilities_error_epoch;
    }
    if cap.flags & c::cpu_capabilities_flag_schedulable == 0 {
        return c::cpu_capabilities_error_offline;
    }
    let lease = match Lease::acquire(role, &request) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let mut stats = c::DriverHeapStats::default();
    if unsafe { heap::r4os_rust_driver_heap_stats(lease.binding.owner, &mut stats) } != 0
        || stats.owner_epoch != lease.binding.epoch
        || stats.closing != 0
    {
        return c::cpu_capabilities_error_busy;
    }
    let previous = if role == 0 {
        owner_identity(false)
    } else {
        None
    };
    let _route = if let Some(previous) = previous {
        match admission::FirmwareRoute::acquire(previous.0, lease.binding.owner) {
            Some(v) => Some(v),
            None => return c::cpu_capabilities_error_busy,
        }
    } else {
        None
    };
    let _callback = if role == 1 {
        match Callback::acquire(lease.binding.owner) {
            Some(v) => Some(v),
            None => return c::cpu_capabilities_error_busy,
        }
    } else {
        None
    };
    type Execute = unsafe extern "C" fn(
        usize,
        *const c::CpuPerformanceRequest,
        *mut c::CpuFirmwareInfo,
    ) -> i32;
    let call: Execute = unsafe { mem::transmute(lease.binding.backend.execute as usize) };
    let mut info = c::CpuFirmwareInfo::default();
    let result = unsafe { call(lease.binding.backend.context as usize, &request, &mut info) };
    if result != 0 {
        return result;
    }
    if info.version != 1
        || info.size != 192
        || info.cpu_index != request.cpu_index
        || info.epoch != request.epoch
    {
        return c::cpu_capabilities_error_invalid;
    }
    if role == 0 {
        info.firmware_generation = lease.binding.cookie;
    } else {
        let runtime = RuntimeGuard::acquire();
        info.provider_generation =
            super::provider_cookie_locked(&runtime, lease.binding.owner, lease.binding.epoch)
                .unwrap_or(0);
        if info.provider_generation == 0 {
            return c::cpu_capabilities_error_epoch;
        }
    }
    unsafe { out.write(info) };
    0
}
/// # Safety
/// Initialized exclusive resident records remain pinned through true callback completion.
pub(crate) unsafe extern "C" fn call(
    request: *const c::CpuPerformanceRequest,
    out: *mut c::CpuFirmwareInfo,
) -> i32 {
    unsafe { execute(1, request, out) }
}
/// # Safety
/// Same resident input/output lifetime as call; only the admitted facade routes here.
pub(crate) unsafe extern "C" fn transport(
    request: *const c::CpuPerformanceRequest,
    out: *mut c::CpuFirmwareInfo,
) -> i32 {
    unsafe { execute(0, request, out) }
}
/// # Safety
/// Exclusive initialized resident output; authenticated driver worker/init/callback only.
pub(crate) unsafe extern "C" fn cpu_info(cpu: u32, out: *mut c::CpuCapabilitiesInfo) -> i32 {
    if out.is_null()
        || !out.is_aligned()
        || unsafe {
            (*out).version != 1 || (*out).size < mem::size_of::<c::CpuCapabilitiesInfo>() as u32
        }
    {
        return c::cpu_capabilities_error_invalid;
    }
    if identity().is_none() {
        return c::cpu_capabilities_error_context;
    }
    match catalog::info(cpu) {
        Ok(v) => {
            unsafe { out.write(v) };
            0
        }
        Err(e) => e,
    }
}
