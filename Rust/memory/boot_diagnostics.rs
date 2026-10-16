//! Existing boot memory reports. Every snapshot is copied from its single
//! native owner; reporting neither admits memory nor changes mapping policy.
use super::{blocks, heap, map, paging, phys, root};
use crate::boot::{
    info,
    log_route::{dec, hex, puts},
};
fn yes(value: bool) -> &'static [u8] {
    if value { b"yes" } else { b"no" }
}
fn cache_name(value: u8) -> &'static [u8] {
    match value {
        0 => b"normal-ram",
        1 => b"framebuffer-wc",
        2 => b"mmio-uncached",
        3 => b"acpi-firmware",
        _ => b"unknown",
    }
}
fn block_kind(value: usize) -> &'static [u8] {
    [
        b"boot".as_slice(),
        b"kernel",
        b"kernel_heap",
        b"page_table",
        b"virtual_range",
        b"program_image",
        b"app_heap",
        b"app_stack",
        b"dma",
        b"mmio",
        b"framebuffer",
        b"reserved",
        b"free",
        b"unknown",
    ][value]
}
fn probe(name: &[u8], bits: u32, bit: u32) {
    puts(name);
    puts(b"=");
    puts(if bits & bit != 0 { b"ok" } else { b"missing" });
}
fn root_stats() -> root::Stats {
    let mut s = core::mem::MaybeUninit::uninit();
    unsafe {
        root::r4os_rust_root_stats(s.as_mut_ptr());
        s.assume_init()
    }
}
fn paging_stats() -> paging::Stats {
    let mut s = core::mem::MaybeUninit::uninit();
    unsafe {
        paging::r4os_rust_paging_stats(s.as_mut_ptr());
        s.assume_init()
    }
}
fn heap_stats() -> heap::Stats {
    let mut s = core::mem::MaybeUninit::uninit();
    unsafe {
        heap::r4os_rust_heap_stats(s.as_mut_ptr());
        s.assume_init()
    }
}
fn blocks_stats() -> blocks::Summary {
    let mut s = core::mem::MaybeUninit::uninit();
    unsafe {
        blocks::r4os_rust_blocks_summary(s.as_mut_ptr());
        s.assume_init()
    }
}
fn phys_stats() -> phys::Stats {
    let mut s = core::mem::MaybeUninit::uninit();
    unsafe {
        phys::r4os_rust_phys_stats(s.as_mut_ptr());
        s.assume_init()
    }
}

pub(crate) fn blocks_summary() {
    let s = blocks_stats();
    puts(b"  MemoryBlocks: active=");
    dec(s.active_blocks);
    puts(b" released=");
    dec(s.released_blocks);
    puts(b" errors=");
    dec(s.error_blocks);
    puts(b" overflow=");
    puts(if s.overflow { b"yes" } else { b"no" });
    puts(b"\r\n");

    puts(b"  MemoryBlocks bytes: phys=");
    dec(s.physical_bytes);
    puts(b" virt=");
    dec(s.virtual_bytes);
    puts(b" reserved=");
    dec(s.reserved_bytes);
    puts(b" committed=");
    dec(s.committed_bytes);
    puts(b"\r\n");

    puts(b"  MemoryBlocks free phys=");
    dec(s.free_physical_bytes);
    puts(b" largest=0x");
    hex(s.largest_free_phys_base, 16);
    puts(b" len=0x");
    hex(s.largest_free_phys_len, 16);
    puts(b"\r\n");

    block_counts(&s);
}

pub(super) fn physical() {
    let s = phys_stats();
    puts(b"  Physical frames: total=");
    dec(s.total_frames);
    puts(b" free=");
    dec(s.free_frames);
    puts(b" used=");
    dec(s.used_frames);
    puts(b"\r\n");
    puts(b"  Physical extents: alloc=");
    dec(s.extent_allocations);
    puts(b" frames=");
    dec(s.extent_allocated_frames);
    puts(b" max=");
    dec(s.extent_max_frames);
    puts(b" free=");
    dec(s.extent_frees);
    puts(b" returned=");
    dec(s.extent_freed_frames);
    puts(b"\r\n");
    puts(b"  PMM bitmap: base=0x");
    hex(s.bitmap_base, 16);
    puts(b" bytes=");
    dec(s.bitmap_bytes);
    puts(b"\r\n");
}

