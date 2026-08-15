const blocks = @import("blocks.zig");
const layout = @import("layout.zig");
const phys = @import("phys.zig");
const k = @import("../kernel/log.zig");

const ENTRIES: usize = 512;
const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

pub const PAGE_SIZE: u64 = layout.PAGE_SIZE;
pub const PRESENT: u64 = layout.PageFlags.present;
pub const WRITABLE: u64 = layout.PageFlags.writable;
pub const USER: u64 = layout.PageFlags.user;
pub const WRITE_THROUGH: u64 = layout.PageFlags.write_through;
pub const CACHE_DISABLE: u64 = layout.PageFlags.cache_disable;
pub const ACCESSED: u64 = layout.PageFlags.accessed;
pub const DIRTY: u64 = layout.PageFlags.dirty;
pub const PAGE_ATTRIBUTE_TABLE: u64 = layout.PageFlags.page_attribute_table;
pub const GLOBAL: u64 = layout.PageFlags.global;
pub const NO_EXECUTE: u64 = layout.PageFlags.no_execute;

const PageTable = [ENTRIES]u64;
const HUGE_PAGE: u64 = 1 << 7;
const HUGE_PAGE_PAT: u64 = 1 << 12;

extern fn r4os_read_cr3() callconv(.c) u64;
extern fn r4os_invlpg(addr: usize) callconv(.c) void;

pub const RootOwner = enum(u8) {
    unknown,
    bootloader,
    r4os,
};

pub const Stats = struct {
    initialized: bool = false,
    root_owner: RootOwner = .unknown,
    active_root_phys: u64 = 0,
    hardware_cr3: u64 = 0,
    active_root_confirmed: bool = false,
    r4os_root_active: bool = false,
    init_runs: u64 = 0,
    adopt_runs: u64 = 0,
    map_pages: u64 = 0,
    unmap_pages: u64 = 0,
    write_combining_pages: u64 = 0,
    invlpg_flushes: u64 = 0,
    map_failures: u64 = 0,
    unmap_failures: u64 = 0,
    root_mismatches: u64 = 0,
};

var pml4_phys: u64 = 0;
var initialized = false;
var root_owner: RootOwner = .unknown;
var stats_value: Stats = .{};

pub fn init() bool {
    stats_value.init_runs += 1;
    pml4_phys = r4os_read_cr3() & ADDR_MASK;
    initialized = pml4_phys != 0;
    root_owner = if (initialized) .bootloader else .unknown;
    return initialized;
}

pub fn rootPhys() u64 {
    return pml4_phys;
}

pub fn adoptRootPhys(root_phys: u64, owner: RootOwner) bool {
    if (root_phys == 0 or !isAligned(root_phys)) return false;
    pml4_phys = root_phys & ADDR_MASK;
    root_owner = owner;
    initialized = true;
    stats_value.adopt_runs += 1;
    return true;
}

pub fn mapPage(virt: u64, frame_phys: u64, flags: u64) bool {
    if (!readyForPageTableMutation() or !isAligned(virt) or !isAligned(frame_phys)) return failMap();
    const pt = getOrCreateTable(virt) orelse return false;
    const pti = index(virt, 12);
    if ((pt[pti] & PRESENT) != 0) return failMap();
    pt[pti] = (frame_phys & ADDR_MASK) | flags | PRESENT;
    stats_value.map_pages += 1;
    flushPage(virt);
    return true;
}

pub fn unmapPage(virt: u64) bool {
    if (!readyForPageTableMutation() or !isAligned(virt)) return failUnmap();
    const pt = getTable(virt) orelse return failUnmap();
    const pti = index(virt, 12);
    if ((pt[pti] & PRESENT) == 0) return failUnmap();
    pt[pti] = 0;
    stats_value.unmap_pages += 1;
    flushPage(virt);
    return true;
}

pub fn isMapped(virt: u64) bool {
    if (!activeRootMatchesHardware()) return false;
    return getLeaf(virt) != null;
}

pub fn mappedFrame(virt: u64) ?u64 {
    if (!activeRootMatchesHardware() or !isAligned(virt)) return null;
    const leaf = getLeaf(virt) orelse return null;
    if (leaf.huge) return null;
    return leaf.entry.* & ADDR_MASK;
}

