const boot_info = @import("../bootloader/boot_info.zig");
const acpi = @import("../platform/acpi.zig");
const pci_scan = @import("../platform/pci_scan.zig");
const heap = @import("heap.zig");
const layout = @import("layout.zig");
const paging = @import("paging.zig");
const k = @import("../kernel/log.zig");

const ENTRIES: usize = layout.ENTRIES_PER_TABLE;
const ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;
const HUGE_PAGE: u64 = 1 << 7;
const HUGE_2M_SIZE: u64 = 2 * layout.MiB;
const MAX_TRACKED_TABLE_FRAMES: usize = 512;
const PROBE_CODE: u32 = 1 << 0;
const PROBE_RODATA: u32 = 1 << 1;
const PROBE_DATA: u32 = 1 << 2;
const PROBE_BSS: u32 = 1 << 3;
const PROBE_STACK: u32 = 1 << 4;
const PROBE_HHDM: u32 = 1 << 5;
const PROBE_PMM: u32 = 1 << 6;
const PROBE_BOOTINFO: u32 = 1 << 7;
const PROBE_HEAPMETA: u32 = 1 << 8;
const REQUIRED_PROBES: u32 = PROBE_CODE | PROBE_RODATA | PROBE_DATA | PROBE_BSS | PROBE_STACK | PROBE_HHDM | PROBE_PMM | PROBE_BOOTINFO | PROBE_HEAPMETA;
const DEVICE_FRAMEBUFFER: u32 = 1 << 0;
const DEVICE_LAPIC: u32 = 1 << 1;
const DEVICE_IOAPIC: u32 = 1 << 2;
const DEVICE_HPET: u32 = 1 << 3;
const DEVICE_ACPI: u32 = 1 << 4;
const DEVICE_ECAM: u32 = 1 << 5;
const DEVICE_PCIE_BAR: u32 = 1 << 6;
const DEVICE_ACPI_RESET: u32 = 1 << 7;
const POST_CODE: u32 = 1 << 0;
const POST_STACK: u32 = 1 << 1;
const POST_HHDM: u32 = 1 << 2;
const POST_PMM: u32 = 1 << 3;
const POST_PAGE_TABLE: u32 = 1 << 4;
const POST_BOOTINFO: u32 = 1 << 5;
const POST_FRAMEBUFFER: u32 = 1 << 6;
const POST_HEAP: u32 = 1 << 7;
const REQUIRED_POST_SWITCH: u32 = POST_CODE | POST_STACK | POST_HHDM | POST_PMM | POST_PAGE_TABLE | POST_BOOTINFO | POST_FRAMEBUFFER | POST_HEAP;
const MAX_DEVICE_MAPPINGS: usize = 64;
const STACK_WINDOW_BYTES: u64 = 1 * layout.MiB;
const MAX_LIMINE_TABLE_FRAMES: usize = 1024;

const PageTable = [ENTRIES]u64;

const DeviceMapping = struct {
    name: []const u8 = "",
    phys_base: u64 = 0,
    virt_base: u64 = 0,
    len: u64 = 0,
    flags: u64 = 0,
    cache_policy: layout.CachePolicy = .unknown,
    mapped: bool = false,
};

pub const MapFlags = struct {
    pub const kernel_rw: u64 = layout.PageFlags.writable | layout.PageFlags.no_execute;
    pub const kernel_ro: u64 = layout.PageFlags.no_execute;
    pub const kernel_rx: u64 = 0;
    pub const direct_map: u64 = layout.PageFlags.writable | layout.PageFlags.no_execute | layout.PageFlags.global;
    pub const framebuffer: u64 = layout.PageFlags.writable | layout.PageFlags.write_through | layout.PageFlags.page_attribute_table | layout.PageFlags.no_execute;
    pub const mmio: u64 = layout.PageFlags.writable | layout.PageFlags.cache_disable | layout.PageFlags.no_execute;
};

pub const RangeMapStats = extern struct {
    pages_4k: u64 = 0,
    pages_2m: u64 = 0,
    skipped: u64 = 0,
};

pub const WalkResult = struct {
    mapped: bool = false,
    virt: u64 = 0,
    phys: u64 = 0,
    flags: u64 = 0,
    level: u8 = 0,
    huge: bool = false,
};

