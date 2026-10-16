//! Existing program providers publish together before the R4DEV API table.
//! They bind the exact native program owners, without a callback registry or
//! another copy of program state. Legacy entries retain their exact prefixes.
use crate::{
    build_bridge::config,
    contract as c,
    program::{diagnostics, execution_inventory},
};
use core::{
    mem, ptr,
    sync::atomic::{AtomicBool, Ordering},
};
static INSTALLED: AtomicBool = AtomicBool::new(false);
pub(crate) fn install() {
    INSTALLED.store(true, Ordering::Release);
}
pub(crate) fn installed() -> bool {
    INSTALLED.load(Ordering::Acquire)
}
const STORAGE_V1_BYTES: usize = 256;
/// # Safety
/// Caller owns the original 256-byte v1 prefix, not necessarily a v2 record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_program_instance_storage_summary(
    out: *mut c::ProgramInstanceStorageSummary,
) -> i32 {
    let present = installed();
    let mut value = c::ProgramInstanceStorageSummary::default();
    if present {
        unsafe { diagnostics::r4os_rust_program_diagnostic_storage(&mut value) };
    }
    value.version = 1;
    value.size = STORAGE_V1_BYTES as u32;
    unsafe {
        ptr::copy_nonoverlapping(
            (&value as *const c::ProgramInstanceStorageSummary).cast::<u8>(),
            out.cast::<u8>(),
            STORAGE_V1_BYTES,
        )
    };
    i32::from(present)
}
/// # Safety
/// Caller owns the header and advertised writable canonical v2 record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_program_instance_storage_summary_v2(
    out: *mut c::ProgramInstanceStorageSummary,
) -> i32 {
    unsafe {
        if (*out).version < 2
            || (*out).size < mem::size_of::<c::ProgramInstanceStorageSummary>() as u32
        {
            return -1;
        }
        let present = installed();
        let mut value = c::ProgramInstanceStorageSummary::default();
        if present {
            diagnostics::r4os_rust_program_diagnostic_storage(&mut value);
        }
        value.version = 2;
        value.size = mem::size_of::<c::ProgramInstanceStorageSummary>() as u32;
        out.write(value);
        i32::from(present)
    }
}
/// # Safety
/// Exclusive resident canonical result; this entry explicitly runs a probe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_program_instance_storage_self_test(
    out: *mut c::ProgramInstanceStorageSelfTestResult,
) -> i32 {
    let mut value = c::ProgramInstanceStorageSelfTestResult::default();
    if !installed() {
        unsafe { out.write(value) };
        return 0;
    }
    let result = unsafe { diagnostics::r4os_rust_program_diagnostic_storage_test(&mut value) };
    value.version = 1;
    value.size = mem::size_of::<c::ProgramInstanceStorageSelfTestResult>() as u32;
    unsafe { out.write(value) };
    result
}
/// # Safety
/// Caller owns exactly the unchanged v1 registry-summary record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_program_registry_summary(
    out: *mut c::ProgramRegistrySummary,
) -> i32 {
    let mut legacy = c::ProgramRegistrySummary::default();
    if !installed() {
        unsafe { out.write(legacy) };
        return 0;
    }
    let mut value = c::ProgramRegistrySummaryV2::default();
    unsafe {
        diagnostics::r4os_rust_program_diagnostic_registry(&mut value);
        ptr::copy_nonoverlapping(
            (&value as *const c::ProgramRegistrySummaryV2).cast::<u8>(),
            (&mut legacy as *mut c::ProgramRegistrySummary).cast::<u8>(),
            mem::size_of::<c::ProgramRegistrySummary>(),
        );
    }
    legacy.version = 1;
    legacy.size = mem::size_of::<c::ProgramRegistrySummary>() as u32;
    unsafe { out.write(legacy) };
    1
}
/// # Safety
/// Caller owns the original v1 self-test input/output record to completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_program_registry_self_test(
    out: *mut c::ProgramRegistrySelfTestResult,
) -> i32 {
    let mut value = c::ProgramRegistrySelfTestResultV2 {
        operation: unsafe { (*out).operation },
        ..Default::default()
    };
    if !installed() {
        unsafe {
            out.write(c::ProgramRegistrySelfTestResult {
                operation: value.operation,
                ..Default::default()
            })
        };
        return 0;
    }
    let result = unsafe { diagnostics::r4os_rust_program_diagnostic_registry_test(&mut value) };
    let mut legacy = c::ProgramRegistrySelfTestResult::default();
    unsafe {
        ptr::copy_nonoverlapping(
            (&value as *const c::ProgramRegistrySelfTestResultV2).cast::<u8>(),
            (&mut legacy as *mut c::ProgramRegistrySelfTestResult).cast::<u8>(),
            mem::size_of::<c::ProgramRegistrySelfTestResult>(),
        );
    }
    legacy.version = 1;
    legacy.size = mem::size_of::<c::ProgramRegistrySelfTestResult>() as u32;
    unsafe { out.write(legacy) };
    result
}
/// # Safety
/// Exclusive resident canonical registry-summary output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_program_registry_summary_v2(
    out: *mut c::ProgramRegistrySummaryV2,
) -> i32 {
    let mut value = c::ProgramRegistrySummaryV2::default();
    if !installed() {
        unsafe { out.write(value) };
        return 0;
    }
    unsafe { diagnostics::r4os_rust_program_diagnostic_registry(&mut value) };
    value.version = 2;
    value.size = mem::size_of::<c::ProgramRegistrySummaryV2>() as u32;
    unsafe { out.write(value) };
    1
}
/// # Safety
/// Exclusive canonical v2 operation record retained across the explicit probe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_program_registry_self_test_v2(
    out: *mut c::ProgramRegistrySelfTestResultV2,
) -> i32 {
    let mut value = c::ProgramRegistrySelfTestResultV2 {
        operation: unsafe { (*out).operation },
        lifecycle_phase: unsafe { (*out).lifecycle_phase },
        requested_next_id: unsafe { (*out).requested_next_id },
        ..Default::default()
    };
    if !installed() {
        unsafe { out.write(value) };
        return 0;
    }
    let result = unsafe { diagnostics::r4os_rust_program_diagnostic_registry_test(&mut value) };
    value.version = 2;
    value.size = mem::size_of::<c::ProgramRegistrySelfTestResultV2>() as u32;
    unsafe { out.write(value) };
    result
}
/// # Safety
/// Exclusive resident canonical execution inventory summary.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_execution_inventory_summary(
    out: *mut c::ProgramInventorySummary,
) -> i32 {
    let mut value = c::ProgramInventorySummary::default();
    if !installed() {
        unsafe { out.write(value) };
        return 0;
    }
    let result = if unsafe { execution_inventory::r4os_rust_program_execution_summary(&mut value) }
    {
        0
    } else {
        -5
    };
    value.version = u32::from(c::program_inventory_version);
    value.size = mem::size_of::<c::ProgramInventorySummary>() as u32;
    unsafe { out.write(value) };
    result
}
/// # Safety
/// Caller owns the header and advertised current kernel-version record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4dev_kernel_version(out: *mut c::KernelVersion) -> i32 {
    unsafe {
        if (*out).version < 1 || (*out).size < mem::size_of::<c::KernelVersion>() as u32 {
            return -1;
        }
        out.write(c::KernelVersion {
            version: 1,
            size: mem::size_of::<c::KernelVersion>() as u32,
            major: config::KERNEL_MAJOR,
            minor: config::KERNEL_MINOR,
            patch: config::KERNEL_PATCH,
            ..Default::default()
        });
    }
    1
}
const _: () = {
    assert!(mem::size_of::<c::ProgramRegistrySummary>() == 160);
    assert!(mem::size_of::<c::ProgramRegistrySummaryV2>() == 224);
    assert!(mem::size_of::<c::ProgramRegistrySelfTestResult>() == 64);
    assert!(mem::size_of::<c::ProgramRegistrySelfTestResultV2>() == 136);
    assert!(mem::size_of::<c::ProgramInventorySummary>() == 160);
    assert!(mem::size_of::<c::KernelVersion>() == 24);
    assert!(mem::size_of::<c::ProgramInstanceStorageSummary>() >= STORAGE_V1_BYTES);
};
