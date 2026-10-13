//! Singleton BSP memory boot order and published summary. Domain owners are
//! called without retaining a mutable orchestration-state borrow.
use super::{blocks, heap, map, paging, phys, root};
use crate::{
    boot::{fatal, foundation, info},
    build_bridge::config,
};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
const CORE: u32 = 1;
const SUMMARY: u32 = 2;
const DEVICES: u32 = 4;
const SWITCHED: u32 = 8;
static FLAGS: AtomicU32 = AtomicU32::new(0);
static USABLE: AtomicU64 = AtomicU64::new(0);
static LARGEST_BASE: AtomicU64 = AtomicU64::new(0);
static LARGEST_LEN: AtomicU64 = AtomicU64::new(0);
#[repr(C)]
pub struct Snapshot {
    pub flags: u32,
    pub usable: u64,
    pub largest_base: u64,
    pub largest_len: u64,
}
unsafe extern "C" {
    // Fixed cuts to the still active display/VM/SMP/diagnostic owners.
    fn r4os_zig_memory_step(operation: u8, argument: u64) -> bool;
}
fn step(operation: u8, argument: u64) -> bool {
    unsafe { r4os_zig_memory_step(operation, argument) }
}
fn phase(name: &[u8], result: &[u8]) {
    foundation::puts(b"MemoryBoot: ");
    foundation::puts(name);
    foundation::puts(result);
    foundation::puts(b"\r\n");
}
fn begin(name: &[u8]) {
    phase(name, b" [START]");
}
fn end(name: &[u8]) {
    phase(name, b" [OK]");
}
fn skip(name: &[u8]) {
    phase(name, b" [SKIP]");
}
fn fail(message: &[u8]) -> bool {
    unsafe {
        fatal::r4os_rust_fatal_record(6, message.as_ptr(), message.len());
    }
    false
}
/// # Safety
/// Singleton BSP startup precedes AP/task publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_memory_core() -> bool {
    if FLAGS.load(Ordering::Acquire) & CORE != 0 {
        return true;
    }
    begin(b"Memory Map");
    step(0, 0);
    let boot = unsafe { &*info::r4os_rust_boot_info() };
    let mut summary = map::Summary::default();
    unsafe {
        map::r4os_rust_memory_summary(boot.memory, boot.memory_count, &mut summary);
    }
    USABLE.store(summary.usable_bytes, Ordering::Relaxed);
    LARGEST_BASE.store(summary.largest_usable_base, Ordering::Relaxed);
    LARGEST_LEN.store(summary.largest_usable_len, Ordering::Relaxed);
    FLAGS.fetch_or(SUMMARY, Ordering::Release);
    end(b"Memory Map");
    begin(b"MemoryBlock table");
    if !blocks::r4os_rust_blocks_init() {
        return fail(b"MemoryBlock init failed");
    }
    step(1, 0);
    end(b"MemoryBlock table");
    begin(b"Physical allocator");
    if !phys::r4os_rust_phys_init() {
        return fail(b"Physical allocator init failed");
    }
    step(2, 0);
    if config::ENABLE_BOOT_SELFTESTS && !blocks::r4os_rust_blocks_invariant() {
        return fail(b"MemoryBlock index invariant failed");
    }
    step(3, 0);
    end(b"Physical allocator");
    begin(b"Paging core");
    if !paging::r4os_rust_paging_init() {
        return fail(b"Paging init failed");
    }
    step(4, 0);
    end(b"Paging core");
    if config::ENABLE_BOOT_SELFTESTS {
        begin(b"Page-table builder selftest");
        if !unsafe { root::r4os_rust_root_control(0) } {
            return fail(b"Page-table builder selftest failed");
        }
        end(b"Page-table builder selftest");
    } else {
        skip(b"Page-table builder selftest");
    }
    begin(b"R4OS PML4 dry-run");
    if !unsafe { root::r4os_rust_root_control(1) } {
        step(5, 0);
        return fail(b"R4OS PML4 dry-run failed");
    }
    step(5, 0);
    end(b"R4OS PML4 dry-run");
    begin(b"Virtual ranges");
    if !step(6, 0) {
        return fail(b"Virtual range init failed");
    }
    step(7, 0);
    end(b"Virtual ranges");
    begin(b"Framebuffer PAT-WC");
    if step(8, 0) {
        end(b"Framebuffer PAT-WC");
    } else {
        skip(b"Framebuffer PAT-WC");
    }
    begin(b"Kernel heap");
    if !heap::r4os_rust_heap_init() {
        return fail(b"Kernel heap init failed");
    }
    step(9, 0);
    if !heap::r4os_rust_heap_boot_invariant() {
        return fail(b"Kernel heap invariant failed");
    }
    if config::ENABLE_BOOT_SELFTESTS {
        if !step(10, 0) {
            return fail(b"Kernel heap selftest failed");
        }
        if !blocks::r4os_rust_blocks_invariant() || !step(11, 0) {
            return fail(b"Memory metadata index invariant failed");
        }
    }
    end(b"Kernel heap");
    if step(12, summary.usable_bytes / 32) {
        end(b"Console pixel backing");
    } else {
        skip(b"Console pixel backing");
    }
    step(13, 0);
    FLAGS.fetch_or(CORE, Ordering::Release);
    true
}
/// # Safety
/// input is the resident normalized ACPI C record retained by the singleton
/// BSP until this operation returns, before AP publication.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_memory_finalize(input: *const root::AcpiInput) -> bool {
    let flags = FLAGS.load(Ordering::Acquire);
    if flags & (DEVICES | SWITCHED) == DEVICES | SWITCHED {
        return true;
    }
    if flags & CORE == 0 {
        return fail(b"Memory core not initialized before device mappings");
    }
    if !unsafe { root::r4os_rust_root_devices(input) } {
        return fail(b"R4OS device mappings dry-run failed");
    }
    FLAGS.fetch_or(DEVICES, Ordering::Release);
    foundation::puts(b"  R4OS device mappings ");
    foundation::puts(b"[OK]\r\n");
    step(14, 0);
    if !unsafe { root::r4os_rust_root_control(2) } {
        step(15, 0);
        return fail(b"R4OS CR3 switch failed");
    }
    FLAGS.fetch_or(SWITCHED, Ordering::Release);
    foundation::puts(b"  R4OS CR3 switch ");
    foundation::puts(b"[OK]\r\n");
    step(15, 0);
    true
}
/// # Safety
/// output is exclusive C storage; caller observes BSP completion or published flags.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_memory_snapshot(output: *mut Snapshot) {
    let flags = FLAGS.load(Ordering::Acquire);
    unsafe {
        output.write(Snapshot {
            flags,
            usable: USABLE.load(Ordering::Relaxed),
            largest_base: LARGEST_BASE.load(Ordering::Relaxed),
            largest_len: LARGEST_LEN.load(Ordering::Relaxed),
        });
    }
}
const _: () = assert!(core::mem::size_of::<Snapshot>() == 32);
