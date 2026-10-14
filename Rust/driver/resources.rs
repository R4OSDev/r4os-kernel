//! Canonical R4D resource runtime. DriverApi's existing wait-spanning execution
//! owner serializes all calls except the passive clock. File plans belong to
//! the still-Zig loader/filesystem owner and retain its exact captured source.
use crate::driver::{
    registry::MAX_DRIVERS,
    resource_state::{self, Binding, State},
};
use crate::{
    contract as a,
    platform::{firmware_tables as firmware, monotonic},
};
use core::{cell::UnsafeCell, ffi::c_void, mem, ptr};

const EMPTY_BINDING: Binding = Binding {
    epoch: 0,
    slot: 0,
    module_generation: 0,
    closing: false,
};
struct ResourceState(UnsafeCell<State>);
// Before publication only the BSP writes; every later operation is serialized
// by DriverApi's exact instance execution guard, including synchronous I/O.
unsafe impl Sync for ResourceState {}
static STATE: ResourceState = ResourceState(UnsafeCell::new(State {
    next_epoch: 1,
    bindings: [EMPTY_BINDING; MAX_DRIVERS],
}));
#[repr(C)]
struct Lookup {
    index: usize,
    bytes: u64,
}
unsafe extern "C" {
    fn r4os_zig_driver_resource_name_valid(name: *const u8, len: usize) -> bool;
    fn r4os_zig_driver_resource_lookup(
        slot: usize,
        generation: u32,
        name: *const u8,
        len: usize,
        out: *mut Lookup,
    ) -> i32;
    fn r4os_zig_driver_resource_prepare(
        slot: usize,
        generation: u32,
        index: usize,
        plan: *mut c_void,
        bytes: *mut u64,
    ) -> i32;
    fn r4os_zig_driver_resource_read(
        plan: *const c_void,
        offset: u64,
        out: *mut u8,
        len: usize,
    ) -> bool;
}

/// # Safety
/// The returned resident state is used only under DriverApi's existing exact
/// execution guard. No concurrent mutation, reference or guard spans its I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_resource_state() -> *mut State {
    STATE.0.get()
}
unsafe fn binding(owner: u32) -> Option<Binding> {
    let mut value = ptr::null();
    if unsafe { resource_state::r4os_rust_resource_current(STATE.0.get(), owner, &mut value) } != 0
    {
        return None;
    }
    Some(unsafe { value.read() })
}
fn valid_read(len: usize, deadline: u64) -> bool {
    len != 0
        && len <= a::driver_resource_max_read_bytes as usize
        && deadline != 0
        && deadline != u64::MAX
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_resource_now() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(u64::MAX)
}

/// # Safety
/// Exclusive aligned Contract output and readable name bytes. DriverApi holds
/// the exact owner guard; catalog lookup performs no callback or release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_resource_stat(
    owner: u32,
    name: *const u8,
    len: usize,
    out: *mut a::DriverResourceInfo,
) -> i32 {
    unsafe {
        if (*out).version != 1
            || (*out).size < mem::size_of::<a::DriverResourceInfo>() as u32
            || !r4os_zig_driver_resource_name_valid(name, len)
        {
            return a::driver_resource_error_invalid;
        }
        out.write(a::DriverResourceInfo::default());
        let Some(b) = binding(owner) else {
            return a::driver_resource_error_stale;
        };
        let mut lookup = Lookup { index: 0, bytes: 0 };
        let status =
            r4os_zig_driver_resource_lookup(b.slot, b.module_generation, name, len, &mut lookup);
        if status != a::driver_resource_ok {
            return status;
        }
        let mut handle = 0;
        if resource_state::r4os_rust_resource_handle(
            STATE.0.get(),
            0,
            owner,
            lookup.index as u64,
            &mut handle,
        ) != 0
        {
            return a::driver_resource_error_stale;
        }
        out.write(a::DriverResourceInfo {
            handle,
            byte_length: lookup.bytes,
            module_generation: u64::from(b.module_generation),
            ..a::DriverResourceInfo::default()
        });
        a::driver_resource_ok
    }
}