const RawWalk = extern struct {
    mapped: bool,
    virt: u64,
    phys: u64,
    flags: u64,
    level: u8,
    huge: bool,
};
extern fn r4os_rust_builder_control(builder: *Builder, operation: u8) bool;
extern fn r4os_rust_builder_map(builder: *Builder, virt: u64, physical: u64, flags: u64, operation: u8) bool;
extern fn r4os_rust_builder_range(builder: *Builder, virt: u64, physical: u64, bytes: u64, flags: u64, auto: bool, out: ?*RangeMapStats) bool;
extern fn r4os_rust_builder_walk(root: u64, virt: u64, out: *RawWalk) void;

// Caller-pinned storage; only Rust changes the construction state. Boot
// adoption, device enumeration and conformance bookkeeping follow separately.
pub const Builder = extern struct {
    pml4_phys: u64 = 0,
    table_frames: [MAX_TRACKED_TABLE_FRAMES]u64 = .{0} ** MAX_TRACKED_TABLE_FRAMES,
    table_frame_count: usize = 0,
    owned: bool = false,

    pub fn init(self: *Builder) bool {
        return r4os_rust_builder_control(self, 0);
    }
    pub fn deinit(self: *Builder) void {
        _ = r4os_rust_builder_control(self, 1);
    }
    pub fn mapPage(self: *Builder, virt: u64, physical: u64, flags: u64) bool {
        return r4os_rust_builder_map(self, virt, physical, flags, 0);
    }
    pub fn ensurePage(self: *Builder, virt: u64, physical: u64, flags: u64) bool {
        return r4os_rust_builder_map(self, virt, physical, flags, 1);
    }
    pub fn ensurePageWithFlags(self: *Builder, virt: u64, physical: u64, flags: u64) bool {
        return r4os_rust_builder_map(self, virt, physical, flags, 2);
    }
    pub fn mapHuge2M(self: *Builder, virt: u64, physical: u64, flags: u64) bool {
        return r4os_rust_builder_map(self, virt, physical, flags, 3);
    }
    pub fn mapRange(self: *Builder, virt: u64, physical: u64, bytes: u64, flags: u64) bool {
        return r4os_rust_builder_range(self, virt, physical, bytes, flags, false, null);
    }
    pub fn mapRangeAuto(self: *Builder, virt: u64, physical: u64, bytes: u64, flags: u64, out: *RangeMapStats) bool {
        return r4os_rust_builder_range(self, virt, physical, bytes, flags, true, out);
    }
    pub fn walk(self: *const Builder, virt: u64) WalkResult {
        return walkFromRoot(self.pml4_phys, virt);
    }
};
comptime {
    if (@sizeOf(Builder) != 4120 or @offsetOf(Builder, "table_frame_count") != 4104 or
        @sizeOf(RangeMapStats) != 24 or @sizeOf(RawWalk) != 40)
        @compileError("Rust private page-table builder C layout mismatch");
}

pub const Stats = extern struct {
    boot_sanity_runs: u64 = 0,
    boot_sanity_failures: u64 = 0,
    last_sanity_root_phys: u64 = 0,
    last_sanity_table_frames: u64 = 0,
    last_sanity_frame: u64 = 0,
    kernel_space_runs: u64 = 0,
    kernel_space_failures: u64 = 0,
    kernel_space_root_phys: u64 = 0,
    kernel_space_table_frames: u64 = 0,
    kernel_space_source_root_phys: u64 = 0,
    kernel_space_hhdm_offset: u64 = 0,
    hhdm_pages_4k: u64 = 0,
    hhdm_pages_2m: u64 = 0,
    kernel_image_pages: u64 = 0,
    kernel_image_skipped: u64 = 0,
    boot_probe_pages: u64 = 0,
    boot_probe_skipped: u64 = 0,
    probe_bits: u32 = 0,
    debug_probe_bit: u32 = 0,
    debug_probe_virt: u64 = 0,
    debug_probe_step: u64 = 0,
    debug_probe_source_phys: u64 = 0,
    debug_probe_existing_phys: u64 = 0,
    uses_limine_subtables: bool = false,
    device_space_runs: u64 = 0,
    device_space_failures: u64 = 0,
    device_mapping_count: u64 = 0,
    device_mapping_truncated: bool = false,
    device_probe_bits: u32 = 0,
    framebuffer_bytes: u64 = 0,
    acpi_bytes: u64 = 0,
    mmio_bytes: u64 = 0,
    pcie_bar_count: u64 = 0,
    pcie_bar_bytes: u64 = 0,
    cr3_switch_runs: u64 = 0,
    cr3_switch_failures: u64 = 0,
    old_cr3: u64 = 0,
    new_cr3: u64 = 0,
    active_cr3: u64 = 0,
    post_switch_bits: u32 = 0,
    cr3_switch_done: bool = false,
    heap_committed_bytes: u64 = 0,
    heap_committed_pages: u64 = 0,
    heap_committed_skipped: u64 = 0,
    limine_pt_scan_runs: u64 = 0,
    limine_pt_scan_failures: u64 = 0,
    limine_old_table_frames: u64 = 0,
    limine_active_table_frames: u64 = 0,
    limine_referenced_frames: u64 = 0,
    limine_release_candidates: u64 = 0,
    limine_quarantined_frames: u64 = 0,
    limine_released_frames: u64 = 0,
    limine_retained_frames: u64 = 0,
    limine_quarantine_failures: u64 = 0,
    limine_old_scan_truncated: bool = false,
    limine_active_scan_truncated: bool = false,
    limine_old_root_active: bool = false,
    limine_release_safe: bool = false,
};

