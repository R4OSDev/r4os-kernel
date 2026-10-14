#![no_std]
pub use r4os_contract as contract;
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
pub mod boot;
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
#[path = "../../../Rust/memory/vm_commit_spans.rs"]
pub mod vm_commit_spans;
#[path = "../../../Rust/memory/vm_frames.rs"]
pub mod vm_frames;
#[path = "../../../Rust/memory/vm_index.rs"]
pub mod vm_index;
#[path = "../../../Rust/memory/vm_query.rs"]
pub mod vm_query;
#[path = "../../../Rust/memory/vm_ranges.rs"]
pub mod vm_ranges;
#[path = "../../../Rust/memory/vm_transactions.rs"]
pub mod vm_transactions;
pub mod build_bridge {
    pub mod config {
        include!(concat!(env!("OUT_DIR"), "/owner_config.rs"));
    }
}
#[path = "../../../Rust/platform/clock_math.rs"]
pub mod clock_math;
#[path = "../../../Rust/arch/io.rs"]
pub mod io;
#[path = "../../../Rust/arch/msr.rs"]
pub mod msr;
#[path = "../../../Rust/arch/pic.rs"]
pub mod pic;
#[path = "../../../Rust/arch/pit.rs"]
pub mod pit;
#[path = "../../../Rust/arch/rtc.rs"]
pub mod rtc;
pub mod memory {
    pub use crate::{owner, paging, phys};
}
pub mod arch {
    pub use crate::{hpet, io, ioapic, lapic, msr, pic, pit, rtc, runtime};
}
pub mod platform {
    pub use crate::timer_boot;
    pub use crate::device_inventory;
    pub use crate::irq_view as irq;
    pub use crate::irq_boot;
    pub use crate::platform_boot as boot;
    pub use crate::acpi;
    pub use crate::clock_math;
    pub use crate::clocksource;
    pub use crate::cpu;
    pub use crate::firmware_tables;
    pub use crate::firmware_view;
    pub use crate::monotonic;
    pub use crate::pci_scan;
    pub use crate::pci;
    pub use crate::pci_inventory;
    pub use crate::pcie;
    pub use crate::time;
    pub use crate::timer;
}
#[path = "../../../Rust/platform/acpi.rs"]
pub mod acpi;
#[path = "../../../Rust/platform/firmware_tables.rs"]
pub mod firmware_tables;
#[path = "../../../Rust/platform/firmware_view.rs"]
pub mod firmware_view;
#[path = "../../../Rust/platform/pci_scan.rs"]
pub mod pci_scan;
#[path = "../../../Rust/platform/pci.rs"]
pub mod pci;
#[path = "../../../Rust/platform/pci_inventory.rs"]
pub mod pci_inventory;
#[path = "../../../Rust/platform/boot.rs"]
pub mod platform_boot;
#[path = "../../../Rust/platform/irq.rs"]
pub mod irq_view;
#[path = "../../../Rust/platform/irq_boot.rs"]
pub mod irq_boot;
#[path = "../../../Rust/platform/pcie.rs"]
pub mod pcie;
#[path = "../../../Rust/platform/clocksource.rs"]
pub mod clocksource;
#[path = "../../../Rust/platform/cpu.rs"]
pub mod cpu;
#[path = "../../../Rust/platform/device_inventory.rs"]
pub mod device_inventory;
#[path = "../../../Rust/arch/hpet.rs"]
pub mod hpet;
#[path = "../../../Rust/arch/interrupt_frame.rs"]
pub mod interrupt_frame;
#[path = "../../../Rust/arch/ioapic.rs"]
pub mod ioapic;
#[path = "../../../Rust/arch/irq.rs"]
pub mod irq;
#[path = "../../../Rust/arch/lapic.rs"]
pub mod lapic;
#[path = "../../../Rust/platform/monotonic.rs"]
pub mod monotonic;
#[path = "../../../Rust/arch/runtime.rs"]
pub mod runtime;
#[path = "../../../Rust/platform/time.rs"]
pub mod time;
#[path = "../../../Rust/platform/timer.rs"]
pub mod timer;
#[path = "../../../Rust/platform/timer_boot.rs"]
pub mod timer_boot;
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
