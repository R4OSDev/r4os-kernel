#![no_std]
pub use r4os_contract as contract;
// The existing Zig fixtures call the exact migrated owner implementations.
// This host-only archive has no boot entry, MMIO execution or kernel runtime.
#[path = "../../../Rust/memory/blocks.rs"]
pub mod blocks;
pub mod boot;
#[path = "../../../Rust/boot/info.rs"]
pub mod boot_info;
#[path = "../../../Rust/boot/crash.rs"]
pub mod crash;
#[path = "../../../Rust/memory/heap.rs"]
pub mod heap;
#[path = "../../../Rust/memory/heap_policy.rs"]
pub mod heap_policy;
#[path = "../../../Rust/memory/layout.rs"]
pub mod layout;
#[path = "../../../Rust/boot/limine.rs"]
pub mod limine;
#[path = "../../../Rust/memory/map.rs"]
pub mod memory_map;
#[path = "../../../Rust/memory/owner.rs"]
pub mod owner;
#[path = "../../../Rust/boot/r4b.rs"]
pub mod r4b;
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
#[path = "../../../Rust/arch/fpu.rs"]
pub mod fpu;
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
#[path = "../../../Rust/sched/scheduler.rs"]
pub mod scheduler;
#[path = "../../../Rust/sched/scheduler_types.rs"]
pub mod scheduler_types;
#[path = "../../../Rust/sched/task/mod.rs"]
pub mod task;
#[path = "../../../Rust/sched/task_context.rs"]
pub mod task_context;
#[path = "../../../Rust/sched/task_index.rs"]
pub mod task_index;
#[path = "../../../Rust/sched/wait_node.rs"]
pub mod wait_node;
pub mod sched {
    pub use crate::{scheduler, scheduler_types, task, task_context, task_index, wait_node};
}
pub mod memory {
    pub use crate::{heap, owner, paging, phys};
}
pub mod arch {
    pub use crate::fpu;
    pub use crate::{
        hpet, io, ioapic, lapic, msr, percpu, pic, pit, rtc, runtime, smp_policy, tlb,
    };
}
pub mod platform {
    pub use crate::acpi;
    pub use crate::clock_math;
    pub use crate::clocksource;
    pub use crate::cpu;
    pub use crate::device_inventory;
    pub use crate::firmware_tables;
    pub use crate::firmware_view;
    pub use crate::irq_boot;
    pub use crate::irq_view as irq;
    pub use crate::monotonic;
    pub use crate::pci;
    pub use crate::pci_inventory;
    pub use crate::pci_scan;
    pub use crate::pcie;
    pub use crate::platform_boot as boot;
    pub use crate::time;
    pub use crate::timer;
    pub use crate::timer_boot;
}
#[path = "../../../Rust/platform/acpi.rs"]
pub mod acpi;
#[path = "../../../Rust/platform/clocksource.rs"]
pub mod clocksource;
#[path = "../../../Rust/platform/cpu.rs"]
pub mod cpu;
#[path = "../../../Rust/platform/device_inventory.rs"]
pub mod device_inventory;
#[path = "../../../Rust/platform/firmware_tables.rs"]
pub mod firmware_tables;
#[path = "../../../Rust/platform/firmware_view.rs"]
pub mod firmware_view;
#[path = "../../../Rust/arch/hpet.rs"]
pub mod hpet;
#[path = "../../../Rust/arch/interrupt_frame.rs"]
pub mod interrupt_frame;
#[path = "../../../Rust/arch/ioapic.rs"]
pub mod ioapic;
#[path = "../../../Rust/arch/irq.rs"]
pub mod irq;
#[path = "../../../Rust/platform/irq_boot.rs"]
pub mod irq_boot;
#[path = "../../../Rust/platform/irq.rs"]
pub mod irq_view;
#[path = "../../../Rust/arch/lapic.rs"]
pub mod lapic;
#[path = "../../../Rust/platform/monotonic.rs"]
pub mod monotonic;
#[path = "../../../Rust/platform/pci.rs"]
pub mod pci;
#[path = "../../../Rust/platform/pci_inventory.rs"]
pub mod pci_inventory;
#[path = "../../../Rust/platform/pci_scan.rs"]
pub mod pci_scan;
#[path = "../../../Rust/platform/pcie.rs"]
pub mod pcie;
#[path = "../../../Rust/arch/percpu.rs"]
pub mod percpu;
#[path = "../../../Rust/platform/boot.rs"]
pub mod platform_boot;
#[path = "../../../Rust/arch/runtime.rs"]
pub mod runtime;
#[path = "../../../Rust/arch/smp_policy.rs"]
pub mod smp_policy;
#[path = "../../../Rust/platform/time.rs"]
pub mod time;
#[path = "../../../Rust/platform/timer.rs"]
pub mod timer;
#[path = "../../../Rust/platform/timer_boot.rs"]
pub mod timer_boot;
#[path = "../../../Rust/arch/tlb.rs"]
pub mod tlb;
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

// Weak archive links retain genuine test adapters and fail on unexpected entry.
core::arch::global_asm!(include_str!("hardware_traps.S"));

#[path = "../../../Rust/sched/sync_types.rs"]
pub mod sync_types;

#[path = "../../../Rust/sched/sync_wait.rs"]
pub mod sync_wait;

#[path = "../../../Rust/sched/sync_locks.rs"]
pub mod sync_locks;

#[path = "../../../Rust/sched/sync_objects.rs"]
pub mod sync_objects;

#[path = "../../../Rust/sched/sync_stats.rs"]
pub mod sync_stats;

#[path = "../../../Rust/sched/sync_selftest.rs"]
pub mod sync_selftest;

#[path = "../../../Rust/kernel/deadman.rs"]
pub mod deadman;
