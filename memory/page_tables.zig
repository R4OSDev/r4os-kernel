const boot_info = @import("../bootloader/boot_info.zig");
const acpi = @import("../platform/acpi.zig");
const blocks = @import("blocks.zig");
const heap = @import("heap.zig");
const layout = @import("layout.zig");
const paging = @import("paging.zig");
const phys = @import("phys.zig");
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

const AcpiSdtHeader = extern struct {
    signature: [4]u8,
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [6]u8,
    oem_table_id: [8]u8,
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
};

extern fn r4os_read_cr3() callconv(.c) u64;
extern fn r4os_write_cr3(value: u64) callconv(.c) void;

pub const MapFlags = struct {
    pub const kernel_rw: u64 = layout.PageFlags.writable | layout.PageFlags.no_execute;
    pub const kernel_ro: u64 = layout.PageFlags.no_execute;
    pub const kernel_rx: u64 = 0;
    pub const direct_map: u64 = layout.PageFlags.writable | layout.PageFlags.no_execute | layout.PageFlags.global;
    pub const framebuffer: u64 = layout.PageFlags.writable | layout.PageFlags.write_through | layout.PageFlags.page_attribute_table | layout.PageFlags.no_execute;
    pub const mmio: u64 = layout.PageFlags.writable | layout.PageFlags.cache_disable | layout.PageFlags.no_execute;
};

