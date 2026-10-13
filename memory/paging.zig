const layout = @import("layout.zig");
const owner_locks = @import("owner_locks.zig");
const tlb = @import("../arch/x86_64/tlb_shootdown.zig");
const k = @import("../kernel/log.zig");

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

pub const RootOwner = enum(u8) {
    unknown,
    bootloader,
    r4os,
};

pub const Stats = extern struct {
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
    map_batches: u64 = 0,
    map_batch_pages: u64 = 0,
    map_batch_max_pages: u64 = 0,
    map_batch_rollbacks: u64 = 0,
    unmap_batches: u64 = 0,
    unmap_batch_pages: u64 = 0,
    unmap_batch_max_pages: u64 = 0,
    shootdown_failures: u64 = 0,
    rollback_restores: u64 = 0,
    rollback_restore_failures: u64 = 0,
    root_mismatches: u64 = 0,
};

extern fn r4os_rust_paging_init() bool;
extern fn r4os_rust_paging_root() u64;
extern fn r4os_rust_paging_adopt(root: u64, owner: u8) bool;
extern fn r4os_rust_paging_map(virtual: u64, physical: u64, flags: u64, locked: bool) bool;
extern fn r4os_rust_paging_unmap(virtual: u64, locked: bool) bool;
extern fn r4os_rust_paging_map_batch(virtual: u64, physical: u64, pages: u64, flags: u64) bool;
extern fn r4os_rust_paging_unmap_batch(virtual: u64, physical: u64, pages: u64, locked: bool) bool;
extern fn r4os_rust_paging_read(virtual: u64, query: u8, locked: bool, out: *u64) bool;
extern fn r4os_rust_paging_contiguous(virtual: u64, bytes: u64, out: *u64) bool;
extern fn r4os_rust_paging_dirty(virtual: u64, clear: bool) bool;
extern fn r4os_rust_paging_write_combining(virtual: u64, bytes: u64) bool;
extern fn r4os_rust_paging_active(r4os: bool) bool;
extern fn r4os_rust_paging_stats(out: *Stats) void;

