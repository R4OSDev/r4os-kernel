//! The existing x86_64 memory windows, without a new address-space policy.
pub const PAGE_SIZE: u64 = 4096;
pub const APP_STACK_BASE: u64 = 0xffff_ff40_0000_0000;
pub const APP_STACK_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const MIB: u64 = 1024 * 1024;
pub const KERNEL_DYNAMIC_BASE: u64 = 0xffff_c000_0000_0000;
pub const KERNEL_IMAGE_BASE: u64 = 0xffff_ffff_8000_0000;
pub const KERNEL_IMAGE_WINDOW_BYTES: u64 = 512 * MIB;
pub const MMIO_BASE: u64 = 0xffff_e000_0000_0000;
pub const MMIO_END: u64 = 0xffff_f000_0000_0000;
pub const FRAMEBUFFER_BASE: u64 = 0xffff_f000_0000_0000;
