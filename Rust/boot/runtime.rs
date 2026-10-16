//! Original singleton BSP runtime/worker/launcher startup. State publication
//! and orchestration are native; no state borrow spans callbacks or waits.
use crate::{
    arch::{runtime, smp, smp_policy::Candidate},
    boot::{fatal, foundation},
    build_bridge::config,
    driver::work,
    fs::page_cache,
    loader::{boot, r4p},
    memory::{boot as memory_boot, reclaim},
    platform::{acpi, boot as platform_boot},
    program::shell_launcher,
    sched::{deadman, scheduler, task::task_api},
    service::ipc,
    storage::block_workers,
};
use core::{
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
static INITIALIZED: AtomicBool = AtomicBool::new(false);
unsafe extern "C" {
    // Typed presentation/instrumentation and still-assigned audio peers own
    // their state; these cuts have no second runtime initialization flag.
    fn r4os_zig_runtime_status(data: *const u8, len: usize);
    fn r4os_zig_runtime_stack_reclaimer() -> usize;
    fn r4os_zig_runtime_audio(progress_worker: bool) -> bool;
    fn r4os_zig_kernel_hex(value: u64, width: u8);
}
fn status(text: &[u8]) {
    unsafe { r4os_zig_runtime_status(text.as_ptr(), text.len()) };
}
unsafe fn start_smp() -> bool {
    unsafe {
        let mut info = MaybeUninit::<acpi::Info>::uninit();
        if !platform_boot::r4os_rust_platform_boot_acpi(info.as_mut_ptr()) {
            return false;
        }
        let info = info.assume_init();
        let mut candidates = [Candidate {
            apic_id: 0,
            enabled: false,
            online_capable: false,
        }; 32];
        let count = (info.madt_lapic_count as usize).min(candidates.len());
        for (out, input) in candidates
            .iter_mut()
            .zip(info.madt_cpu_entries.iter())
            .take(count)
        {
            *out = Candidate {
                apic_id: input.apic_id,
                enabled: input.flags & 1 != 0,
                online_capable: input.flags & 2 != 0,
            };
        }
        status(b"SMP starten");
        if !smp::r4os_rust_smp_start(candidates.as_ptr(), count) {
            return false;
        }
        smp::r4os_rust_smp_control(2, 0);
        true
    }
}
/// # Safety
/// Original singleton BSP startup. Domain owners retain their idempotence,
/// failure cleanup, task/FPU and admitted worker lifetime contracts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_runtime_boot_init() -> bool {
    unsafe {
        if INITIALIZED.load(Ordering::Acquire) {
            return true;
        }
        status(b"Task-System starten");
        if task_api::r4os_rust_task_control(0, 0, 0) == 0 {
            return false;
        }
        reclaim::r4os_rust_reclaim_register_stack(r4os_zig_runtime_stack_reclaimer());
        foundation::puts(b"  Task system ");
        foundation::puts(b"[OK]\r\n");
        status(b"Scheduler starten");
        if scheduler::r4os_rust_scheduler_control(0, 0, 0) == 0 {
            return false;
        }
        foundation::puts(b"  Scheduler ");
        foundation::puts(b"[OK]\r\n");
        if !start_smp() {
            return false;
        }
        foundation::puts(b"  SMP foundation ");
        foundation::puts(b"[OK]\r\n");
        status(b"Service-IPC starten");
        if ipc::r4os_rust_ipc_control(1, 0) == 0 {
            return false;
        }
        foundation::puts(b"  Service IPC worker ");
        foundation::puts(b"[OK]\r\n");
        status(b"Treiberarbeit starten");
        if !work::r4os_rust_driver_work_init() {
            return false;
        }
        foundation::puts(b"  Driver workqueue ");
        foundation::puts(b"[OK]\r\n");
        status(b"Block-Worker starten");
        if !block_workers::r4os_rust_block_workers_start() {
            return false;
        }
        foundation::puts(b"  Block worker ");
        foundation::puts(b"[OK]\r\n");
        status(b"Seitencache starten");
        if !page_cache::r4os_rust_pagecache_start() {
            return false;
        }
        foundation::puts(b"  Page-cache policy worker ");
        foundation::puts(b"[OK]\r\n");
        if !r4os_zig_runtime_audio(true) {
            return false;
        }
        foundation::puts(b"  Audio progress worker [OK]\r\n");
        INITIALIZED.store(true, Ordering::Release);
        // Same IF handoff: timed module/driver work below may park kernel-main.
        runtime::r4os_rust_runtime_control(1, 0);
        true
    }
}
fn page_fault_test() -> ! {
    let address = 0xffff_ff00_0000_0000u64;
    foundation::puts(b"  Triggering test page fault at 0x");
    unsafe { r4os_zig_kernel_hex(address, 16) };
    foundation::puts(b"...\r\n");
    unsafe { ptr::with_exposed_provenance_mut::<u8>(address as usize).write_volatile(0xcc) };
    runtime::halt_forever()
}
/// # Safety
/// Terminal original BSP boot/launcher entry. The config owner keeps its
/// admitted immutable object resident through the existing shell handoff.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_runtime_boot_start() -> ! {
    unsafe {
        status(b"Protokolle laden");
        let start = crate::loader::performance::begin_r4p_runtime();
        r4p::r4os_rust_r4p_load_all();
        crate::loader::performance::finish_r4p_runtime(start);
        r4os_zig_runtime_audio(false);
        r4p::r4os_rust_r4p_dump();
        foundation::puts(b"  Loader [OK]\r\n");
        if !r4os_rust_runtime_boot_init() {
            fatal::kernel(13, b"Task runtime init failed");
        }
        if config::ENABLE_PAGE_FAULT_TEST {
            page_fault_test();
        }
        status(b"Watchdog starten");
        if deadman::r4os_rust_deadman_start() {
            foundation::puts(b"  Deadman watchdog [OK]\r\n");
        } else {
            foundation::puts(b"  Deadman watchdog [FAILED]\r\n");
            foundation::puts(b"[DEADMAN] kernel thread creation failed\r\n");
        }
        foundation::puts(b"\r\n  Launcher [START]\r\n");
        status(b"Launcher starten");
        let config = boot::r4os_rust_loader_boot_config();
        if config.is_null() {
            fatal::kernel(13, b"Loader boot state missing");
        }
        fatal::r4os_rust_fatal_phase(18);
        let mut memory = MaybeUninit::<memory_boot::Snapshot>::uninit();
        memory_boot::r4os_rust_memory_snapshot(memory.as_mut_ptr());
        shell_launcher::r4os_rust_program_shell_start(config, memory.assume_init().usable)
    }
}