// The existing generation/online-mask/ack owner is migrated in 0.83.10.
// Rust retains no State borrow across this bounded, owner-held callback.
export fn r4os_zig_paging_shootdown(base: u64, pages: u64) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("privileged TLB callback in host owner fixture");
    return tlb.invalidateRange(base, pages).ok;
}
pub fn init() bool {
    return r4os_rust_paging_init();
}
pub fn rootPhys() u64 {
    return r4os_rust_paging_root();
}
pub fn acquireMutation() owner_locks.Token {
    return owner_locks.page_tables.acquire();
}
pub fn releaseMutation(token: owner_locks.Token) void {
    owner_locks.page_tables.release(token);
}
pub fn shootdownStats() tlb.Stats {
    return tlb.stats();
}
pub fn adoptRootPhys(root_phys: u64, owner: RootOwner) bool {
    return r4os_rust_paging_adopt(root_phys, @intFromEnum(owner));
}
pub fn mapPage(virtual: u64, physical: u64, flags: u64) bool {
    return r4os_rust_paging_map(virtual, physical, flags, false);
}
pub fn mapPageLocked(virtual: u64, physical: u64, flags: u64) bool {
    return r4os_rust_paging_map(virtual, physical, flags, true);
}
pub fn unmapPage(virtual: u64) bool {
    return r4os_rust_paging_unmap(virtual, false);
}
pub fn unmapPageLocked(virtual: u64) bool {
    return r4os_rust_paging_unmap(virtual, true);
}
pub fn mapContiguousPages(virtual: u64, physical: u64, pages: u64, flags: u64) bool {
    return r4os_rust_paging_map_batch(virtual, physical, pages, flags);
}
pub fn unmapContiguousPages(virtual: u64, physical: u64, pages: u64) bool {
    return r4os_rust_paging_unmap_batch(virtual, physical, pages, false);
}
pub fn unmapContiguousPagesLocked(virtual: u64, physical: u64, pages: u64) bool {
    return r4os_rust_paging_unmap_batch(virtual, physical, pages, true);
}
fn readValue(virtual: u64, query: u8, locked: bool) ?u64 {
    var value: u64 = undefined;
    return if (r4os_rust_paging_read(virtual, query, locked, &value)) value else null;
}
pub fn isMapped(virtual: u64) bool {
    return readValue(virtual, 3, false) != null;
}
pub fn mappedFrame(virtual: u64) ?u64 {
    return readValue(virtual, 0, false);
}
pub fn mappedFrameLocked(virtual: u64) ?u64 {
    return readValue(virtual, 0, true);
}
pub fn cacheSelector(virtual: u64) ?u3 {
    return if (readValue(virtual, 1, false)) |value| @intCast(value) else null;
}
pub fn physicalAddress(virtual: u64) ?u64 {
    return readValue(virtual, 2, false);
}
pub fn contiguousPhysicalAddress(virtual: u64, bytes: u64) ?u64 {
    var value: u64 = undefined;
    return if (r4os_rust_paging_contiguous(virtual, bytes, &value)) value else null;
}
pub fn pageDirty(virtual: u64) bool {
    return (readValue(virtual, 4, false) orelse 0) != 0;
}
pub fn markDirty(virtual: u64) bool {
    return r4os_rust_paging_dirty(virtual, false);
}
pub fn clearDirty(virtual: u64) bool {
    return r4os_rust_paging_dirty(virtual, true);
}
pub fn setWriteCombiningRange(virtual: u64, bytes: u64) bool {
    return r4os_rust_paging_write_combining(virtual, bytes);
}
pub fn activeRootMatchesHardware() bool {
    return r4os_rust_paging_active(false);
}
pub fn activeRootIsR4os() bool {
    return r4os_rust_paging_active(true);
}
pub fn stats() Stats {
    var result: Stats = undefined;
    r4os_rust_paging_stats(&result);
    return result;
}
comptime {
    if (@sizeOf(Stats) != 184 or @offsetOf(Stats, "init_runs") != 32)
        @compileError("Rust paging Stats C layout mismatch");
}

pub fn dumpStats() void {
    const lock_token = owner_locks.page_tables.acquire();
    defer owner_locks.page_tables.release(lock_token);
    const s = stats();
    dumpRuntimeStatus();
    k.puts("  Paging mutations: map=");
    k.putDec(s.map_pages);
    k.puts(" unmap=");
    k.putDec(s.unmap_pages);
    k.puts(" wc-pages=");
    k.putDec(s.write_combining_pages);
    k.puts(" invlpg=");
    k.putDec(s.invlpg_flushes);
    k.puts(" failures=");
    k.putDec(s.map_failures + s.unmap_failures);
    k.puts("\r\n");
    k.puts("  Paging batches: map=");
    k.putDec(s.map_batches);
    k.puts(" pages=");
    k.putDec(s.map_batch_pages);
    k.puts(" max=");
    k.putDec(s.map_batch_max_pages);
    k.puts(" rollback=");
    k.putDec(s.map_batch_rollbacks);
    k.puts(" unmap=");
    k.putDec(s.unmap_batches);
    k.puts(" pages=");
    k.putDec(s.unmap_batch_pages);
    k.puts(" max=");
    k.putDec(s.unmap_batch_max_pages);
    k.puts("\r\n");
    k.puts("  Paging shootdown failures=");
    k.putDec(s.shootdown_failures);
    k.puts(" restores=");
    k.putDec(s.rollback_restores);
    k.puts(" restore-failures=");
    k.putDec(s.rollback_restore_failures);
    k.puts("\r\n");
    k.puts("  Paging root mismatches=");
    k.putDec(s.root_mismatches);
    k.puts("\r\n");
    layout.dumpPlan();
}

pub fn dumpRuntimeStatus() void {
    const lock_token = owner_locks.page_tables.acquire();
    defer owner_locks.page_tables.release(lock_token);
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

fn rootOwnerName(owner: RootOwner) []const u8 {
    return switch (owner) {
        .unknown => "unknown",
        .bootloader => "bootloader",
        .r4os => "r4os",
    };
}
