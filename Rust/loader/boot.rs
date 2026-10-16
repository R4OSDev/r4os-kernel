//! Original linear BSP module/filesystem bootstrap. Peers retain their own
//! implementations; only this owner publishes loader readiness and config.
use core::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};

// As before, initialization is called by the linear BSP bootstrap. There is
// no additional reservation, retry policy or owner held across peer calls.
static MODULE_READY: AtomicBool = AtomicBool::new(false);
static LOADER_READY: AtomicBool = AtomicBool::new(false);
static CONFIG: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

/// # Safety
/// The BSP bootstrap supplies live peers that preserve their existing owner
/// contracts. Registry initialization has completed before it returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_module_boot_init() {
    if MODULE_READY.load(Ordering::Acquire) {
        return;
    }
    let peers = super::registry_control::native_peers();
    unsafe { super::registry_control::r4os_rust_mod_init(&peers) };
    MODULE_READY.store(true, Ordering::Release);
    crate::boot::log_route::puts(b"  Module registry ");
    crate::boot::log_route::puts(b"[OK]\r\n");
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_module_boot_ready() -> bool {
    MODULE_READY.load(Ordering::Acquire)
}

fn log(text: &[u8]) {
    crate::boot::log_route::puts(text);
}

/// # Safety
/// The linear BSP caller supplies live peer functions. Returned config stays
/// resident, and no peer recursively starts another bootstrap generation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_loader_boot_init() -> bool {
    if LOADER_READY.load(Ordering::Acquire) {
        return true;
    }
    if !unsafe { crate::storage::boot::r4os_rust_storage_boot(3, core::ptr::null()) != 0 } {
        let text = b"Loader boot before storage controllers";
        unsafe { crate::boot::fatal::r4os_rust_fatal_record(10, text.as_ptr(), text.len()) };
        return false;
    }
    super::performance::begin_filesystem_loader();
    let peers = super::registry_control::native_peers();
    unsafe { super::registry_loader::r4os_rust_mod_load_libraries(&peers) };
    unsafe { super::r4d::r4os_rust_r4d_discover() };
    super::registry_control::r4os_rust_mod_summary();
    let config: *const c_void = unsafe { crate::boot::config::load() }.cast();
    CONFIG.store(config.cast_mut(), Ordering::Relaxed);
    let fonts = crate::font::catalog_io::reload_installed();
    log(b"[FONT] catalog loaded=");
    crate::boot::log_route::dec(u64::from(fonts.registered));
    log(b" rejected=");
    crate::boot::log_route::dec(u64::from(fonts.rejected));
    log(b"\r\n");
    super::performance::finish_filesystem_loader();
    LOADER_READY.store(true, Ordering::Release);
    let text = b"  Loader config [OK]\r\n";
    log(text);
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_loader_boot_ready() -> bool {
    LOADER_READY.load(Ordering::Acquire)
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_loader_boot_config() -> *const c_void {
    if !LOADER_READY.load(Ordering::Acquire) {
        return core::ptr::null();
    }
    CONFIG.load(Ordering::Relaxed)
}
