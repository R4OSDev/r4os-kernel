#![no_std]
// The existing Zig fixtures call the exact migrated owner implementations.
// This host-only archive has no boot entry, MMIO execution or kernel runtime.
#[path = "../../../Rust/boot/crash.rs"]
pub mod crash;
#[path = "../../../Rust/boot/limine.rs"]
pub mod limine;
#[path = "../../../Rust/boot/r4b.rs"]
pub mod r4b;
unsafe extern "C" {
    fn r4os_kernel_owner_test_panic() -> !;
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    unsafe { r4os_kernel_owner_test_panic() }
}

// Native-host core libraries can retain an EH symbol even in an abort build.
// No unwind is allowed through the fixture's C calls; unexpected entry fails
// the owner test instead of pretending to implement an unwinder.
#[cfg(not(target_os = "none"))]
#[unsafe(no_mangle)]
pub extern "C" fn rust_eh_personality() -> ! {
    unsafe { r4os_kernel_owner_test_panic() }
}