pub(super) fn paging() {
    let _guard = super::owner::Guard::acquire(6);
    let s = paging_stats();
    runtime_status();
    puts(b"  Paging mutations: map=");
    dec(s.map_pages);
    puts(b" unmap=");
    dec(s.unmap_pages);
    puts(b" wc-pages=");
    dec(s.write_combining_pages);
    puts(b" invlpg=");
    dec(s.invlpg_flushes);
    puts(b" failures=");
    dec(s.map_failures + s.unmap_failures);
    puts(b"\r\n");
    puts(b"  Paging batches: map=");
    dec(s.map_batches);
    puts(b" pages=");
    dec(s.map_batch_pages);
    puts(b" max=");
    dec(s.map_batch_max_pages);
    puts(b" rollback=");
    dec(s.map_batch_rollbacks);
    puts(b" unmap=");
    dec(s.unmap_batches);
    puts(b" pages=");
    dec(s.unmap_batch_pages);
    puts(b" max=");
    dec(s.unmap_batch_max_pages);
    puts(b"\r\n");
    puts(b"  Paging shootdown failures=");
    dec(s.shootdown_failures);
    puts(b" restores=");
    dec(s.rollback_restores);
    puts(b" restore-failures=");
    dec(s.rollback_restore_failures);
    puts(b"\r\n");
    puts(b"  Paging root mismatches=");
    dec(s.root_mismatches);
    puts(b"\r\n");
    layout_plan();
}

pub(super) fn runtime_status() {
    let _guard = super::owner::Guard::acquire(6);
    let s = paging_stats();
    puts(b"  Paging root CR3: 0x");
    hex(s.active_root_phys, 16);
    puts(b" hardware=0x");
    hex(s.hardware_cr3, 16);
    puts(b" owner=");
    puts(match s.root_owner {
        0 => b"unknown",
        1 => b"bootloader",
        2 => b"r4os",
        _ => unreachable!(),
    });
    puts(b" active=");
    puts(if s.active_root_confirmed {
        b"yes"
    } else {
        b"no"
    });
    puts(b" r4os=");
    puts(if s.r4os_root_active { b"yes" } else { b"no" });
    puts(b"\r\n");
    puts(b"  Paging mutations: map=");
    dec(s.map_pages);
    puts(b" unmap=");
    dec(s.unmap_pages);
    puts(b" wc-pages=");
    dec(s.write_combining_pages);
    puts(b" invlpg=");
    dec(s.invlpg_flushes);
    puts(b" map-fail=");
    dec(s.map_failures);
    puts(b" unmap-fail=");
    dec(s.unmap_failures);
    puts(b" root-mismatches=");
    dec(s.root_mismatches);
    puts(b"\r\n");
}

pub(super) fn root() {
    let s = root_stats();
    puts(b"  Page-table builder: sanity=");
    dec(s.boot_sanity_runs);
    puts(b" failures=");
    dec(s.boot_sanity_failures);
    puts(b" last-root=0x");
    hex(s.last_sanity_root_phys, 16);
    puts(b" tables=");
    dec(s.last_sanity_table_frames);
    puts(b"\r\n");

    puts(b"  R4OS PML4 dry-run: runs=");
    dec(s.kernel_space_runs);
    puts(b" failures=");
    dec(s.kernel_space_failures);
    puts(b" root=0x");
    hex(s.kernel_space_root_phys, 16);
    puts(b" tables=");
    dec(s.kernel_space_table_frames);
    puts(b" source-cr3=0x");
    hex(s.kernel_space_source_root_phys, 16);
    puts(b" limine-subtables=");
    puts(if s.uses_limine_subtables {
        b"yes"
    } else {
        b"no"
    });
    puts(b"\r\n");

    puts(b"  R4OS PML4 maps: hhdm 2M=");
    dec(s.hhdm_pages_2m);
    puts(b" 4K=");
    dec(s.hhdm_pages_4k);
    puts(b" kernel-pages=");
    dec(s.kernel_image_pages);
    puts(b" kernel-skipped=");
    dec(s.kernel_image_skipped);
    puts(b" boot-probe-pages=");
    dec(s.boot_probe_pages);
    puts(b"\r\n");

    puts(b"  R4OS PML4 probes:");
    probe(b" code", s.probe_bits, 1);
    probe(b" rodata", s.probe_bits, 2);
    probe(b" data", s.probe_bits, 4);
    probe(b" bss", s.probe_bits, 8);
    probe(b" stack", s.probe_bits, 16);
    probe(b" hhdm", s.probe_bits, 32);
    probe(b" pmm", s.probe_bits, 64);
    probe(b" bootinfo", s.probe_bits, 128);
    probe(b" heapmeta", s.probe_bits, 256);
    puts(b"\r\n");
    if s.debug_probe_bit != 0 {
        puts(b"  R4OS PML4 probe debug: bit=0x");
        hex((s.debug_probe_bit) as u64, 8);
        puts(b" virt=0x");
        hex(s.debug_probe_virt, 16);
        puts(b" step=");
        dec(s.debug_probe_step);
        puts(b" source=0x");
        hex(s.debug_probe_source_phys, 16);
        puts(b" existing=0x");
        hex(s.debug_probe_existing_phys, 16);
        puts(b"\r\n");
    }
}