/// # Safety
/// Output is writable resident caller storage and `plan` is exclusive loader
/// plan storage for the complete call. Its captured FileSource survives waits.
/// The exact execution guard remains installed; no Rust reference spans I/O.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_resource_read(
    owner: u32,
    id: u64,
    offset: u64,
    out: *mut u8,
    len: usize,
    deadline: u64,
    plan: *mut c_void,
) -> i32 {
    unsafe {
        if !valid_read(len, deadline) {
            return a::driver_resource_error_invalid;
        }
        let Some(b) = binding(owner) else {
            return a::driver_resource_error_stale;
        };
        let mut index = 0;
        if resource_state::r4os_rust_resource_handle(STATE.0.get(), 1, owner, id, &mut index) != 0 {
            return a::driver_resource_error_stale;
        }
        let mut bytes = 0;
        let status = r4os_zig_driver_resource_prepare(
            b.slot,
            b.module_generation,
            index as usize,
            plan,
            &mut bytes,
        );
        if status != a::driver_resource_ok {
            return status;
        }
        if offset > bytes || len as u64 > bytes - offset {
            return a::driver_resource_error_invalid;
        }
        let started = r4os_rust_driver_resource_now();
        if started >= deadline {
            return a::driver_resource_error_deadline;
        }
        let ok = r4os_zig_driver_resource_read(plan, offset, out, len);
        let finished = r4os_rust_driver_resource_now();
        if finished < started || finished >= deadline {
            return a::driver_resource_error_deadline;
        }
        if !ok {
            return a::driver_resource_error_io;
        }
        i32::try_from(len).expect("Validated resource read length exceeds i32")
    }
}
fn firmware_error(error: i32) -> i32 {
    match error {
        -1 => a::driver_resource_error_invalid,
        -5 => a::driver_resource_error_stale,
        -4 => a::driver_resource_error_not_found,
        -2 | -3 => a::driver_resource_error_source,
        _ => panic!("Invalid firmware catalog status"),
    }
}

/// # Safety
/// Output permits its version/size header and declared Contract result. The
/// caller holds the exact execution guard; the boot catalog stays resident.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_resource_acpi_stat(
    owner: u32,
    signature: u32,
    index: u32,
    out: *mut a::DriverFirmwareTableInfo,
) -> i32 {
    unsafe {
        if out.is_null()
            || !out
                .addr()
                .is_multiple_of(mem::align_of::<a::DriverFirmwareTableInfo>())
            || (*out).version != 1
            || (*out).size < mem::size_of::<a::DriverFirmwareTableInfo>() as u32
        {
            return a::driver_resource_error_invalid;
        }
        if binding(owner).is_none() {
            return a::driver_resource_error_stale;
        }
        let mut info = firmware::Info::default();
        let code =
            firmware::r4os_rust_firmware_stat(firmware::catalog(), signature, index, &mut info);
        if code != 0 {
            return firmware_error(code);
        }
        out.write(a::DriverFirmwareTableInfo {
            handle: info.handle,
            byte_length: info.bytes,
            generation: info.generation,
            signature: info.signature,
            revision: u32::from(info.revision),
            ..a::DriverFirmwareTableInfo::default()
        });
        a::driver_resource_ok
    }
}

/// # Safety
/// Resident writable output, exact execution guard and immutable boot catalog
/// remain live through this synchronous read. Original alias checks are kept.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_resource_acpi_read(
    owner: u32,
    handle: u64,
    offset: u64,
    out: *mut u8,
    len: usize,
    deadline: u64,
) -> i32 {
    unsafe {
        if !valid_read(len, deadline) {
            return a::driver_resource_error_invalid;
        }
        if binding(owner).is_none() {
            return a::driver_resource_error_stale;
        }
        let started = r4os_rust_driver_resource_now();
        if started >= deadline {
            return a::driver_resource_error_deadline;
        }
        let code =
            firmware::r4os_rust_firmware_read(firmware::catalog(), handle, offset, out, len as u64);
        if code != 0 {
            return firmware_error(code);
        }
        let finished = r4os_rust_driver_resource_now();
        if finished < started || finished >= deadline {
            return a::driver_resource_error_deadline;
        }
        i32::try_from(len).expect("Validated firmware read length exceeds i32")
    }
}