const RootInputs = extern struct {
    legacy_info: u64,
    map_address: u64,
    map_bytes: u64,
    framebuffer_record: u64,
    framebuffer_address: u64,
    framebuffer_bytes: u64,
    heap_metadata_base: u64,
    heap_metadata_bytes: u64,
    heap_base: u64,
    heap_committed: u64,
};
pub const RootAcpiInput = extern struct {
    rsdp: u64,
    xsdt: u64,
    rsdt: u64,
    mcfg: u64,
    madt: u64,
    fadt: u64,
    hpet: u64,
    dsdt: u64,
    x_dsdt: u64,
    lapic: u64,
    first_ioapic: u64,
    hpet_base: u64,
    mcfg_base: u64,
    reset_address: u64,
    ioapics: [4]u64,
    ioapic_count: u32,
    start_bus: u8,
    end_bus: u8,
    reset_address_space: u8,
    found: bool,
    reset_supported: bool,
    reset_valid: bool,
};
const RawMapping = extern struct {
    name_ptr: [*]const u8,
    name_len: usize,
    phys_base: u64,
    virt_base: u64,
    len: u64,
    flags: u64,
    cache_policy: u8,
    mapped: bool,
};
extern fn r4os_rust_root_control(operation: u8) bool;
extern fn r4os_rust_root_devices(input: *const RootAcpiInput) bool;
extern fn r4os_rust_root_stats(output: *Stats) void;
extern fn r4os_rust_root_mapping(index: usize, output: *RawMapping) bool;
extern fn r4os_rust_root_alias(physical: u64, bytes: u64, output: *u64) bool;