pub(super) fn devices() {
    let s = root_stats();
    puts(b"  R4OS device mappings: runs=");
    dec(s.device_space_runs);
    puts(b" failures=");
    dec(s.device_space_failures);
    puts(b" records=");
    dec(s.device_mapping_count);
    puts(b" truncated=");
    puts(if s.device_mapping_truncated {
        b"yes"
    } else {
        b"no"
    });
    puts(b"\r\n");

    puts(b"  R4OS device bytes: framebuffer=");
    dec(s.framebuffer_bytes);
    puts(b" acpi=");
    dec(s.acpi_bytes);
    puts(b" mmio=");
    dec(s.mmio_bytes);
    puts(b" pcie-bars=");
    dec(s.pcie_bar_bytes);
    puts(b" pcie-bar-count=");
    dec(s.pcie_bar_count);
    puts(b"\r\n");

    puts(b"  R4OS device probes:");
    probe(b" framebuffer", s.device_probe_bits, 1);
    probe(b" lapic", s.device_probe_bits, 2);
    probe(b" ioapic", s.device_probe_bits, 4);
    probe(b" hpet", s.device_probe_bits, 8);
    probe(b" acpi", s.device_probe_bits, 16);
    probe(b" acpi-reset", s.device_probe_bits, 128);
    probe(b" ecam", s.device_probe_bits, 32);
    probe(b" pcie-bar", s.device_probe_bits, 64);
    puts(b"\r\n");

    puts(b"  R4OS device cache-policy: framebuffer=");
    puts(cache_name(1));
    puts(b" mmio=");
    puts(cache_name(2));
    puts(b" acpi=");
    puts(cache_name(3));
    puts(b"\r\n");

    for i in 0..s.device_mapping_count as usize {
        let mut m = core::mem::MaybeUninit::uninit();
        if !unsafe { root::r4os_rust_root_mapping(i, m.as_mut_ptr()) } {
            break;
        }
        let m = unsafe { m.assume_init() };
        puts(b"    ");
        puts(unsafe { core::slice::from_raw_parts(m.name_ptr, m.name_len) });
        puts(b" phys=0x");
        hex(m.phys_base, 16);
        puts(b" virt=0x");
        hex(m.virt_base, 16);
        puts(b" len=0x");
        hex(m.len, 16);
        puts(b" cache=");
        puts(cache_name(m.cache_policy));
        puts(b" flags=0x");
        hex(m.flags, 16);
        puts(b" mapped=");
        puts(yes(m.mapped));
        puts(b"\r\n");
    }
}

pub(super) fn cr3() {
    let s = root_stats();
    puts(b"  R4OS CR3 switch: runs=");
    dec(s.cr3_switch_runs);
    puts(b" failures=");
    dec(s.cr3_switch_failures);
    puts(b" old=0x");
    hex(s.old_cr3, 16);
    puts(b" new=0x");
    hex(s.new_cr3, 16);
    puts(b" active=0x");
    hex(s.active_cr3, 16);
    puts(b" done=");
    puts(if s.cr3_switch_done { b"yes" } else { b"no" });
    puts(b"\r\n");

    puts(b"  R4OS CR3 post-checks:");
    probe(b" code", s.post_switch_bits, 1);
    probe(b" stack", s.post_switch_bits, 2);
    probe(b" hhdm", s.post_switch_bits, 4);
    probe(b" pmm", s.post_switch_bits, 8);
    probe(b" page-table", s.post_switch_bits, 16);
    probe(b" bootinfo", s.post_switch_bits, 32);
    probe(b" framebuffer", s.post_switch_bits, 64);
    probe(b" heap", s.post_switch_bits, 128);
    puts(b"\r\n");

    puts(b"  R4OS runtime maps: heap-bytes=");
    dec(s.heap_committed_bytes);
    puts(b" heap-pages=");
    dec(s.heap_committed_pages);
    puts(b" heap-skipped=");
    dec(s.heap_committed_skipped);
    puts(b"\r\n");
    if s.debug_probe_bit != 0 {
        puts(b"  R4OS CR3 debug probe: bit=0x");
        hex((s.debug_probe_bit) as u64, 8);
        puts(b" virt=0x");
        hex(s.debug_probe_virt, 16);
        puts(b" step=");
        dec(s.debug_probe_step);
        puts(b" source=0x");
        hex(s.debug_probe_source_phys, 16);
        puts(b" existing=0x");
        hex(s.debug_probe_existing_phys, 16);
        puts(b"\r\n");
    }

    limine_status(&s);
    puts(b"  R4OS TLB: CR3 write flushed active address translations; later single-page changes use INVLPG\r\n");
    runtime_status();
}

