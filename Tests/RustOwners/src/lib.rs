#![no_std]
pub use r4os_contract as contract;
#[path = "../../../Rust/fs/mod.rs"]
pub mod fs;
#[path = "../../../Rust/loader/mod.rs"]
pub mod loader;
#[path = "../../../Rust/program/mod.rs"]
pub mod program;
#[path = "../../../Rust/service/mod.rs"]
pub mod service;
#[path = "../../../Rust/usb/mod.rs"]
pub mod usb;
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
    pub use crate::{sync_locks, sync_objects, sync_types, sync_wait};
}
#[path = "../../../Rust/memory/backing_types.rs"]
pub mod backing_types;
#[path = "../../../Rust/memory/backing_store.rs"]
pub mod backing_store;
#[path = "../../../Rust/memory/reclaim.rs"]
pub mod reclaim;
#[path = "../../../Rust/memory/vm_pager_types.rs"]
pub mod vm_pager_types;
#[path = "../../../Rust/memory/vm_page_state.rs"]
pub mod vm_page_state;
#[path = "../../../Rust/memory/vm_pager.rs"]
pub mod vm_pager;
#[path = "../../../Rust/memory/vm_pager_api.rs"]
pub mod vm_pager_api;
#[path = "../../../Rust/memory/vm_pager_diagnostics.rs"]
pub mod vm_pager_diagnostics;
pub mod memory {
    pub use crate::{
        backing_types, backing_store, reclaim, vm_pager_types, vm_page_state, vm_pager,
        vm_pager_api, vm_pager_diagnostics,
    };
    pub use crate::{
        blocks, heap, layout, owner, paging, phys, virt_policy, vm_query, vm_ranges,
        vm_transactions,
    };
}
pub mod arch {
    pub use crate::callback_abort;
    // Scalar native SMP admission uses the same exported symbol as the product.
    // Host owner cases never start CPUs; an unexpected call fails via the trap.
    pub mod smp {
        unsafe extern "C" {
            pub safe fn r4os_rust_smp_control(operation: u8, index: u32) -> u64;
        }
    }
    pub use crate::fpu;
    pub use crate::{hpet, io, ioapic, lapic, msr, percpu, pic, pit, rtc, runtime, smp_policy, tlb};
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

#[path = "../../../Rust/driver/utilities.rs"]
pub mod driver_utilities;

#[path = "../../../Rust/driver/work_queue.rs"]
pub mod driver_work_queue;

#[path = "../../../Rust/arch/callback_abort.rs"]
pub mod callback_abort;
#[path = "../../../Rust/driver/dma_segments.rs"]
pub mod dma_segments;
#[path = "../../../Rust/driver/registry.rs"]
pub mod driver_registry;
#[path = "../../../Rust/driver/resources.rs"]
pub mod driver_resources;
#[path = "../../../Rust/driver/irq_router.rs"]
pub mod irq_router;
#[path = "../../../Rust/driver/protocol_api.rs"]
pub mod protocol_api;
#[path = "../../../Rust/driver/protocol_registry.rs"]
pub mod protocol_registry;
#[path = "../../../Rust/driver/resource_state.rs"]
pub mod resource_state;
pub mod driver {
    pub use crate::protocol_api;
    pub use crate::dma_segments;
    pub use crate::driver_api_admission as api_admission;
    pub use crate::driver_api_audio as api_audio;
    pub use crate::driver_api_cleanup as api_cleanup;
    pub use crate::driver_api_dma as api_dma;
    pub use crate::driver_api_entry as api_entry;
    pub use crate::driver_api_gfx as api_gfx;
    pub use crate::driver_api_msi as api_msi;
    pub use crate::driver_api_net as api_net;
    pub use crate::driver_api_options as api_options;
    pub use crate::driver_api_platform as api_platform;
    pub use crate::driver_api_resource as api_resource;
    pub use crate::driver_api_runtime as api_runtime;
    pub use crate::driver_api_storage as api_storage;
    pub use crate::driver_api_validation as api_validation;
    pub use crate::driver_work as work;
    pub use crate::{driver_heap as heap, irq_router};
    pub use crate::{driver_memory_owner as memory_owner, driver_treap as treap};
    pub use crate::{
        driver_registry as registry, driver_resources as resources, driver_utilities as utilities,
        driver_work_queue as work_queue, protocol_registry, resource_state,
    };
    pub use crate::{driver_semaphore_owner as semaphore_owner, driver_thread_owner as thread_owner};
    pub use crate::{driver_semaphores as semaphores, driver_threads as threads};
}

#[path = "../../../Rust/driver/heap.rs"]
pub mod driver_heap;
#[path = "../../../Rust/driver/memory_owner.rs"]
pub mod driver_memory_owner;
#[path = "../../../Rust/driver/treap.rs"]
pub mod driver_treap;
#[path = "../../../Rust/driver/mmio_windows.rs"]
pub mod mmio_windows;

#[path = "../../../Rust/driver/semaphore_owner.rs"]
pub mod driver_semaphore_owner;
#[path = "../../../Rust/driver/semaphores.rs"]
pub mod driver_semaphores;
#[path = "../../../Rust/driver/thread_owner.rs"]
pub mod driver_thread_owner;

#[path = "../../../Rust/driver/threads.rs"]
pub mod driver_threads;

#[path = "../../../Rust/driver/work.rs"]
pub mod driver_work;

#[path = "../../../Rust/driver/api_admission.rs"]
pub mod driver_api_admission;

#[path = "../../../Rust/driver/api_dma.rs"]
pub mod driver_api_dma;

#[path = "../../../Rust/driver/api_platform.rs"]
pub mod driver_api_platform;

#[path = "../../../Rust/driver/api_msi.rs"]
pub mod driver_api_msi;

#[path = "../../../Rust/driver/api_runtime.rs"]
pub mod driver_api_runtime;

#[path = "../../../Rust/driver/api_resource.rs"]
pub mod driver_api_resource;

#[path = "../../../Rust/driver/api_options.rs"]
pub mod driver_api_options;

#[path = "../../../Rust/driver/api_audio.rs"]
pub mod driver_api_audio;

#[path = "../../../Rust/driver/api_validation.rs"]
pub mod driver_api_validation;

#[path = "../../../Rust/driver/api_storage.rs"]
pub mod driver_api_storage;

#[path = "../../../Rust/driver/api_net.rs"]
pub mod driver_api_net;

#[path = "../../../Rust/driver/api_cleanup.rs"]
pub mod driver_api_cleanup;
#[path = "../../../Rust/driver/api_entry.rs"]
pub mod driver_api_entry;

#[path = "../../../Rust/driver/api_gfx.rs"]
pub mod driver_api_gfx;

#[path = "../../../Rust/storage/dispatch.rs"]
pub mod storage_dispatch;
#[path = "../../../Rust/storage/guid.rs"]
pub mod storage_guid;
#[path = "../../../Rust/storage/split.rs"]
pub mod storage_split;
#[path = "../../../Rust/storage/access_state.rs"]
pub mod storage_access_state;
#[path = "../../../Rust/storage/access_runtime.rs"]
pub mod storage_access_runtime;
#[path = "../../../Rust/storage/gpt.rs"]
pub mod storage_gpt;
#[path = "../../../Rust/storage/partition_table.rs"]
pub mod storage_partition_table;
#[path = "../../../Rust/storage/installation.rs"]
pub mod storage_installation;
#[path = "../../../Rust/storage/boot_source.rs"]
pub mod storage_boot_source;
#[path = "../../../Rust/storage/block_registry.rs"]
pub mod storage_block_registry;
#[path = "../../../Rust/storage/block_requests.rs"]
pub mod storage_block_requests;
#[path = "../../../Rust/storage/block_state.rs"]
pub mod storage_block_state;
#[path = "../../../Rust/storage/block_accounting.rs"]
pub mod storage_block_accounting;
#[path = "../../../Rust/storage/block_workers.rs"]
pub mod storage_block_workers;
#[path = "../../../Rust/storage/block_device_state.rs"]
pub mod storage_block_device_state;
#[path = "../../../Rust/storage/block_io.rs"]
pub mod storage_block_io;
#[path = "../../../Rust/storage/block_execution.rs"]
pub mod storage_block_execution;
#[path = "../../../Rust/storage/block_diagnostics.rs"]
pub mod storage_block_diagnostics;
#[path = "../../../Rust/storage/media_inventory.rs"]
pub mod storage_media_inventory;
#[path = "../../../Rust/storage/normal_boot.rs"]
pub mod storage_normal_boot;
#[path = "../../../Rust/storage/mbr.rs"]
pub mod storage_mbr;
#[path = "../../../Rust/storage/ahci.rs"]
pub mod storage_ahci;
#[path = "../../../Rust/storage/boot.rs"]
pub mod storage_boot_owner;
#[path = "../../../Rust/storage/operations.rs"]
pub mod storage_operations;
pub mod storage {
    pub use crate::storage_operations as operations;
    pub use crate::storage_boot_owner as boot;
    pub use crate::storage_ahci as ahci;
    pub use crate::storage_mbr as mbr;
    pub use crate::storage_media_inventory as media_inventory;
    pub use crate::storage_normal_boot as normal_boot;
    pub use crate::storage_block_execution as block_execution;
    pub use crate::storage_block_diagnostics as block_diagnostics;
    pub use crate::storage_block_io as block_io;
    pub use crate::storage_block_device_state as block_device_state;
    pub use crate::storage_block_workers as block_workers;
    pub use crate::storage_block_accounting as block_accounting;
    pub use crate::storage_block_state as block_state;
    pub use crate::storage_block_requests as block_requests;
    pub use crate::storage_block_registry as block_registry;
    pub use crate::storage_boot_source as boot_source;
    pub use crate::storage_installation as installation;
    pub use crate::{storage_gpt as gpt, storage_partition_table as partition_table};
    pub use crate::{storage_access_state as access_state, storage_access_runtime as access_runtime};
    pub use crate::{storage_dispatch as dispatch, storage_guid as guid, storage_split as split};
}

#[path = "../../../Rust/net/mod.rs"]
pub mod net;

#[path = "../../../Rust/input/mod.rs"]
pub mod input;

#[path="../../../Rust/font/mod.rs"]
pub mod font;

#[path="../../../Rust/display/mod.rs"]
pub mod display;
