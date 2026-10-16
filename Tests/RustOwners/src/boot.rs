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
        pub safe fn r4os_rust_boot_release();
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

// The exact failure owner is compiled for new boot-owner references; host
// USB registry cases never execute machine boot or framebuffer output.
#[path = "../../../Rust/boot/fatal.rs"]
pub mod fatal;

// Native UART dependency; pure owner fixtures never execute its hardware calls.
#[path = "../../../Rust/boot/serial.rs"]
pub mod serial;

// Exact native visible boot renderer dependencies; pure fixtures do not boot.
pub use crate::{limine, r4b};

#[path = "../../../Rust/boot/config.rs"]
pub mod config;

#[path = "../../../Rust/boot/performance.rs"]
pub mod performance;

// Exact new observer/output owners. Serialized host models never execute
// the machine boot-release cut; its resident C ABI remains explicit.
#[path = "../../../Rust/boot/instrumentation.rs"]
pub mod instrumentation;
#[path = "../../../Rust/boot/log_event.rs"]
pub mod log_event;
#[path = "../../../Rust/boot/log_route.rs"]
pub mod log_route;
