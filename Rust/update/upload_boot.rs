//! Exact upload-claim replay summary published by the linear boot owner.
use super::{claim::Summary, claim_store};
use crate::boot::foundation;
use core::{cell::UnsafeCell, ptr};
struct Storage(UnsafeCell<Summary>);
// SAFETY: linear boot publishes before runtime; later readers only copy.
unsafe impl Sync for Storage {}
static SUMMARY: Storage = Storage(UnsafeCell::new(Summary {
    published: 0,
    rolled_back: 0,
    retired: 0,
    foreign: 0,
    invalid: 0,
    failed: 0,
}));
pub fn recover_before_runtime() -> bool {
    if !unsafe { claim_store::recover(SUMMARY.0.get()) } {
        foundation::puts(b"[UPLOADCLAIM] claim store unreadable\n");
        return false;
    }
    let summary = unsafe { SUMMARY.0.get().read() };
    if summary.total() == 0 {
        return true;
    }
    foundation::puts(b"[UPLOADCLAIM] replay published=");
    foundation::dec(summary.published as u64);
    for (text, value) in [
        (&b" rolled-back="[..], summary.rolled_back),
        (b" retired=", summary.retired),
        (b" foreign=", summary.foreign),
        (b" invalid=", summary.invalid),
        (b" failed=", summary.failed),
    ] {
        foundation::puts(text);
        foundation::dec(value as u64)
    }
    foundation::puts(b"\n");
    if summary.failed != 0 {
        foundation::puts(b"Upload claim replay incomplete; retried on next boot")
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_upload_boot_recover() -> bool {
    recover_before_runtime()
}
/// # Safety
/// Output is the exact live C summary; runtime only reads the boot snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_upload_boot_summary(out: *mut Summary) {
    unsafe { ptr::copy_nonoverlapping(SUMMARY.0.get(), out, 1) }
}