// Immutable, value-only cuts to the still active BootInfo/Heap/PCI owners.
// No root-state borrow leaves Rust and no provider retains the output.
export fn r4os_zig_root_inputs(output: *RootInputs) callconv(.c) void {
    if (@import("builtin").is_test) @panic("boot root inputs requested by host fixture");
    const mem = boot_info.memoryMap();
    const metadata = heap.metadataRange();
    const current = heap.stats();
    output.* = .{
        .legacy_info = @intFromPtr(boot_info.get()),
        .map_address = @intFromPtr(mem.ptr),
        .map_bytes = mem.len * @sizeOf(boot_info.MemoryMapEntry),
        .framebuffer_record = 0,
        .framebuffer_address = 0,
        .framebuffer_bytes = 0,
        .heap_metadata_base = metadata.base,
        .heap_metadata_bytes = metadata.len,
        .heap_base = current.base,
        .heap_committed = current.committed_bytes,
    };
    if (boot_info.framebuffer()) |fb| {
        output.framebuffer_record = @intFromPtr(fb);
        output.framebuffer_address = @intFromPtr(fb.address);
        const product = @mulWithOverflow(fb.pitch, fb.height);
        output.framebuffer_bytes = if (product[1] == 0) product[0] else 0;
    }
}
export fn r4os_zig_root_ecam(base: u64, start: u8, end: u8, output_base: *u64, output_bytes: *u64) callconv(.c) bool {
    const window = pci_scan.ecamWindow(base, start, end) orelse return false;
    output_base.* = window.base;
    output_bytes.* = window.bytes;
    return true;
}
pub fn bootSanityCheck() bool {
    return r4os_rust_root_control(0);
}
pub fn buildKernelSpaceDryRun() bool {
    return r4os_rust_root_control(1);
}
pub fn switchToKernelSpaceIfReady() bool {
    return r4os_rust_root_control(2);
}
pub fn mapDeviceSpaceDryRun(info: acpi.Info) bool {
    const raw = acpiInput(info);
    return r4os_rust_root_devices(&raw);
}
pub fn acpiInput(info: acpi.Info) RootAcpiInput {
    var raw: RootAcpiInput = .{
        .rsdp = info.rsdp_phys,
        .xsdt = info.xsdt_phys,
        .rsdt = info.rsdt_phys,
        .mcfg = info.mcfg_phys,
        .madt = info.madt_phys,
        .fadt = info.fadt_phys,
        .hpet = info.hpet_phys,
        .dsdt = info.fadt_dsdt_phys,
        .x_dsdt = info.fadt_x_dsdt_phys,
        .lapic = info.madt_lapic_address,
        .first_ioapic = info.madt_first_ioapic_address,
        .hpet_base = info.hpet_base,
        .mcfg_base = info.mcfg_base,
        .reset_address = info.fadt_reset_address,
        .ioapics = .{0} ** 4,
        .ioapic_count = info.madt_ioapic_count,
        .start_bus = info.mcfg_start_bus,
        .end_bus = info.mcfg_end_bus,
        .reset_address_space = info.fadt_reset_address_space,
        .found = info.found,
        .reset_supported = info.fadt_reset_supported,
        .reset_valid = info.fadt_reset_gas_valid,
    };
    for (0..4) |index_value| raw.ioapics[index_value] = info.madt_ioapics[index_value].address;
    return raw;
}
pub fn stats() Stats {
    var result: Stats = undefined;
    r4os_rust_root_stats(&result);
    return result;
}
fn mapping(index_value: usize) ?DeviceMapping {
    var raw: RawMapping = undefined;
    if (!r4os_rust_root_mapping(index_value, &raw)) return null;
    return .{ .name = raw.name_ptr[0..raw.name_len], .phys_base = raw.phys_base, .virt_base = raw.virt_base, .len = raw.len, .flags = raw.flags, .cache_policy = @enumFromInt(raw.cache_policy), .mapped = raw.mapped };
}
pub fn mmioAliasForPhysical(physical: u64, bytes: u64) ?u64 {
    var address: u64 = undefined;
    return if (r4os_rust_root_alias(physical, bytes, &address)) address else null;
}
comptime {
    if (acpi.MAX_MADT_IOAPICS != 4 or @sizeOf(RootInputs) != 80 or @sizeOf(RootAcpiInput) != 160 or
        @sizeOf(RawMapping) != 56 or @sizeOf(Stats) != 416)
        @compileError("Rust boot root C layout mismatch");
}

pub fn dumpStats() void {
    const s = stats();
    k.puts("  Page-table builder: sanity=");
    k.putDec(s.boot_sanity_runs);
    k.puts(" failures=");
    k.putDec(s.boot_sanity_failures);
    k.puts(" last-root=0x");
    k.putHex(s.last_sanity_root_phys, 16);
    k.puts(" tables=");
    k.putDec(s.last_sanity_table_frames);
    k.puts("\r\n");

    k.puts("  R4OS PML4 dry-run: runs=");
    k.putDec(s.kernel_space_runs);
    k.puts(" failures=");
    k.putDec(s.kernel_space_failures);
    k.puts(" root=0x");
    k.putHex(s.kernel_space_root_phys, 16);
    k.puts(" tables=");
    k.putDec(s.kernel_space_table_frames);
    k.puts(" source-cr3=0x");
    k.putHex(s.kernel_space_source_root_phys, 16);
    k.puts(" limine-subtables=");
    k.puts(if (s.uses_limine_subtables) "yes" else "no");
    k.puts("\r\n");

    k.puts("  R4OS PML4 maps: hhdm 2M=");
    k.putDec(s.hhdm_pages_2m);
    k.puts(" 4K=");
    k.putDec(s.hhdm_pages_4k);
    k.puts(" kernel-pages=");
    k.putDec(s.kernel_image_pages);
    k.puts(" kernel-skipped=");
    k.putDec(s.kernel_image_skipped);
    k.puts(" boot-probe-pages=");
    k.putDec(s.boot_probe_pages);
    k.puts("\r\n");

    k.puts("  R4OS PML4 probes:");
    dumpProbe(" code", s.probe_bits, PROBE_CODE);
    dumpProbe(" rodata", s.probe_bits, PROBE_RODATA);
    dumpProbe(" data", s.probe_bits, PROBE_DATA);
    dumpProbe(" bss", s.probe_bits, PROBE_BSS);
    dumpProbe(" stack", s.probe_bits, PROBE_STACK);
    dumpProbe(" hhdm", s.probe_bits, PROBE_HHDM);
    dumpProbe(" pmm", s.probe_bits, PROBE_PMM);
    dumpProbe(" bootinfo", s.probe_bits, PROBE_BOOTINFO);
    dumpProbe(" heapmeta", s.probe_bits, PROBE_HEAPMETA);
    k.puts("\r\n");
    if (s.debug_probe_bit != 0) {
        k.puts("  R4OS PML4 probe debug: bit=0x");
        k.putHex(s.debug_probe_bit, 8);
        k.puts(" virt=0x");
        k.putHex(s.debug_probe_virt, 16);
        k.puts(" step=");
        k.putDec(s.debug_probe_step);
        k.puts(" source=0x");
        k.putHex(s.debug_probe_source_phys, 16);
        k.puts(" existing=0x");
        k.putHex(s.debug_probe_existing_phys, 16);
        k.puts("\r\n");
    }
}