pub(super) fn limine_status(s: &root::Stats) {
    puts(b"  Limine page-tables: old=");
    dec(s.limine_old_table_frames);
    puts(b" active=");
    dec(s.limine_active_table_frames);
    puts(b" referenced=");
    dec(s.limine_referenced_frames);
    puts(b" candidates=");
    dec(s.limine_release_candidates);
    puts(b" quarantined=");
    dec(s.limine_quarantined_frames);
    puts(b" released=");
    dec(s.limine_released_frames);
    puts(b" retained=");
    dec(s.limine_retained_frames);
    puts(b" failures=");
    dec(s.limine_pt_scan_failures + s.limine_quarantine_failures);
    puts(b" truncated=");
    puts(
        if s.limine_old_scan_truncated || s.limine_active_scan_truncated {
            b"yes"
        } else {
            b"no"
        },
    );
    puts(b" old-active=");
    puts(if s.limine_old_root_active {
        b"yes"
    } else {
        b"no"
    });
    puts(b" decision=");
    puts(limine_decision(s));
    puts(b"\r\n");
}

pub(super) fn heap() {
    let s = heap_stats();
    puts(b"  Kernel heap: v3 pages=");
    dec((s.pages) as u64);
    puts(b" base=0x");
    hex(s.base, 16);
    puts(b" used=");
    dec((s.used_bytes) as u64);
    puts(b" free=");
    dec((s.free_bytes) as u64);
    puts(b" committed=");
    dec((s.committed_bytes) as u64);
    puts(b" cap=");
    dec((s.reserved_bytes) as u64);
    puts(b"\r\n");

    puts(b"  Kernel heap VM: commit_calls=");
    dec(s.commit_calls);
    puts(b" pages=");
    dec(s.committed_pages_total);
    puts(b" failures=");
    dec(s.commit_failures);
    puts(b" uncommit_calls=");
    dec(s.uncommit_calls);
    puts(b" pages=");
    dec(s.uncommitted_pages_total);
    puts(b" failures=");
    dec(s.uncommit_failures);
    puts(b" retained=");
    dec((s.retained_tail_pages) as u64);
    puts(b" suppressed=");
    dec(s.release_suppressed);
    puts(b" pressure=");
    dec(s.pressure_releases);
    puts(b" poison_bytes=");
    dec(s.poison_bytes);
    puts(b"\r\n");

    puts(b"  Kernel heap blocks: active=");
    dec((s.active_blocks) as u64);
    puts(b" free=");
    dec((s.free_blocks) as u64);
    puts(b" largest_free=");
    dec((s.largest_free) as u64);
    puts(b" frag=");
    puts(if s.fragmentation_hint { b"yes" } else { b"no" });
    puts(b"\r\n");

    puts(b"  Kernel heap errors: alloc=");
    dec(s.allocation_errors);
    puts(b" oom=");
    dec(s.oom_errors);
    puts(b" invalid_free=");
    dec(s.invalid_free_errors);
    puts(b" double_free=");
    dec(s.double_free_errors);
    puts(b" size_mismatch=");
    dec(s.size_mismatch_errors);
    puts(b" reentry=");
    dec(s.reentry_errors);
    puts(b"\r\n");
}

