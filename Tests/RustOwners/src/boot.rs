// Exact boot owners, with the same module relationships as the product crate.
pub use crate::boot_info as info;
#[path = "../../../Rust/boot/log.rs"]
pub mod log;
#[path = "../../../Rust/boot/owner.rs"]
pub mod owner;
