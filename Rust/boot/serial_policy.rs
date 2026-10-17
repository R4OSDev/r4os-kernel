//! Registry is read once from the boot shell task, never from output/IRQ/
//! panic paths. Only a well-formed explicit BOOL0 disables postboot debug.
use crate::{build_config::config, contract as c};
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering::SeqCst};
static LOAD: AtomicU8 = AtomicU8::new(0);
static BOOT_COMPLETE: AtomicBool = AtomicBool::new(false);
static KEEP: AtomicBool = AtomicBool::new(true);
static APPLIED: AtomicBool = AtomicBool::new(false);
pub(crate) fn prepare() {
    if !config::ENABLE_COM1_DEBUG || LOAD.compare_exchange(0, 1, SeqCst, SeqCst).is_err() {
        return;
    }
    let mut info = c::RegistryValueInfo::default();
    let mut value = 1;
    let result = unsafe {
        crate::program::system::registry_api::r4os_rust_sys_registry_get(
            c"SYSTEM\\Kernel\\Debug".as_ptr().cast(),
            c"Com1DebugAfterBoot".as_ptr().cast(),
            &mut info,
            &mut value,
            1,
        )
    };
    KEEP.store(
        !(result == 1
            && info.value_type == c::registry_value_type_bool
            && info.data_len == 1
            && value == 0),
        SeqCst,
    );
    LOAD.store(2, SeqCst);
    apply_prepared();
}
pub(crate) fn successful_ready() {
    BOOT_COMPLETE.store(true, SeqCst);
    apply_prepared();
}
fn apply_prepared() {
    if !config::ENABLE_COM1_DEBUG
        || LOAD.load(SeqCst) != 2
        || !BOOT_COMPLETE.load(SeqCst)
        || APPLIED.swap(true, SeqCst)
    {
        return;
    }
    if !KEEP.load(SeqCst) {
        let _ = super::serial::close_after_boot();
    }
}
