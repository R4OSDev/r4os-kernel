// Compatibility constants; the resident TSS and IST stacks live in Rust.
pub const DOUBLE_FAULT_IST: u8 = 1;
pub const FAULT_IST: u8 = 2;