pub const RangeMapStats = struct {
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

pub const Builder = struct {
    pml4_phys: u64 = 0,
    table_frames: [MAX_TRACKED_TABLE_FRAMES]u64 = .{0} ** MAX_TRACKED_TABLE_FRAMES,
    table_frame_count: usize = 0,
    owned: bool = false,

    pub fn init(self: *Builder) bool {
        self.* = .{};
        const root = self.allocTableFrame() orelse return false;
        self.pml4_phys = root;
        self.owned = true;
        return true;
    }

    pub fn deinit(self: *Builder) void {
        var i = self.table_frame_count;
        while (i > 0) {
            i -= 1;
            releaseFrame(self.table_frames[i]);
            self.table_frames[i] = 0;
        }
        self.* = .{};
    }

    pub fn mapPage(self: *Builder, virt: u64, frame_phys: u64, flags: u64) bool {
        if (self.pml4_phys == 0 or !isAligned(virt) or !isAligned(frame_phys)) return false;
        const pt = self.getOrCreateLeafTable(virt) orelse return false;
        const pti = index(virt, 12);
        if ((pt[pti] & layout.PageFlags.present) != 0) return false;
        pt[pti] = (frame_phys & ADDR_MASK) | sanitizeLeafFlags(flags) | layout.PageFlags.present;
        return true;
    }

    pub fn ensurePage(self: *Builder, virt: u64, frame_phys: u64, flags: u64) bool {
        if (self.pml4_phys == 0 or !isAligned(virt) or !isAligned(frame_phys)) return false;
        const existing = self.walk(virt);
        if (existing.mapped) return alignDown(existing.phys, layout.PAGE_SIZE) == frame_phys;
        return self.mapPage(virt, frame_phys, flags);
    }

    pub fn ensurePageWithFlags(self: *Builder, virt: u64, frame_phys: u64, flags: u64) bool {
        if (self.pml4_phys == 0 or !isAligned(virt) or !isAligned(frame_phys)) return false;
        const pt = self.getOrCreateLeafTableWithSplit(virt) orelse return false;
        const pti = index(virt, 12);
        if ((pt[pti] & layout.PageFlags.present) != 0) {
            if ((pt[pti] & ADDR_MASK) != frame_phys) return false;
        }
        pt[pti] = (frame_phys & ADDR_MASK) | sanitizeLeafFlags(flags) | layout.PageFlags.present;
        return true;
    }

    pub fn mapHuge2M(self: *Builder, virt: u64, frame_phys: u64, flags: u64) bool {
        if (self.pml4_phys == 0 or !isAlignedTo(virt, HUGE_2M_SIZE) or !isAlignedTo(frame_phys, HUGE_2M_SIZE)) return false;
        const pd = self.getOrCreatePageDirectory(virt) orelse return false;
        const pdi = index(virt, 21);
        if ((pd[pdi] & layout.PageFlags.present) != 0) return false;
        pd[pdi] = (frame_phys & ADDR_MASK) | sanitizeLeafFlags(flags) | layout.PageFlags.present | HUGE_PAGE;
        return true;
    }

    pub fn mapRange(self: *Builder, virt_base: u64, phys_base: u64, byte_len: u64, flags: u64) bool {
        if (byte_len == 0 or !isAligned(virt_base) or !isAligned(phys_base)) return false;
        const end = alignUpChecked(byte_len, layout.PAGE_SIZE) orelse return false;
        var offset: u64 = 0;
        while (offset < end) : (offset += layout.PAGE_SIZE) {
            if (!self.mapPage(virt_base + offset, phys_base + offset, flags)) return false;
        }
        return true;
    }

    pub fn mapRangeAuto(self: *Builder, virt_base: u64, phys_base: u64, byte_len: u64, flags: u64, out: *RangeMapStats) bool {
        if (byte_len == 0 or !isAligned(virt_base) or !isAligned(phys_base)) return false;
        const end = alignUpChecked(byte_len, layout.PAGE_SIZE) orelse return false;
        var offset: u64 = 0;
        while (offset < end) {
            const virt = virt_base + offset;
            const phys_addr = phys_base + offset;
            const remaining = end - offset;
            if (remaining >= HUGE_2M_SIZE and isAlignedTo(virt, HUGE_2M_SIZE) and isAlignedTo(phys_addr, HUGE_2M_SIZE)) {
                if (!self.mapHuge2M(virt, phys_addr, flags)) return false;
                out.pages_2m += 1;
                offset += HUGE_2M_SIZE;
            } else {
                if (!self.mapPage(virt, phys_addr, flags)) return false;
                out.pages_4k += 1;
                offset += layout.PAGE_SIZE;
            }
        }
        return true;
    }

    pub fn walk(self: *const Builder, virt: u64) WalkResult {
        if (self.pml4_phys == 0) return .{ .virt = virt };
        return walkFromRoot(self.pml4_phys, virt);
    }

    fn getOrCreateLeafTable(self: *Builder, virt: u64) ?*PageTable {
        const pml4 = tableFromPhys(self.pml4_phys) orelse return null;
        const pdpt = self.nextTableOrCreate(pml4, index(virt, 39)) orelse return null;
        const pd = self.nextTableOrCreate(pdpt, index(virt, 30)) orelse return null;
        return self.nextTableOrCreate(pd, index(virt, 21));
    }

    fn getOrCreateLeafTableWithSplit(self: *Builder, virt: u64) ?*PageTable {
        const pml4 = tableFromPhys(self.pml4_phys) orelse return null;
        const pdpt = self.nextTableOrCreate(pml4, index(virt, 39)) orelse return null;
        const pd = self.nextTableOrCreate(pdpt, index(virt, 30)) orelse return null;
        const pdi = index(virt, 21);
        const entry = pd[pdi];
        if ((entry & layout.PageFlags.present) != 0 and (entry & HUGE_PAGE) != 0) {
            const table_frame = self.allocTableFrame() orelse return null;
            const pt = tableFromPhys(table_frame) orelse return null;
            const phys_base = entry & ADDR_MASK;
            const inherited_flags = sanitizeLeafFlags(entry & ~ADDR_MASK & ~HUGE_PAGE);
            var page_index: usize = 0;
            while (page_index < ENTRIES) : (page_index += 1) {
                pt[page_index] = (phys_base + @as(u64, page_index) * layout.PAGE_SIZE) |
                    inherited_flags | layout.PageFlags.present;
            }
            pd[pdi] = (table_frame & ADDR_MASK) | layout.PageFlags.present | layout.PageFlags.writable;
            return pt;
        }
        return self.nextTableOrCreate(pd, pdi);
    }

    fn getOrCreatePageDirectory(self: *Builder, virt: u64) ?*PageTable {
        const pml4 = tableFromPhys(self.pml4_phys) orelse return null;
        const pdpt = self.nextTableOrCreate(pml4, index(virt, 39)) orelse return null;
        return self.nextTableOrCreate(pdpt, index(virt, 30));
    }

    fn nextTableOrCreate(self: *Builder, table: *PageTable, idx: usize) ?*PageTable {
        if ((table[idx] & layout.PageFlags.present) == 0) {
            const frame = self.allocTableFrame() orelse return null;
            table[idx] = (frame & ADDR_MASK) | layout.PageFlags.present | layout.PageFlags.writable;
        }
        if ((table[idx] & HUGE_PAGE) != 0) return null;
        return tableFromPhys(table[idx] & ADDR_MASK);
    }

    fn allocTableFrame(self: *Builder) ?u64 {
        if (self.table_frame_count >= self.table_frames.len) return null;
        const frame = allocFrame(.page_table, "page-table") orelse return null;
        const table = tableFromPhys(frame) orelse {
            releaseFrame(frame);
            return null;
        };
        @memset(table, 0);
        self.table_frames[self.table_frame_count] = frame;
        self.table_frame_count += 1;
        return frame;
    }
};

pub const Stats = struct {
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

var stats_value: Stats = .{};
var kernel_space_builder: Builder = .{};
var kernel_space_ready: bool = false;
var kernel_space_bss_probe: u64 = 0;
var cr3_switch_guard: u64 = 0;
var device_mappings: [MAX_DEVICE_MAPPINGS]DeviceMapping = .{DeviceMapping{}} ** MAX_DEVICE_MAPPINGS;
var device_mapping_count: usize = 0;
var next_mmio_offset: u64 = 0;
var next_framebuffer_offset: u64 = 0;
var limine_table_frames: [MAX_LIMINE_TABLE_FRAMES]u64 = .{0} ** MAX_LIMINE_TABLE_FRAMES;
var active_table_frames: [MAX_LIMINE_TABLE_FRAMES]u64 = .{0} ** MAX_LIMINE_TABLE_FRAMES;

pub fn bootSanityCheck() bool {
    stats_value.boot_sanity_runs += 1;

    var builder: Builder = .{};
    if (!builder.init()) return failBootSanity();
    defer builder.deinit();

    const test_frame = allocFrame(.kernel, "pt-sanity-frame") orelse return failBootSanity();
    defer releaseFrame(test_frame);

    const virt = layout.KERNEL_DYNAMIC_BASE + 0x20_0000;
    if (!builder.mapPage(virt, test_frame, MapFlags.kernel_rw)) return failBootSanity();

    const walked = builder.walk(virt);
    if (!walked.mapped or walked.phys != test_frame or walked.level != 1 or walked.huge) return failBootSanity();
    if ((walked.flags & layout.PageFlags.writable) == 0) return failBootSanity();
    if ((walked.flags & layout.PageFlags.no_execute) == 0) return failBootSanity();

    const missing = builder.walk(virt + layout.PAGE_SIZE);
    if (missing.mapped) return failBootSanity();

    stats_value.last_sanity_root_phys = builder.pml4_phys;
    stats_value.last_sanity_table_frames = builder.table_frame_count;
    stats_value.last_sanity_frame = test_frame;
    return true;
}

pub fn buildKernelSpaceDryRun() bool {
    if (kernel_space_ready) return true;
    stats_value.kernel_space_runs += 1;
    stats_value.probe_bits = 0;
    stats_value.hhdm_pages_4k = 0;
    stats_value.hhdm_pages_2m = 0;
    stats_value.kernel_image_pages = 0;
    stats_value.kernel_image_skipped = 0;
    stats_value.boot_probe_pages = 0;
    stats_value.boot_probe_skipped = 0;
    stats_value.uses_limine_subtables = false;

    const source_root = paging.rootPhys();
    if (source_root == 0) return failKernelSpace();
    stats_value.kernel_space_source_root_phys = source_root;

    if (!kernel_space_builder.init()) return failKernelSpace();
    errdefer kernel_space_builder.deinit();

    if (!mapHhdmDirectMap(&kernel_space_builder)) return failKernelSpace();
    if (!mapKernelImageFromActive(&kernel_space_builder, source_root)) return failKernelSpace();
    if (!mapBootSanityProbes(&kernel_space_builder, source_root)) return failKernelSpace();
    if ((stats_value.probe_bits & REQUIRED_PROBES) != REQUIRED_PROBES) return failKernelSpace();

    stats_value.kernel_space_root_phys = kernel_space_builder.pml4_phys;
    stats_value.kernel_space_table_frames = kernel_space_builder.table_frame_count;
    kernel_space_ready = true;
    return true;
}

pub fn mapDeviceSpaceDryRun(info: acpi.Info) bool {
    stats_value.device_space_runs += 1;
    resetDeviceMappingState();
    if (!kernel_space_ready) return failDeviceSpace();

    const source_root = paging.rootPhys();
    if (source_root == 0) return failDeviceSpace();

    var required: u32 = DEVICE_FRAMEBUFFER;
    if (info.found) required |= DEVICE_ACPI;
    if (info.madt_lapic_address != 0) required |= DEVICE_LAPIC;
    if (info.madt_first_ioapic_address != 0) required |= DEVICE_IOAPIC;
    if (info.hpet_base != 0) required |= DEVICE_HPET;
    if (info.mcfg_base != 0 and info.mcfg_start_bus <= info.mcfg_end_bus) required |= DEVICE_ECAM;
    if (info.fadt_reset_supported and info.fadt_reset_gas_valid and info.fadt_reset_address_space == 0) required |= DEVICE_ACPI_RESET;

    if (!mapFramebufferWindow(&kernel_space_builder, source_root)) return failDeviceSpace();
    if (!mapAcpiFirmware(info)) return failDeviceSpace();
    if (!mapPlatformMmio(info)) return failDeviceSpace();
    if (!mapPcieEcam(info)) return failDeviceSpace();

    stats_value.device_mapping_count = device_mapping_count;
    if ((stats_value.device_probe_bits & required) != required) return failDeviceSpace();
    return true;
}

pub fn switchToKernelSpaceIfReady() bool {
    stats_value.cr3_switch_runs += 1;
    stats_value.post_switch_bits = 0;
    stats_value.cr3_switch_done = false;

    if (!kernel_space_ready) return failCr3Switch();
    if (stats_value.device_space_runs == 0 or stats_value.device_space_failures != 0) return failCr3Switch();

    const new_root = kernel_space_builder.pml4_phys;
    if (!validPreparedRoot(new_root)) return failCr3Switch();
    if (!mapRuntimeKernelStateFromActive(paging.rootPhys())) return failCr3Switch();
    if (!preSwitchChecks(new_root)) return failCr3Switch();

    const old_cr3_raw = r4os_read_cr3();
    const old_root = old_cr3_raw & ADDR_MASK;
    stats_value.old_cr3 = old_root;
    stats_value.new_cr3 = new_root;

    r4os_write_cr3(new_root);
    const active = r4os_read_cr3() & ADDR_MASK;
    stats_value.active_cr3 = active;
    if (active != new_root) {
        r4os_write_cr3(old_cr3_raw);
        _ = paging.adoptRootPhys(old_root, .bootloader);
        stats_value.active_cr3 = r4os_read_cr3() & ADDR_MASK;
        return failCr3Switch();
    }

    if (!paging.adoptRootPhys(new_root, .r4os)) {
        r4os_write_cr3(old_cr3_raw);
        _ = paging.adoptRootPhys(old_root, .bootloader);
        stats_value.active_cr3 = r4os_read_cr3() & ADDR_MASK;
        return failCr3Switch();
    }

    if (!postSwitchChecks(new_root)) {
        r4os_write_cr3(old_cr3_raw);
        _ = paging.adoptRootPhys(old_root, .bootloader);
        stats_value.active_cr3 = r4os_read_cr3() & ADDR_MASK;
        return failCr3Switch();
    }

    stats_value.cr3_switch_done = true;
    inspectLiminePageTables(old_root, new_root);
    return true;
}

pub fn stats() Stats {
    return stats_value;
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
    while (i < device_mapping_count) : (i += 1) {
        const m = device_mappings[i];
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

pub fn mmioAliasForPhysical(phys_addr: u64, byte_len: u64) ?u64 {
    if (phys_addr == 0 or byte_len == 0) return null;
    const phys_end = checkedAdd(phys_addr, byte_len) orelse return null;
    var mapping_index: usize = 0;
    while (mapping_index < device_mapping_count) : (mapping_index += 1) {
        const mapping = device_mappings[mapping_index];
        if (!mapping.mapped or mapping.cache_policy != .mmio_uncached or mapping.len == 0) continue;
        const mapping_end = checkedAdd(mapping.phys_base, mapping.len) orelse continue;
        if (phys_addr < mapping.phys_base or phys_end > mapping_end) continue;
        return checkedAdd(mapping.virt_base, phys_addr - mapping.phys_base);
    }
    return null;
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
    if (root_phys == 0) return .{ .virt = virt };

    const pml4 = tableFromPhys(root_phys) orelse return .{ .virt = virt };
    const pml4_entry = pml4[index(virt, 39)];
    if ((pml4_entry & layout.PageFlags.present) == 0) return .{ .virt = virt };

    const pdpt = tableFromPhys(pml4_entry & ADDR_MASK) orelse return .{ .virt = virt };
    const pdpt_entry = pdpt[index(virt, 30)];
    if ((pdpt_entry & layout.PageFlags.present) == 0) return .{ .virt = virt };
    if ((pdpt_entry & HUGE_PAGE) != 0) {
        return hugeResult(virt, pdpt_entry, 1 * layout.GiB, 3);
    }

    const pd = tableFromPhys(pdpt_entry & ADDR_MASK) orelse return .{ .virt = virt };
    const pd_entry = pd[index(virt, 21)];
    if ((pd_entry & layout.PageFlags.present) == 0) return .{ .virt = virt };
    if ((pd_entry & HUGE_PAGE) != 0) {
        return hugeResult(virt, pd_entry, 2 * layout.MiB, 2);
    }

    const pt = tableFromPhys(pd_entry & ADDR_MASK) orelse return .{ .virt = virt };
    const pt_entry = pt[index(virt, 12)];
    if ((pt_entry & layout.PageFlags.present) == 0) return .{ .virt = virt };
    return .{
        .mapped = true,
        .virt = virt,
        .phys = (pt_entry & ADDR_MASK) | (virt & (layout.PAGE_SIZE - 1)),
        .flags = pt_entry & ~ADDR_MASK,
        .level = 1,
        .huge = false,
    };
}

fn hugeResult(virt: u64, entry: u64, size: u64, level: u8) WalkResult {
    return .{
        .mapped = true,
        .virt = virt,
        .phys = (entry & ADDR_MASK) | (virt & (size - 1)),
        .flags = entry & ~ADDR_MASK,
        .level = level,
        .huge = true,
    };
}

fn failBootSanity() bool {
    stats_value.boot_sanity_failures += 1;
    return false;
}

fn failKernelSpace() bool {
    stats_value.kernel_space_failures += 1;
    if (!kernel_space_ready) kernel_space_builder.deinit();
    return false;
}

fn failDeviceSpace() bool {
    stats_value.device_space_failures += 1;
    stats_value.device_mapping_count = device_mapping_count;
    return false;
}

fn failCr3Switch() bool {
    stats_value.cr3_switch_failures += 1;
    return false;
}

fn resetDeviceMappingState() void {
    device_mapping_count = 0;
    next_mmio_offset = 0;
    next_framebuffer_offset = 0;
    stats_value.device_mapping_count = 0;
    stats_value.device_mapping_truncated = false;
    stats_value.device_probe_bits = 0;
    stats_value.framebuffer_bytes = 0;
    stats_value.acpi_bytes = 0;
    stats_value.mmio_bytes = 0;
    stats_value.pcie_bar_count = 0;
    stats_value.pcie_bar_bytes = 0;
    var i: usize = 0;
    while (i < device_mappings.len) : (i += 1) device_mappings[i] = .{};
}

fn mapHhdmDirectMap(builder: *Builder) bool {
    const offset = boot_info.hhdmOffset() orelse return false;
    stats_value.kernel_space_hhdm_offset = offset;

    const entries = boot_info.memoryMap();
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        if (!entry.valid or entry.length == 0) continue;
        const base = alignDown(entry.base, layout.PAGE_SIZE);
        const end = alignUpChecked(entry.end, layout.PAGE_SIZE) orelse return false;
        if (end <= base) continue;
        const virt = checkedAdd(offset, base) orelse return false;
        var mapped: RangeMapStats = .{};
        if (!builder.mapRangeAuto(virt, base, end - base, MapFlags.direct_map, &mapped)) return false;
        stats_value.hhdm_pages_4k += mapped.pages_4k;
        stats_value.hhdm_pages_2m += mapped.pages_2m;
    }
    return true;
}

fn mapFramebufferWindow(builder: *Builder, source_root: u64) bool {
    const fb = boot_info.framebuffer() orelse return false;
    const source_virt: u64 = @intFromPtr(fb.address);
    const byte_len = checkedMul(fb.pitch, fb.height) orelse return false;
    if (byte_len == 0) return false;

    const start = alignDown(source_virt, layout.PAGE_SIZE);
    const end = alignUpChecked(checkedAdd(source_virt, byte_len) orelse return false, layout.PAGE_SIZE) orelse return false;
    var offset: u64 = 0;
    var page = start;
    while (page < end) : ({
        page += layout.PAGE_SIZE;
        offset += layout.PAGE_SIZE;
    }) {
        const source = walkFromRoot(source_root, page);
        if (!source.mapped) return false;
        const frame = alignDown(source.phys, layout.PAGE_SIZE);
        const virt = checkedAdd(layout.FRAMEBUFFER_BASE, next_framebuffer_offset + offset) orelse return false;
        if (!builder.ensurePageWithFlags(virt, frame, MapFlags.framebuffer)) return false;
    }

    const mapped_len = end - start;
    recordMapping("framebuffer-window", alignDown(walkFromRoot(source_root, start).phys, layout.PAGE_SIZE), layout.FRAMEBUFFER_BASE + next_framebuffer_offset, mapped_len, MapFlags.framebuffer, .framebuffer_write_combining, true);
    next_framebuffer_offset += mapped_len;
    stats_value.framebuffer_bytes += mapped_len;
    stats_value.device_probe_bits |= DEVICE_FRAMEBUFFER;
    return true;
}

fn mapAcpiFirmware(info: acpi.Info) bool {
    if (!info.found) return true;

    var ok = true;
    ok = verifyAcpiRange("acpi-rsdp", info.rsdp_phys, 4096) and ok;
    if (info.xsdt_phys != 0) ok = verifyAcpiTable("acpi-xsdt", info.xsdt_phys) and ok;
    if (info.rsdt_phys != 0) ok = verifyAcpiTable("acpi-rsdt", info.rsdt_phys) and ok;
    if (info.mcfg_phys != 0) ok = verifyAcpiTable("acpi-mcfg", info.mcfg_phys) and ok;
    if (info.madt_phys != 0) ok = verifyAcpiTable("acpi-madt", info.madt_phys) and ok;
    if (info.fadt_phys != 0) ok = verifyAcpiTable("acpi-fadt", info.fadt_phys) and ok;
    if (info.hpet_phys != 0) ok = verifyAcpiTable("acpi-hpet", info.hpet_phys) and ok;
    const dsdt = if (info.fadt_x_dsdt_phys != 0) info.fadt_x_dsdt_phys else info.fadt_dsdt_phys;
    if (dsdt != 0) ok = verifyAcpiTable("acpi-dsdt", dsdt) and ok;

    if (ok) stats_value.device_probe_bits |= DEVICE_ACPI;
    return ok;
}

fn mapPlatformMmio(info: acpi.Info) bool {
    var ok = true;
    if (info.madt_lapic_address != 0) {
        ok = mapMmioWindow("lapic-mmio", info.madt_lapic_address, layout.PAGE_SIZE, DEVICE_LAPIC) and ok;
    }

    const ioapic_count = minU32(info.madt_ioapic_count, acpi.MAX_MADT_IOAPICS);
    var i: usize = 0;
    while (i < ioapic_count) : (i += 1) {
        const entry = info.madt_ioapics[i];
        if (entry.address == 0) continue;
        ok = mapMmioWindow(if (i == 0) "ioapic-mmio" else "ioapic-extra-mmio", entry.address, layout.PAGE_SIZE, DEVICE_IOAPIC) and ok;
    }
    if (ioapic_count == 0 and info.madt_first_ioapic_address != 0) {
        ok = mapMmioWindow("ioapic-mmio", info.madt_first_ioapic_address, layout.PAGE_SIZE, DEVICE_IOAPIC) and ok;
    }

    if (info.hpet_base != 0) {
        ok = mapMmioWindow("hpet-mmio", info.hpet_base, layout.PAGE_SIZE, DEVICE_HPET) and ok;
    }
    if (info.fadt_reset_supported and info.fadt_reset_gas_valid and info.fadt_reset_address_space == 0 and info.fadt_reset_address != 0) {
        ok = mapMmioWindow("acpi-reset-mmio", info.fadt_reset_address, 1, DEVICE_ACPI_RESET) and ok;
    }
    return ok;
}

fn mapPcieEcam(info: acpi.Info) bool {
    if (info.mcfg_base == 0) return true;
    if (info.mcfg_start_bus > info.mcfg_end_bus) return true;
    const bus_count = @as(u64, info.mcfg_end_bus) - @as(u64, info.mcfg_start_bus) + 1;
    const len = checkedMul(bus_count, layout.MiB) orelse return false;
    return mapMmioWindow("pcie-ecam", info.mcfg_base, len, DEVICE_ECAM);
}

fn mapMmioWindow(name: []const u8, phys_base: u64, len: u64, bit: u32) bool {
    if (phys_base == 0 or len == 0) return true;
    const base = alignDown(phys_base, layout.PAGE_SIZE);
    const raw_end = checkedAdd(phys_base, len) orelse return false;
    const end = alignUpChecked(raw_end, layout.PAGE_SIZE) orelse return false;
    if (end <= base) return false;
    const map_len = end - base;
    const cursor = checkedAdd(layout.MMIO_BASE, next_mmio_offset) orelse return false;
    // Preserve the physical 2-MiB offset in the dedicated MMIO window. Large
    // ECAM apertures can then use huge leaves instead of consuming one page
    // table per 2 MiB; the HHDM UC conversion may independently need those
    // table frames to split retained direct-map leaves.
    const desired_offset = base & (HUGE_2M_SIZE - 1);
    const cursor_offset = cursor & (HUGE_2M_SIZE - 1);
    const padding = (desired_offset + HUGE_2M_SIZE - cursor_offset) & (HUGE_2M_SIZE - 1);
    const virt = checkedAdd(cursor, padding) orelse return false;
    const virt_end = checkedAdd(virt, map_len) orelse return false;
    if (virt < layout.MMIO_BASE or virt_end > layout.MMIO_END) return false;

    var mapped: RangeMapStats = .{};
    if (!kernel_space_builder.mapRangeAuto(virt, base, map_len, MapFlags.mmio, &mapped)) return false;
    if (!ensureDirectMmioRange(base, map_len)) return false;
    const consumed = checkedAdd(padding, map_len) orelse return false;
    next_mmio_offset = checkedAdd(next_mmio_offset, consumed) orelse return false;
    stats_value.mmio_bytes += map_len;
    stats_value.device_probe_bits |= bit;
    recordMapping(name, base, virt, map_len, MapFlags.mmio, .mmio_uncached, true);
    return true;
}

fn verifyAcpiTable(name: []const u8, phys_base: u64) bool {
    if (phys_base == 0) return true;
    return verifyAcpiRange(name, phys_base, acpiTableLength(phys_base));
}

fn verifyAcpiRange(name: []const u8, phys_base: u64, len: u64) bool {
    if (phys_base == 0 or len == 0) return true;
    const base = alignDown(phys_base, layout.PAGE_SIZE);
    const raw_end = checkedAdd(phys_base, len) orelse return false;
    const end = alignUpChecked(raw_end, layout.PAGE_SIZE) orelse return false;
    if (end <= base) return false;

    var page = base;
    while (page < end) : (page += layout.PAGE_SIZE) {
        const virt = phys.physToVirt(page);
        const walked = kernel_space_builder.walk(virt);
        if (!walked.mapped or alignDown(walked.phys, layout.PAGE_SIZE) != page) return false;
    }

    const map_len = end - base;
    stats_value.acpi_bytes += map_len;
    recordMapping(name, base, phys.physToVirt(base), map_len, MapFlags.kernel_ro, .acpi_firmware, true);
    return true;
}

fn acpiTableLength(phys_base: u64) u64 {
    if (phys_base == 0) return layout.PAGE_SIZE;
    const virt = phys.physToVirt(alignDown(phys_base, layout.PAGE_SIZE));
    if (virt == 0) return layout.PAGE_SIZE;
    const header: *align(1) const AcpiSdtHeader = @ptrFromInt(phys.physToVirt(phys_base));
    const len = header.length;
    if (len < @sizeOf(AcpiSdtHeader) or len > layout.MiB) return layout.PAGE_SIZE;
    return len;
}

fn recordMapping(name: []const u8, phys_base: u64, virt_base: u64, len: u64, flags: u64, policy: layout.CachePolicy, mapped: bool) void {
    if (device_mapping_count >= device_mappings.len) {
        stats_value.device_mapping_truncated = true;
        return;
    }
    device_mappings[device_mapping_count] = .{
        .name = name,
        .phys_base = phys_base,
        .virt_base = virt_base,
        .len = len,
        .flags = flags,
        .cache_policy = policy,
        .mapped = mapped,
    };
    device_mapping_count += 1;
}

fn mapKernelImageFromActive(builder: *Builder, source_root: u64) bool {
    const span = kernelImageSpanLen();
    var mapped: RangeMapStats = .{};
    if (!mapExistingRangeSparse(builder, source_root, layout.KERNEL_IMAGE_BASE, span, &mapped)) return false;
    stats_value.kernel_image_pages += mapped.pages_4k;
    stats_value.kernel_image_skipped += mapped.skipped;
    return mapped.pages_4k != 0;
}

fn kernelImageSpanLen() u64 {
    const entries = boot_info.memoryMap();
    var min_base: u64 = ~@as(u64, 0);
    var max_end: u64 = 0;

    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        if (!entry.valid or entry.kind != .kernel_and_modules) continue;
        if (entry.base < min_base) min_base = entry.base;
        if (entry.end > max_end) max_end = entry.end;
    }

    if (max_end <= min_base) return 16 * layout.MiB;
    const raw_len = max_end - min_base;
    const capped = if (raw_len > layout.KERNEL_IMAGE_WINDOW_BYTES) layout.KERNEL_IMAGE_WINDOW_BYTES else raw_len;
    return alignUpChecked(capped, layout.PAGE_SIZE) orelse (16 * layout.MiB);
}

fn mapBootSanityProbes(builder: *Builder, source_root: u64) bool {
    var stack_probe: u64 = 0;
    const rodata = "r4os-pml4-rodata-probe";
    var mapped: RangeMapStats = .{};

    if (!mapProbePage(builder, source_root, layout.KERNEL_IMAGE_BASE, PROBE_CODE, &mapped)) return false;
    if (!mapProbePage(builder, source_root, @intFromPtr(rodata.ptr), PROBE_RODATA, &mapped)) return false;
    if (!mapProbePage(builder, source_root, @intFromPtr(&stats_value), PROBE_DATA, &mapped)) return false;
    if (!mapProbePage(builder, source_root, @intFromPtr(&kernel_space_bss_probe), PROBE_BSS, &mapped)) return false;
    if (!mapProbePage(builder, source_root, @intFromPtr(&stack_probe), PROBE_STACK, &mapped)) return false;
    if (!mapStackWindow(builder, source_root, @intFromPtr(&stack_probe), &mapped)) return false;
    if (!mapBootInfoProbes(builder, source_root, &mapped)) return false;
    if (!mapHeapMetadataProbe(builder, source_root, &mapped)) return false;
    if (!verifyHhdmAndPmm(builder)) return false;

    stats_value.boot_probe_pages += mapped.pages_4k;
    stats_value.boot_probe_skipped += mapped.skipped;
    return true;
}

fn mapStackWindow(builder: *Builder, source_root: u64, stack_addr: u64, mapped: *RangeMapStats) bool {
    const stack_page = alignDown(stack_addr, layout.PAGE_SIZE);
    const half = STACK_WINDOW_BYTES / 2;
    const base = if (stack_page > half) stack_page - half else stack_page;
    const before = mapped.pages_4k;
    if (!mapExistingRangeSparse(builder, source_root, base, STACK_WINDOW_BYTES, mapped)) return false;
    return mapped.pages_4k > before;
}

fn mapRuntimeKernelStateFromActive(source_root: u64) bool {
    if (source_root == 0) return false;
    const heap_stats = heap.stats();
    if (heap_stats.committed_bytes == 0) return false;

    var mapped: RangeMapStats = .{};
    if (!mapExistingRangeSparse(&kernel_space_builder, source_root, heap_stats.base, @intCast(heap_stats.committed_bytes), &mapped)) return false;
    stats_value.heap_committed_bytes = @intCast(heap_stats.committed_bytes);
    stats_value.heap_committed_pages = mapped.pages_4k;
    stats_value.heap_committed_skipped = mapped.skipped;

    var stack_probe: u64 = 0;
    var stack_mapped: RangeMapStats = .{};
    if (!mapStackWindow(&kernel_space_builder, source_root, @intFromPtr(&stack_probe), &stack_mapped)) return false;
    stats_value.boot_probe_pages += stack_mapped.pages_4k;
    stats_value.boot_probe_skipped += stack_mapped.skipped;
    return mapped.pages_4k != 0;
}

fn mapBootInfoProbes(builder: *Builder, source_root: u64, mapped: *RangeMapStats) bool {
    if (!mapProbePage(builder, source_root, @intFromPtr(boot_info.get()), PROBE_BOOTINFO, mapped)) return false;
    const mem = boot_info.memoryMap();
    if (mem.len != 0) {
        if (!mapExistingRangeSparse(builder, source_root, @intFromPtr(mem.ptr), @sizeOf(boot_info.MemoryMapEntry) * mem.len, mapped)) return false;
    }
    if (boot_info.framebuffer()) |fb| {
        if (!mapProbePage(builder, source_root, @intFromPtr(fb), PROBE_BOOTINFO, mapped)) return false;
    }
    return true;
}

fn mapHeapMetadataProbe(builder: *Builder, source_root: u64, mapped: *RangeMapStats) bool {
    const range = heap.metadataRange();
    if (range.len == 0) return false;
    if (!mapExistingRangeSparse(builder, source_root, range.base, range.len, mapped)) return false;
    const walked = builder.walk(range.base);
    if (!walked.mapped) return false;
    stats_value.probe_bits |= PROBE_HEAPMETA;
    return true;
}

fn verifyHhdmAndPmm(builder: *Builder) bool {
    const entries = boot_info.memoryMap();
    var hhdm_checked = false;
    var i: usize = 0;
    while (i < entries.len and !hhdm_checked) : (i += 1) {
        const entry = entries[i];
        if (!entry.valid or entry.length == 0) continue;
        if (boot_info.physToHhdm(entry.base)) |virt| {
            const walked = builder.walk(virt);
            if (walked.mapped and alignDown(walked.phys, layout.PAGE_SIZE) == alignDown(entry.base, layout.PAGE_SIZE)) {
                stats_value.probe_bits |= PROBE_HHDM;
                hhdm_checked = true;
            }
        }
    }

    const pmm = phys.stats();
    if (pmm.bitmap_base != 0 and pmm.bitmap_bytes != 0) {
        const pmm_virt = phys.physToVirt(pmm.bitmap_base);
        const walked = builder.walk(pmm_virt);
        if (walked.mapped and alignDown(walked.phys, layout.PAGE_SIZE) == alignDown(pmm.bitmap_base, layout.PAGE_SIZE)) {
            stats_value.probe_bits |= PROBE_PMM;
        }
    }

    return (stats_value.probe_bits & PROBE_HHDM) != 0 and (stats_value.probe_bits & PROBE_PMM) != 0;
}

fn mapProbePage(builder: *Builder, source_root: u64, virt: u64, bit: u32, mapped: *RangeMapStats) bool {
    const page = alignDown(virt, layout.PAGE_SIZE);
    stats_value.debug_probe_bit = bit;
    stats_value.debug_probe_virt = page;
    stats_value.debug_probe_step = 1;
    stats_value.debug_probe_source_phys = 0;
    stats_value.debug_probe_existing_phys = 0;
    const source = walkFromRoot(source_root, page);
    if (!source.mapped) return false;
    stats_value.debug_probe_source_phys = alignDown(source.phys, layout.PAGE_SIZE);
    const existing = builder.walk(page);
    if (existing.mapped) stats_value.debug_probe_existing_phys = alignDown(existing.phys, layout.PAGE_SIZE);
    if (!builder.ensurePage(page, stats_value.debug_probe_source_phys, source.flags)) {
        stats_value.debug_probe_step = 2;
        return false;
    }
    mapped.pages_4k += 1;
    const walked = builder.walk(page);
    if (!walked.mapped) {
        stats_value.debug_probe_step = 3;
        return false;
    }
    stats_value.probe_bits |= bit;
    stats_value.debug_probe_step = 0;
    return true;
}

fn mapExistingRangeSparse(builder: *Builder, source_root: u64, virt_base: u64, byte_len: u64, out: *RangeMapStats) bool {
    if (byte_len == 0) return false;
    const start = alignDown(virt_base, layout.PAGE_SIZE);
    const raw_end = checkedAdd(virt_base, byte_len) orelse return false;
    const end = alignUpChecked(raw_end, layout.PAGE_SIZE) orelse return false;
    var virt = start;
    while (virt < end) : (virt += layout.PAGE_SIZE) {
        if (!mapExistingPageFrom(builder, source_root, virt, out)) out.skipped += 1;
    }
    return true;
}

fn mapExistingPageFrom(builder: *Builder, source_root: u64, virt_page: u64, out: *RangeMapStats) bool {
    const source = walkFromRoot(source_root, virt_page);
    if (!source.mapped) return false;
    const frame = alignDown(source.phys, layout.PAGE_SIZE);
    if (!builder.ensurePage(virt_page, frame, source.flags)) return false;
    out.pages_4k += 1;
    return true;
}

fn ensureDirectMmioRange(phys_base: u64, byte_len: u64) bool {
    if (phys_base == 0 or byte_len == 0) return true;
    const base = alignDown(phys_base, layout.PAGE_SIZE);
    const raw_end = checkedAdd(phys_base, byte_len) orelse return false;
    const end = alignUpChecked(raw_end, layout.PAGE_SIZE) orelse return false;
    var offset: u64 = 0;
    while (base + offset < end) {
        const frame = base + offset;
        const virt = phys.physToVirt(frame);
        const existing = kernel_space_builder.walk(virt);
        if (existing.mapped) {
            if (alignDown(existing.phys, layout.PAGE_SIZE) != frame) return false;
            // Never retain a WB/global direct-map alias for device MMIO. If
            // the HHDM used a 2-MiB leaf, ensurePageWithFlags splits it first
            // and rewrites this exact 4-KiB page as uncached.
            if (!kernel_space_builder.ensurePageWithFlags(virt, frame, MapFlags.mmio)) return false;
            offset += layout.PAGE_SIZE;
            continue;
        }

        const remaining = end - frame;
        if (remaining >= HUGE_2M_SIZE and isAlignedTo(virt, HUGE_2M_SIZE) and isAlignedTo(frame, HUGE_2M_SIZE)) {
            if (!kernel_space_builder.mapHuge2M(virt, frame, MapFlags.mmio)) return false;
            offset += HUGE_2M_SIZE;
        } else {
            if (!kernel_space_builder.mapPage(virt, frame, MapFlags.mmio)) return false;
            offset += layout.PAGE_SIZE;
        }
    }
    return true;
}

fn validPreparedRoot(root_phys: u64) bool {
    if (root_phys == 0 or !isAligned(root_phys)) return false;
    if (root_phys != kernel_space_builder.pml4_phys) return false;
    if (tableFromPhys(root_phys) == null) return false;
    return pageTableBlockContains(root_phys);
}

fn pageTableBlockContains(root_phys: u64) bool {
    const summary = blocks.summary();
    var i: u32 = 0;
    while (i < summary.active_blocks) : (i += 1) {
        const block = blocks.activeAt(i) orelse return false;
        if (block.kind != .page_table or block.phys_len == 0) continue;
        const end = checkedAdd(block.phys_base, block.phys_len) orelse continue;
        const root_end = checkedAdd(root_phys, layout.PAGE_SIZE) orelse return false;
        if (root_phys >= block.phys_base and root_end <= end) return true;
    }
    return false;
}

fn preSwitchChecks(root_phys: u64) bool {
    var bits: u32 = 0;
    if (checkVirtMapped(root_phys, layout.KERNEL_IMAGE_BASE)) bits |= POST_CODE;
    var stack_probe: u64 = 0;
    if (checkVirtMapped(root_phys, @intFromPtr(&stack_probe))) bits |= POST_STACK;
    if (checkHhdmMapped(root_phys)) bits |= POST_HHDM;
    if (checkPmmMapped(root_phys)) bits |= POST_PMM;
    if (checkPageTableAccess(root_phys)) bits |= POST_PAGE_TABLE;
    if (checkBootInfoMapped(root_phys)) bits |= POST_BOOTINFO;
    if (checkFramebufferMapped(root_phys)) bits |= POST_FRAMEBUFFER;
    if (checkHeapMapped(root_phys)) bits |= POST_HEAP;
    stats_value.post_switch_bits = bits;
    return (bits & REQUIRED_POST_SWITCH) == REQUIRED_POST_SWITCH;
}

fn postSwitchChecks(root_phys: u64) bool {
    var bits: u32 = 0;
    if (checkVirtMapped(root_phys, layout.KERNEL_IMAGE_BASE)) bits |= POST_CODE;
    var stack_probe: u64 = 0;
    if (checkVirtMapped(root_phys, @intFromPtr(&stack_probe))) bits |= POST_STACK;
    if (checkHhdmMapped(root_phys)) bits |= POST_HHDM;
    if (checkPmmMapped(root_phys)) bits |= POST_PMM;
    if (checkPageTableAccess(root_phys)) bits |= POST_PAGE_TABLE;
    if (checkBootInfoMapped(root_phys)) bits |= POST_BOOTINFO;
    if (checkFramebufferMapped(root_phys)) bits |= POST_FRAMEBUFFER;
    if (checkHeapMapped(root_phys)) bits |= POST_HEAP;
    cr3_switch_guard +%= 1;
    if (!checkVirtMapped(root_phys, @intFromPtr(&cr3_switch_guard))) bits &= ~POST_CODE;
    stats_value.post_switch_bits = bits;
    return (bits & REQUIRED_POST_SWITCH) == REQUIRED_POST_SWITCH;
}

fn checkVirtMapped(root_phys: u64, virt: u64) bool {
    const walked = walkFromRoot(root_phys, alignDown(virt, layout.PAGE_SIZE));
    return walked.mapped;
}

fn checkVirtToPhys(root_phys: u64, virt: u64, phys_addr: u64) bool {
    const walked = walkFromRoot(root_phys, alignDown(virt, layout.PAGE_SIZE));
    if (!walked.mapped) return false;
    return alignDown(walked.phys, layout.PAGE_SIZE) == alignDown(phys_addr, layout.PAGE_SIZE);
}

fn checkHhdmMapped(root_phys: u64) bool {
    const entries = boot_info.memoryMap();
    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        if (!entry.valid or entry.length == 0) continue;
        if (boot_info.physToHhdm(entry.base)) |virt| {
            if (checkVirtToPhys(root_phys, virt, entry.base)) return true;
        }
    }
    return false;
}

fn checkPmmMapped(root_phys: u64) bool {
    const pmm = phys.stats();
    if (pmm.bitmap_base == 0 or pmm.bitmap_bytes == 0) return false;
    return checkVirtToPhys(root_phys, phys.physToVirt(pmm.bitmap_base), pmm.bitmap_base);
}

fn checkPageTableAccess(root_phys: u64) bool {
    return checkVirtToPhys(root_phys, phys.physToVirt(root_phys), root_phys);
}

fn checkBootInfoMapped(root_phys: u64) bool {
    if (!checkVirtMapped(root_phys, @intFromPtr(boot_info.get()))) return false;
    const mem = boot_info.memoryMap();
    if (mem.len != 0 and !checkVirtMapped(root_phys, @intFromPtr(mem.ptr))) return false;
    if (boot_info.framebuffer()) |fb| {
        if (!checkVirtMapped(root_phys, @intFromPtr(fb))) return false;
    }
    return true;
}

fn checkFramebufferMapped(root_phys: u64) bool {
    const fb = boot_info.framebuffer() orelse return false;
    const source_virt: u64 = @intFromPtr(fb.address);
    const source = walkFromRoot(root_phys, source_virt);
    if (!source.mapped) return false;
    const window = walkFromRoot(root_phys, layout.FRAMEBUFFER_BASE);
    return window.mapped;
}

fn checkHeapMapped(root_phys: u64) bool {
    const heap_stats = heap.stats();
    if (heap_stats.committed_bytes == 0) return false;
    if (!checkVirtMapped(root_phys, heap_stats.base)) return false;
    const last = checkedAdd(heap_stats.base, @intCast(heap_stats.committed_bytes - 1)) orelse return false;
    return checkVirtMapped(root_phys, last);
}

fn inspectLiminePageTables(old_root: u64, active_root: u64) void {
    stats_value.limine_pt_scan_runs += 1;
    resetLiminePageTableStats();

    if (old_root == 0 or active_root == 0 or !isAligned(old_root) or !isAligned(active_root)) {
        stats_value.limine_pt_scan_failures += 1;
        return;
    }

    var old_count: usize = 0;
    var active_count: usize = 0;
    collectTableFrames(old_root, 4, limine_table_frames[0..], &old_count, &stats_value.limine_old_scan_truncated);
    collectTableFrames(active_root, 4, active_table_frames[0..], &active_count, &stats_value.limine_active_scan_truncated);

    stats_value.limine_old_table_frames = old_count;
    stats_value.limine_active_table_frames = active_count;
    stats_value.limine_old_root_active = old_root == active_root;

    var i: usize = 0;
    while (i < old_count) : (i += 1) {
        if (containsFrame(active_table_frames[0..active_count], limine_table_frames[i])) {
            stats_value.limine_referenced_frames += 1;
        }
    }

    stats_value.limine_release_safe =
        !stats_value.limine_old_root_active and
        !stats_value.limine_old_scan_truncated and
        !stats_value.limine_active_scan_truncated and
        stats_value.limine_referenced_frames == 0 and
        old_count != 0;

    if (!stats_value.limine_release_safe) {
        stats_value.limine_retained_frames = old_count;
        return;
    }

    stats_value.limine_release_candidates = old_count;
    i = 0;
    while (i < old_count) : (i += 1) {
        if (quarantineLiminePageTableFrame(limine_table_frames[i])) {
            stats_value.limine_quarantined_frames += 1;
        } else {
            stats_value.limine_quarantine_failures += 1;
        }
    }

    if (stats_value.limine_quarantine_failures != 0) {
        stats_value.limine_retained_frames = stats_value.limine_quarantine_failures;
    }
}

fn resetLiminePageTableStats() void {
    @memset(limine_table_frames[0..], 0);
    @memset(active_table_frames[0..], 0);
    stats_value.limine_old_table_frames = 0;
    stats_value.limine_active_table_frames = 0;
    stats_value.limine_referenced_frames = 0;
    stats_value.limine_release_candidates = 0;
    stats_value.limine_quarantined_frames = 0;
    stats_value.limine_released_frames = 0;
    stats_value.limine_retained_frames = 0;
    stats_value.limine_quarantine_failures = 0;
    stats_value.limine_old_scan_truncated = false;
    stats_value.limine_active_scan_truncated = false;
    stats_value.limine_old_root_active = false;
    stats_value.limine_release_safe = false;
}

fn collectTableFrames(root_phys: u64, level: u8, out: []u64, count: *usize, truncated: *bool) void {
    if (root_phys == 0 or !isAligned(root_phys)) {
        stats_value.limine_pt_scan_failures += 1;
        return;
    }
    if (containsFrame(out[0..count.*], root_phys)) return;
    if (count.* >= out.len) {
        truncated.* = true;
        return;
    }

    out[count.*] = root_phys;
    count.* += 1;
    if (level <= 1) return;

    const table = tableFromPhys(root_phys) orelse {
        stats_value.limine_pt_scan_failures += 1;
        return;
    };

    var i: usize = 0;
    while (i < ENTRIES) : (i += 1) {
        const entry = table[i];
        if ((entry & layout.PageFlags.present) == 0) continue;
        if ((entry & HUGE_PAGE) != 0) continue;
        const next = entry & ADDR_MASK;
        if (next == 0) continue;
        collectTableFrames(next, level - 1, out, count, truncated);
    }
}

fn quarantineLiminePageTableFrame(frame: u64) bool {
    if (frame == 0 or !isAligned(frame)) return false;
    _ = blocks.retagPhysicalRange(
        frame,
        layout.PAGE_SIZE,
        .page_table,
        .bootloader,
        0,
        .reserved,
        "limine-page-table",
    ) catch return false;
    return true;
}

fn containsFrame(frames: []const u64, frame: u64) bool {
    var i: usize = 0;
    while (i < frames.len) : (i += 1) {
        if (frames[i] == frame) return true;
    }
    return false;
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

fn allocFrame(kind: blocks.Kind, name: []const u8) ?u64 {
    const frame = phys.allocFrame() orelse return null;
    _ = blocks.claimPhysicalRange(frame, layout.PAGE_SIZE, kind, .kernel, 0, name) catch {
        phys.freeFrame(frame);
        return null;
    };
    return frame;
}

fn releaseFrame(frame: u64) void {
    if (frame == 0) return;
    var release_plan: blocks.PhysicalReleasePlan = undefined;
    blocks.preparePhysicalRangeRelease(frame, layout.PAGE_SIZE, &release_plan) catch return;
    defer blocks.cancelPhysicalRangeRelease(&release_plan);
    phys.freeFrame(frame);
    blocks.commitPhysicalRangeRelease(&release_plan);
}

fn tableFromPhys(addr: u64) ?*PageTable {
    if (!isAligned(addr)) return null;
    const virt = phys.physToVirt(addr);
    if (virt == 0) return null;
    return @ptrFromInt(virt);
}

fn sanitizeLeafFlags(flags: u64) u64 {
    return flags & ~ADDR_MASK & ~HUGE_PAGE;
}

fn index(virt: u64, comptime shift: u6) usize {
    return @intCast((virt >> shift) & 0x1ff);
}

fn isAligned(addr: u64) bool {
    return (addr & (layout.PAGE_SIZE - 1)) == 0;
}

fn isAlignedTo(addr: u64, alignment: u64) bool {
    return (addr & (alignment - 1)) == 0;
}

fn alignDown(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn alignUpChecked(value: u64, alignment: u64) ?u64 {
    const adjusted = checkedAdd(value, alignment - 1) orelse return null;
    return adjusted & ~(alignment - 1);
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    const result = a +% b;
    if (result < a) return null;
    return result;
}

fn checkedMul(a: u64, b: u64) ?u64 {
    if (a != 0 and b > ~@as(u64, 0) / a) return null;
    return a * b;
}

fn minU32(value: u32, comptime limit: usize) usize {
    return if (value < limit) @intCast(value) else limit;
}
