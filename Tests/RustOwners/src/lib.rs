#![no_std]
// The existing Zig fixtures call the exact migrated owner implementations.
// This host-only archive has no boot entry, MMIO execution or kernel runtime.
#[path = "../../../Rust/boot/info.rs"]
pub mod boot_info;
#[path = "../../../Rust/boot/crash.rs"]
pub mod crash;
#[path = "../../../Rust/boot/limine.rs"]
pub mod limine;
#[path = "../../../Rust/boot/r4b.rs"]
pub mod r4b;
pub mod boot {
    pub use crate::boot_info as info;
}
#[path = "../../../Rust/memory/blocks.rs"]
pub mod blocks;
#[path = "../../../Rust/memory/heap.rs"]
pub mod heap;
#[path = "../../../Rust/memory/heap_policy.rs"]
pub mod heap_policy;
#[path = "../../../Rust/memory/layout.rs"]
pub mod layout;
#[path = "../../../Rust/memory/map.rs"]
pub mod memory_map;
#[path = "../../../Rust/memory/owner.rs"]
pub mod owner;
pub use crate::memory_map as map;
#[path = "../../../Rust/memory/owner_policy.rs"]
pub mod owner_policy;
#[path = "../../../Rust/memory/page_batch.rs"]
pub mod page_batch;
#[path = "../../../Rust/memory/page_tables.rs"]
pub mod page_tables;
#[path = "../../../Rust/memory/paging.rs"]
pub mod paging;
#[path = "../../../Rust/memory/phys.rs"]
pub mod phys;
#[path = "../../../Rust/memory/root.rs"]
pub mod root;
#[path = "../../../Rust/memory/virt_policy.rs"]
pub mod virt_policy;
#[path = "../../../Rust/memory/vm_index.rs"]
pub mod vm_index;
#[path = "../../../Rust/memory/vm_query.rs"]
pub mod vm_query;
#[path = "../../../Rust/memory/vm_commit_spans.rs"]
pub mod vm_commit_spans;
#[path = "../../../Rust/memory/vm_frames.rs"]
pub mod vm_frames;
#[path = "../../../Rust/memory/vm_ranges.rs"]
pub mod vm_ranges;
#[path = "../../../Rust/memory/vm_transactions.rs"]
pub mod vm_transactions;
pub mod build_bridge {
    pub mod config {
        include!(concat!(env!("OUT_DIR"), "/owner_config.rs"));
    }
}
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
