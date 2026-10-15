// Exact boot owners, with the same module relationships as the product crate.
pub use crate::boot_info as info;
#[path = "../../../Rust/boot/log.rs"]
pub mod log;
#[path = "../../../Rust/boot/owner.rs"]
pub mod owner;

pub use crate::crash;
// Host fixtures expose the same output ABI; they never link early machine boot.
pub mod foundation {
    unsafe extern "C" {
        pub safe fn r4os_rust_boot_redirect_active() -> bool;
        fn r4os_zig_kernel_write(text: *const u8, len: usize);
        fn r4os_zig_kernel_dec(value: u64);
    }
    pub fn puts(text: &[u8]) {
        unsafe { r4os_zig_kernel_write(text.as_ptr(), text.len()) };
    }
    pub fn dec(value: u64) {
        unsafe { r4os_zig_kernel_dec(value) };
    }
}
