//! Singleton service boot publication; external SERVMAN retains policy.
use crate::boot::foundation;
use core::sync::atomic::{AtomicBool, Ordering};
static INITIALIZED: AtomicBool = AtomicBool::new(false);
/// # Safety
/// Original singleton BSP boot admission, before any live service instance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_service_boot_init() {
    if INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    unsafe { super::ffi::r4os_rust_services_init() };
    INITIALIZED.store(true, Ordering::Release);
    foundation::puts(b"  Service registry ");
    foundation::puts(b"[OK]\r\n");
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_service_boot_ready() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}
