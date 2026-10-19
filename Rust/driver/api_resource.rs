//! Original optional resource/platform API admission. File I/O retains its
//! captured loader plan and the exact wait-spanning lifecycle owner.
use crate::{
    contract as a,
    driver::{
        api_admission as admission, api_runtime as runtime, heap, irq_router, resource_state,
        resources,
    },
    platform::firmware_view,
};
use core::{mem::size_of, ptr};
struct OwnerGuard(u32);
impl OwnerGuard {
    fn enter() -> Result<Self, i32> {
        if irq_router::r4os_rust_irq_router_local(false) != 0
            || admission::r4os_rust_driver_api_storage_owner() != 0
        {
            return Err(a::driver_resource_error_owner);
        }
        let id = admission::r4os_rust_driver_api_runtime_owner(false);
        if id == 0 {
            return Err(a::driver_resource_error_owner);
        }
        if !admission::r4os_rust_driver_api_enter(id, 0) {
            return Err(a::driver_resource_error_busy);
        }
        Ok(Self(id))
    }
}
impl Drop for OwnerGuard {
    fn drop(&mut self) {
        admission::r4os_rust_driver_api_leave();
    }
}

/// # Safety
/// The caller provides an aligned resident versioned C prefix of its declared
/// size and retains the admitted driver identity through the call and any wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_resource_api(out: *mut a::DriverResourceApi) -> i32 {
    let guard = match OwnerGuard::enter() {
        Ok(g) => g,
        Err(c) => return c,
    };
    let mut binding = ptr::null();
    if unsafe {
        resource_state::r4os_rust_resource_current(
            resources::r4os_rust_driver_resource_state(),
            guard.0,
            &mut binding,
        )
    } != 0
    {
        return a::driver_resource_error_stale;
    }
    let value = a::DriverResourceApi {
        stat: resource_stat as *const () as usize as u64,
        read_at: resource_read as *const () as usize as u64,
        now_ns: resources::r4os_rust_driver_resource_now as *const () as usize as u64,
        acpi_stat: resource_acpi_stat as *const () as usize as u64,
        acpi_read_at: resource_acpi_read as *const () as usize as u64,
        platform_query: platform_query as *const () as usize as u64,
        cpu_power_query: super::api_cpu::query as *const () as usize as u64,
        ..a::DriverResourceApi::default()
    };
    if !unsafe { resource_state::r4os_rust_resource_publish(out, &value) } {
        return a::driver_resource_error_invalid;
    }
    a::driver_resource_ok
}
fn identity() -> Option<(u32, u64)> {
    let id = runtime::r4os_rust_driver_heap_owner();
    if id == 0 {
        return None;
    }
    let mut stats = a::DriverHeapStats::default();
    if unsafe { heap::r4os_rust_driver_heap_stats(id, &mut stats) } != 0
        || stats.owner_epoch == 0
        || stats.closing != 0
    {
        return None;
    }
    Some((id, stats.owner_epoch))
}
unsafe extern "C" fn platform_query(out: *mut a::DriverPlatformApi) -> i32 {
    if identity().is_none() {
        return a::driver_resource_error_owner;
    }
    if out.is_null()
        || !(out as usize).is_multiple_of(core::mem::align_of::<a::DriverPlatformApi>())
        || unsafe { (*out).version != 1 || (*out).size < size_of::<a::DriverPlatformApi>() as u32 }
    {
        return a::driver_resource_error_invalid;
    }
    unsafe {
        ptr::write(
            out,
            a::DriverPlatformApi {
                rsdp: platform_rsdp as *const () as usize as u64,
                physical_view: platform_physical as *const () as usize as u64,
                input_submit: platform_input as *const () as usize as u64,
                ..a::DriverPlatformApi::default()
            },
        )
    };
    a::driver_resource_ok
}
extern "C" fn platform_rsdp() -> u64 {
    if identity().is_none() {
        0
    } else {
        firmware_view::root()
    }
}
unsafe extern "C" fn platform_physical(physical: u64, bytes: u64, out: *mut u64) -> i32 {
    if identity().is_none() {
        return a::driver_resource_error_owner;
    }
    if out.is_null() || !(out as usize).is_multiple_of(8) {
        return a::driver_resource_error_invalid;
    }
    let Some(address) = firmware_view::view(physical, bytes) else {
        return a::driver_resource_error_source;
    };
    unsafe { *out = address };
    a::driver_resource_ok
}
extern "C" fn platform_input(kind: u32, value: u32) -> i32 {
    let Some((id, epoch)) = identity() else {
        return a::driver_resource_error_owner;
    };
    crate::input::platform_input::r4os_rust_input_platform_submit(id, epoch, kind, value)
}
unsafe extern "C" fn resource_stat(
    name: *const u8,
    length: u32,
    out: *mut a::DriverResourceInfo,
) -> i32 {
    let g = match OwnerGuard::enter() {
        Ok(g) => g,
        Err(c) => return c,
    };
    if length == 0 || length > 63 {
        return a::driver_resource_error_invalid;
    }
    unsafe { resources::r4os_rust_driver_resource_stat(g.0, name, length as usize, out) }
}
unsafe extern "C" fn resource_read(
    id: u64,
    offset: u64,
    out: *mut u8,
    length: u32,
    deadline: u64,
) -> i32 {
    let g = match OwnerGuard::enter() {
        Ok(g) => g,
        Err(c) => return c,
    };
    if length == 0 || length > a::driver_resource_max_read_bytes {
        return a::driver_resource_error_invalid;
    }
    unsafe { driver_resource_read_owned(g.0, id, offset, out, length, deadline) }
}
unsafe extern "C" fn resource_acpi_stat(
    signature: u32,
    index: u32,
    out: *mut a::DriverFirmwareTableInfo,
) -> i32 {
    let g = match OwnerGuard::enter() {
        Ok(g) => g,
        Err(c) => return c,
    };
    unsafe { resources::r4os_rust_driver_resource_acpi_stat(g.0, signature, index, out) }
}
unsafe extern "C" fn resource_acpi_read(
    id: u64,
    offset: u64,
    out: *mut u8,
    length: u32,
    deadline: u64,
) -> i32 {
    let g = match OwnerGuard::enter() {
        Ok(g) => g,
        Err(c) => return c,
    };
    if out.is_null() || length == 0 || length > a::driver_resource_max_read_bytes {
        return a::driver_resource_error_invalid;
    }
    unsafe {
        resources::r4os_rust_driver_resource_acpi_read(
            g.0,
            id,
            offset,
            out,
            length as usize,
            deadline,
        )
    }
}

unsafe fn driver_resource_read_owned(
    id: u32,
    resource: u64,
    offset: u64,
    out: *mut u8,
    length: u32,
    deadline: u64,
) -> i32 {
    let mut plan = core::mem::MaybeUninit::<crate::driver::resources::ReadPlan>::uninit();
    unsafe {
        crate::driver::resources::r4os_rust_driver_resource_read(
            id,
            resource,
            offset,
            out,
            length as usize,
            deadline,
            plan.as_mut_ptr().cast(),
        )
    }
}