pub fn pageDirty(virt: u64) bool {
    if (!activeRootMatchesHardware() or !isAligned(virt)) return false;
    const leaf = getLeaf(virt) orelse return false;
    if (leaf.huge) return false;
    return (leaf.entry.* & DIRTY) != 0;
}

pub fn markDirty(virt: u64) bool {
    if (!readyForPageTableMutation() or !isAligned(virt)) return false;
    const leaf = getLeaf(virt) orelse return false;
    if (leaf.huge) return false;
    leaf.entry.* |= DIRTY | ACCESSED;
    flushPage(virt);
    return true;
}

pub fn clearDirty(virt: u64) bool {
    if (!readyForPageTableMutation() or !isAligned(virt)) return false;
    const leaf = getLeaf(virt) orelse return false;
    if (leaf.huge) return false;
    leaf.entry.* &= ~DIRTY;
    flushPage(virt);
    return true;
}

pub fn setWriteCombiningRange(virt_base: u64, byte_len: u64) bool {
    if (!readyForPageTableMutation() or byte_len == 0) return false;
    const start = alignDown(virt_base, PAGE_SIZE);
    const end = alignUp(virt_base + byte_len, PAGE_SIZE);
    var virt = start;
    while (virt < end) : (virt += PAGE_SIZE) {
        const leaf = getLeaf(virt) orelse return false;
        var entry = leaf.entry.*;
        entry &= ~CACHE_DISABLE;
        entry |= WRITE_THROUGH;
        if (leaf.huge) {
            entry |= HUGE_PAGE_PAT;
        } else {
            entry |= PAGE_ATTRIBUTE_TABLE;
        }
        leaf.entry.* = entry;
        stats_value.write_combining_pages += 1;
        flushPage(virt);
    }
    return true;
}

pub fn activeRootMatchesHardware() bool {
    if (!initialized or pml4_phys == 0) return false;
    const hardware = r4os_read_cr3() & ADDR_MASK;
    const ok = hardware == pml4_phys;
    if (!ok) stats_value.root_mismatches += 1;
    return ok;
}

pub fn activeRootIsR4os() bool {
    return root_owner == .r4os and activeRootMatchesHardware();
}

pub fn stats() Stats {
    var s = stats_value;
    s.initialized = initialized;
    s.root_owner = root_owner;
    s.active_root_phys = pml4_phys;
    s.hardware_cr3 = r4os_read_cr3() & ADDR_MASK;
    s.active_root_confirmed = initialized and pml4_phys != 0 and s.hardware_cr3 == pml4_phys;
    s.r4os_root_active = s.active_root_confirmed and root_owner == .r4os;
    return s;
}

pub fn dumpStats() void {
    dumpRuntimeStatus();
    k.puts("  Paging mutations: map=");
    k.putDec(stats_value.map_pages);
    k.puts(" unmap=");
    k.putDec(stats_value.unmap_pages);
    k.puts(" wc-pages=");
    k.putDec(stats_value.write_combining_pages);
    k.puts(" invlpg=");
    k.putDec(stats_value.invlpg_flushes);
    k.puts(" failures=");
    k.putDec(stats_value.map_failures + stats_value.unmap_failures);
    k.puts("\r\n");
    k.puts("  Paging root mismatches=");
    k.putDec(stats_value.root_mismatches);
    k.puts("\r\n");
    layout.dumpPlan();
}

pub fn dumpRuntimeStatus() void {
    const s = stats();
    k.puts("  Paging root CR3: 0x");
    k.putHex(s.active_root_phys, 16);
    k.puts(" hardware=0x");
    k.putHex(s.hardware_cr3, 16);
    k.puts(" owner=");
    k.puts(rootOwnerName(s.root_owner));
    k.puts(" active=");
    k.puts(if (s.active_root_confirmed) "yes" else "no");
    k.puts(" r4os=");
    k.puts(if (s.r4os_root_active) "yes" else "no");
    k.puts("\r\n");
    k.puts("  Paging mutations: map=");
    k.putDec(s.map_pages);
    k.puts(" unmap=");
    k.putDec(s.unmap_pages);
    k.puts(" wc-pages=");
    k.putDec(s.write_combining_pages);
    k.puts(" invlpg=");
    k.putDec(s.invlpg_flushes);
    k.puts(" map-fail=");
    k.putDec(s.map_failures);
    k.puts(" unmap-fail=");
    k.putDec(s.unmap_failures);
    k.puts(" root-mismatches=");
    k.putDec(s.root_mismatches);
    k.puts("\r\n");
}