pub fn dumpDeviceMappings() void {
    const s = stats();
    k.puts("  R4OS device mappings: runs=");
    k.putDec(s.device_space_runs);
    k.puts(" failures=");
    k.putDec(s.device_space_failures);
    k.puts(" records=");
    k.putDec(s.device_mapping_count);
    k.puts(" truncated=");
    k.puts(if (s.device_mapping_truncated) "yes" else "no");
    k.puts("\r\n");

    k.puts("  R4OS device bytes: framebuffer=");
    k.putDec(s.framebuffer_bytes);
    k.puts(" acpi=");
    k.putDec(s.acpi_bytes);
    k.puts(" mmio=");
    k.putDec(s.mmio_bytes);
    k.puts(" pcie-bars=");
    k.putDec(s.pcie_bar_bytes);
    k.puts(" pcie-bar-count=");
    k.putDec(s.pcie_bar_count);
    k.puts("\r\n");

    k.puts("  R4OS device probes:");
    dumpProbe(" framebuffer", s.device_probe_bits, DEVICE_FRAMEBUFFER);
    dumpProbe(" lapic", s.device_probe_bits, DEVICE_LAPIC);
    dumpProbe(" ioapic", s.device_probe_bits, DEVICE_IOAPIC);
    dumpProbe(" hpet", s.device_probe_bits, DEVICE_HPET);
    dumpProbe(" acpi", s.device_probe_bits, DEVICE_ACPI);
    dumpProbe(" acpi-reset", s.device_probe_bits, DEVICE_ACPI_RESET);
    dumpProbe(" ecam", s.device_probe_bits, DEVICE_ECAM);
    dumpProbe(" pcie-bar", s.device_probe_bits, DEVICE_PCIE_BAR);
    k.puts("\r\n");

    k.puts("  R4OS device cache-policy: framebuffer=");
    k.puts(layout.cachePolicyName(.framebuffer_write_combining));
    k.puts(" mmio=");
    k.puts(layout.cachePolicyName(.mmio_uncached));
    k.puts(" acpi=");
    k.puts(layout.cachePolicyName(.acpi_firmware));
    k.puts("\r\n");

    var i: usize = 0;
    while (i < s.device_mapping_count) : (i += 1) {
        const m = mapping(i) orelse break;
        k.puts("    ");
        k.puts(m.name);
        k.puts(" phys=0x");
        k.putHex(m.phys_base, 16);
        k.puts(" virt=0x");
        k.putHex(m.virt_base, 16);
        k.puts(" len=0x");
        k.putHex(m.len, 16);
        k.puts(" cache=");
        k.puts(layout.cachePolicyName(m.cache_policy));
        k.puts(" flags=0x");
        k.putHex(m.flags, 16);
        k.puts(" mapped=");
        k.puts(if (m.mapped) "yes" else "no");
        k.puts("\r\n");
    }
}

