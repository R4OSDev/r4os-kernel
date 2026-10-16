#![cfg_attr(not(test), no_std)]
extern crate alloc;

// The Contract owns every public and technical ABI definition. This kernel
// import contains no copy of its types, slots, constants or defaults.
pub use r4os_contract as contract;

pub mod arch;
pub mod boot;
pub mod build_bridge;
pub mod driver;
pub mod fs;
pub mod loader;
pub mod memory;
pub mod platform;
pub mod program;
pub mod sched;
pub mod service;
pub mod storage;
pub mod usb;

pub mod net;

pub mod input;

pub mod font;

pub mod display;

pub mod gfx;

pub mod audio;
pub mod update;