fn block_counts(s: &blocks::Summary) {
    for (i, count) in s.by_kind.iter().enumerate() {
        if *count != 0 {
            puts(b"    ");
            puts(block_kind(i));
            puts(b": ");
            dec(*count);
            puts(b"\r\n");
        }
    }
}
fn limine_decision(s: &root::Stats) -> &'static [u8] {
    if s.limine_pt_scan_failures != 0 {
        return b"retain-scan-failed";
    }
    if s.limine_old_scan_truncated || s.limine_active_scan_truncated {
        return b"retain-truncated";
    }
    if s.limine_old_root_active {
        return b"retain-active";
    }
    if s.limine_referenced_frames != 0 {
        return b"retain-referenced";
    }
    if !s.limine_release_safe {
        return b"retain-unproven";
    }
    if s.limine_quarantine_failures != 0 {
        return b"retain-partial";
    }
    if s.limine_quarantined_frames == s.limine_release_candidates
        && s.limine_release_candidates != 0
    {
        return b"quarantine";
    }
    b"retain-unproven"
}
pub(super) fn memory_map() {
    let info = unsafe { &*info::r4os_rust_boot_info() };
    if info.memory_count == 0 {
        puts(b"  Memory map: unavailable\r\n");
        return;
    }
    let entries = unsafe { core::slice::from_raw_parts(info.memory, info.memory_count) };
    puts(b"  Memory map entries: ");
    dec(entries.len() as u64);
    if info.flags & 2 != 0 {
        puts(b" truncated=yes");
    }
    if info.invalid_entries != 0 {
        puts(b" invalid=");
        dec(info.invalid_entries);
    }
    puts(b"\r\n");
    for (i, e) in entries.iter().enumerate() {
        puts(b"    [");
        dec(i as u64);
        puts(b"] ");
        puts(match e.kind {
            0 => b"usable",
            1 => b"reserved",
            2 => b"acpi-reclaim",
            3 => b"acpi-nvs",
            4 => b"bad",
            5 => b"bootloader",
            6 => b"kernel",
            7 => b"framebuffer",
            _ => b"unknown",
        });
        puts(b" base=0x");
        hex(e.base, 16);
        puts(b" len=0x");
        hex(e.length, 16);
        if e.kind == 0 {
            puts(b" usable-pages=0x");
            hex(e.usable_base, 16);
            puts(b"+0x");
            hex(e.usable_len, 16);
        }
        puts(b"\r\n");
    }
    let s = map::summarize(entries);
    puts(b"  Usable page-aligned bytes: ");
    dec(s.usable_page_bytes);
    puts(b" overflow=");
    puts(yes(s.overflow));
    puts(b"\r\n");
}
struct PlannedRegion {
    name: &'static [u8],
    base: u64,
    end: u64,
    cache: u8,
    executable: bool,
    writable: bool,
    dynamic: bool,
}
fn layout_plan() {
    use super::layout::*;
    let regions: [PlannedRegion; 6] = [
        PlannedRegion {
            name: b"low-canonical-unused",
            base: 0,
            end: 0x0000_8000_0000_0000,
            cache: 4,
            executable: false,
            writable: false,
            dynamic: false,
        },
        PlannedRegion {
            name: b"hhdm-direct-map",
            base: 0xffff_8000_0000_0000,
            end: KERNEL_DYNAMIC_BASE,
            cache: 0,
            executable: false,
            writable: true,
            dynamic: true,
        },
        PlannedRegion {
            name: b"kernel-dynamic",
            base: KERNEL_DYNAMIC_BASE,
            end: MMIO_BASE,
            cache: 0,
            executable: false,
            writable: true,
            dynamic: false,
        },
        PlannedRegion {
            name: b"mmio-window",
            base: MMIO_BASE,
            end: MMIO_END,
            cache: 2,
            executable: false,
            writable: true,
            dynamic: false,
        },
        PlannedRegion {
            name: b"framebuffer-window",
            base: FRAMEBUFFER_BASE,
            end: FRAMEBUFFER_END,
            cache: 1,
            executable: false,
            writable: true,
            dynamic: false,
        },
        PlannedRegion {
            name: b"kernel-image",
            base: KERNEL_IMAGE_BASE,
            end: KERNEL_IMAGE_BASE + KERNEL_IMAGE_WINDOW_BYTES,
            cache: 0,
            executable: true,
            writable: false,
            dynamic: false,
        },
    ];
    puts(b"  R4OS address layout plan:\r\n");
    for PlannedRegion {
        name,
        base,
        end,
        cache,
        executable: x,
        writable: w,
        dynamic,
    } in regions
    {
        puts(b"    ");
        puts(name);
        puts(b" 0x");
        hex(base, 16);
        puts(b"-0x");
        hex(end, 16);
        puts(b" cache=");
        puts(cache_name(cache));
        puts(b" x=");
        puts(yes(x));
        puts(b" w=");
        puts(yes(w));
        if dynamic {
            puts(b" dynamic=yes");
        }
        puts(b"\r\n");
    }
    puts(b"  Page flags: present,writable,user,wt,cd,pat,global,nx\r\n");
}