fn getOrCreateTable(virt: u64) ?*PageTable {
    const pml4 = tableFromPhys(pml4_phys);
    const pdpt = nextTableOrCreate(pml4, index(virt, 39)) orelse return null;
    const pd = nextTableOrCreate(pdpt, index(virt, 30)) orelse return null;
    return nextTableOrCreate(pd, index(virt, 21));
}

fn getTable(virt: u64) ?*PageTable {
    const pml4 = tableFromPhys(pml4_phys);
    const pdpt = nextTable(pml4, index(virt, 39)) orelse return null;
    const pd = nextTable(pdpt, index(virt, 30)) orelse return null;
    return nextTable(pd, index(virt, 21));
}

const LeafEntry = struct {
    entry: *u64,
    huge: bool,
};

fn getLeaf(virt: u64) ?LeafEntry {
    const pml4 = tableFromPhys(pml4_phys);
    const pml4_entry = &pml4[index(virt, 39)];
    if ((pml4_entry.* & PRESENT) == 0) return null;
    const pdpt = tableFromPhys(pml4_entry.* & ADDR_MASK);

    const pdpt_entry = &pdpt[index(virt, 30)];
    if ((pdpt_entry.* & PRESENT) == 0) return null;
    if ((pdpt_entry.* & HUGE_PAGE) != 0) return .{ .entry = pdpt_entry, .huge = true };
    const pd = tableFromPhys(pdpt_entry.* & ADDR_MASK);

    const pd_entry = &pd[index(virt, 21)];
    if ((pd_entry.* & PRESENT) == 0) return null;
    if ((pd_entry.* & HUGE_PAGE) != 0) return .{ .entry = pd_entry, .huge = true };
    const pt = tableFromPhys(pd_entry.* & ADDR_MASK);

    const pt_entry = &pt[index(virt, 12)];
    if ((pt_entry.* & PRESENT) == 0) return null;
    return .{ .entry = pt_entry, .huge = false };
}

fn nextTableOrCreate(table: *PageTable, idx: usize) ?*PageTable {
    if ((table[idx] & PRESENT) == 0) {
        const frame = phys.allocFrame() orelse return null;
        _ = blocks.claimPhysicalRange(frame, PAGE_SIZE, .page_table, .kernel, 0, "page-table") catch {
            phys.freeFrame(frame);
            return null;
        };
        const next = tableFromPhys(frame);
        @memset(next, 0);
        table[idx] = (frame & ADDR_MASK) | PRESENT | WRITABLE;
    }
    return tableFromPhys(table[idx] & ADDR_MASK);
}

fn nextTable(table: *PageTable, idx: usize) ?*PageTable {
    const entry = table[idx];
    if ((entry & PRESENT) == 0) return null;
    return tableFromPhys(entry & ADDR_MASK);
}

fn tableFromPhys(addr: u64) *PageTable {
    return @ptrFromInt(phys.physToVirt(addr));
}

fn readyForPageTableMutation() bool {
    return activeRootMatchesHardware();
}

fn flushPage(virt: u64) void {
    r4os_invlpg(@intCast(virt));
    stats_value.invlpg_flushes += 1;
}

fn failMap() bool {
    stats_value.map_failures += 1;
    return false;
}

fn failUnmap() bool {
    stats_value.unmap_failures += 1;
    return false;
}

fn rootOwnerName(owner: RootOwner) []const u8 {
    return switch (owner) {
        .unknown => "unknown",
        .bootloader => "bootloader",
        .r4os => "r4os",
    };
}

fn index(virt: u64, comptime shift: u6) usize {
    return @intCast((virt >> shift) & 0x1FF);
}

fn isAligned(addr: u64) bool {
    return (addr & (PAGE_SIZE - 1)) == 0;
}

fn alignDown(value: u64, alignment: u64) u64 {
    return value & ~(alignment - 1);
}

fn alignUp(value: u64, alignment: u64) u64 {
    return alignDown(value + alignment - 1, alignment);
}
