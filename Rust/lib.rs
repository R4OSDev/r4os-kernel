#![cfg_attr(not(test), no_std)]
extern crate alloc;

// The Contract owns every public and technical ABI definition. This kernel
// import contains no copy of its types, slots, constants or defaults.
pub use r4os_contract as contract;

pub mod arch;
pub mod boot;
pub mod build_bridge;
pub mod memory;