pub fn dumpCr3SwitchStatus() void {
    const s = stats();
    k.puts("  R4OS CR3 switch: runs=");
    k.putDec(s.cr3_switch_runs);
    k.puts(" failures=");
    k.putDec(s.cr3_switch_failures);
    k.puts(" old=0x");
    k.putHex(s.old_cr3, 16);
    k.puts(" new=0x");
    k.putHex(s.new_cr3, 16);
    k.puts(" active=0x");
    k.putHex(s.active_cr3, 16);
    k.puts(" done=");
    k.puts(if (s.cr3_switch_done) "yes" else "no");
    k.puts("\r\n");

    k.puts("  R4OS CR3 post-checks:");
    dumpProbe(" code", s.post_switch_bits, POST_CODE);
    dumpProbe(" stack", s.post_switch_bits, POST_STACK);
    dumpProbe(" hhdm", s.post_switch_bits, POST_HHDM);
    dumpProbe(" pmm", s.post_switch_bits, POST_PMM);
    dumpProbe(" page-table", s.post_switch_bits, POST_PAGE_TABLE);
    dumpProbe(" bootinfo", s.post_switch_bits, POST_BOOTINFO);
    dumpProbe(" framebuffer", s.post_switch_bits, POST_FRAMEBUFFER);
    dumpProbe(" heap", s.post_switch_bits, POST_HEAP);
    k.puts("\r\n");

    k.puts("  R4OS runtime maps: heap-bytes=");
    k.putDec(s.heap_committed_bytes);
    k.puts(" heap-pages=");
    k.putDec(s.heap_committed_pages);
    k.puts(" heap-skipped=");
    k.putDec(s.heap_committed_skipped);
    k.puts("\r\n");
    if (s.debug_probe_bit != 0) {
        k.puts("  R4OS CR3 debug probe: bit=0x");
        k.putHex(s.debug_probe_bit, 8);
        k.puts(" virt=0x");
        k.putHex(s.debug_probe_virt, 16);
        k.puts(" step=");
        k.putDec(s.debug_probe_step);
        k.puts(" source=0x");
        k.putHex(s.debug_probe_source_phys, 16);
        k.puts(" existing=0x");
        k.putHex(s.debug_probe_existing_phys, 16);
        k.puts("\r\n");
    }

    dumpLiminePageTableStatus(s);
    k.puts("  R4OS TLB: CR3 write flushed active address translations; later single-page changes use INVLPG\r\n");
    paging.dumpRuntimeStatus();
}

pub fn walkFromRoot(root_phys: u64, virt: u64) WalkResult {
    var raw: RawWalk = undefined;
    r4os_rust_builder_walk(root_phys, virt, &raw);
    return .{ .mapped = raw.mapped, .virt = raw.virt, .phys = raw.phys, .flags = raw.flags, .level = raw.level, .huge = raw.huge };
}

fn dumpLiminePageTableStatus(s: Stats) void {
    k.puts("  Limine page-tables: old=");
    k.putDec(s.limine_old_table_frames);
    k.puts(" active=");
    k.putDec(s.limine_active_table_frames);
    k.puts(" referenced=");
    k.putDec(s.limine_referenced_frames);
    k.puts(" candidates=");
    k.putDec(s.limine_release_candidates);
    k.puts(" quarantined=");
    k.putDec(s.limine_quarantined_frames);
    k.puts(" released=");
    k.putDec(s.limine_released_frames);
    k.puts(" retained=");
    k.putDec(s.limine_retained_frames);
    k.puts(" failures=");
    k.putDec(s.limine_pt_scan_failures + s.limine_quarantine_failures);
    k.puts(" truncated=");
    k.puts(if (s.limine_old_scan_truncated or s.limine_active_scan_truncated) "yes" else "no");
    k.puts(" old-active=");
    k.puts(if (s.limine_old_root_active) "yes" else "no");
    k.puts(" decision=");
    k.puts(liminePageTableDecision(s));
    k.puts("\r\n");
}

fn liminePageTableDecision(s: Stats) []const u8 {
    if (s.limine_pt_scan_failures != 0) return "retain-scan-failed";
    if (s.limine_old_scan_truncated or s.limine_active_scan_truncated) return "retain-truncated";
    if (s.limine_old_root_active) return "retain-active";
    if (s.limine_referenced_frames != 0) return "retain-referenced";
    if (!s.limine_release_safe) return "retain-unproven";
    if (s.limine_quarantine_failures != 0) return "retain-partial";
    if (s.limine_quarantined_frames == s.limine_release_candidates and s.limine_release_candidates != 0) return "quarantine";
    return "retain-unproven";
}

fn dumpProbe(name: []const u8, bits: u32, bit: u32) void {
    k.puts(name);
    k.puts("=");
    k.puts(if ((bits & bit) != 0) "ok" else "missing");
}
