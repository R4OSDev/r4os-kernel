// Rust owns the memory boot order, cached summary and completion flags.
// These cuts retain the existing display/VM/SMP and diagnostic owners.
const display = @import("../display/display.zig");
const boot_display = @import("../display/boot_display.zig");
const blocks = @import("../memory/blocks.zig");
const heap = @import("../memory/heap.zig");
const mem_map = @import("../memory/map.zig");
const page_tables = @import("../memory/page_tables.zig");
const paging = @import("../memory/paging.zig");
const phys = @import("../memory/phys.zig");
const virt = @import("../memory/virt.zig");
const boot_status = @import("boot_status.zig");
const smp = @import("smp.zig");
const Snapshot = extern struct { flags: u32, usable: u64, largest_base: u64, largest_len: u64 };
extern fn r4os_rust_memory_core() bool;
extern fn r4os_rust_memory_finalize(input: *const page_tables.RootAcpiInput) bool;
extern fn r4os_rust_memory_snapshot(output: *Snapshot) void;
export fn r4os_zig_memory_step(operation: u8, argument: u64) callconv(.c) bool {
    if (@import("builtin").is_test) @panic("memory boot operation requested by host fixture");
    switch (operation) {
        0 => mem_map.dump(),
        1 => blocks.dumpSummary(),
        2 => smp.reserveLowMemory(),
        3 => phys.dumpStats(),
        4 => paging.dumpStats(),
        5 => page_tables.dumpStats(),
        6 => return virt.init(),
        7 => virt.dumpStats(),
        8 => return display.enableFramebufferWriteCombining(),
        9 => heap.dumpStats(),
        10 => return heap.selfTest(),
        11 => return virt.metadataInvariant(),
        12 => return boot_display.prepareConsolePixelBacking(argument),
        13 => boot_status.statusLine("  Memory [OK]\r\n"),
        14 => page_tables.dumpDeviceMappings(),
        15 => page_tables.dumpCr3SwitchStatus(),
        else => return false,
    }
    return true;
}
fn snapshot() Snapshot {
    var result: Snapshot = undefined;
    r4os_rust_memory_snapshot(&result);
    return result;
}
pub fn initCore() bool {
    return r4os_rust_memory_core();
}
pub fn isCoreInitialized() bool {
    return snapshot().flags & 1 != 0;
}
pub fn hasSummary() bool {
    return snapshot().flags & 2 != 0;
}
pub fn usableBytes() u64 {
    return snapshot().usable;
}
pub fn largestUsableBase() u64 {
    return snapshot().largest_base;
}
pub fn largestUsableLen() u64 {
    return snapshot().largest_len;
}
pub fn dumpBlockSummary() void {
    blocks.dumpSummary();
}
pub fn finalizeDeviceMappings(acpi_info: anytype) bool {
    const input = page_tables.acpiInput(acpi_info);
    return r4os_rust_memory_finalize(&input);
}
pub fn deviceMappingsFinalized() bool {
    return snapshot().flags & 4 != 0;
}
pub fn cr3SwitchDone() bool {
    return snapshot().flags & 8 != 0;
}
comptime {
    if (@sizeOf(Snapshot) != 32) @compileError("Rust memory boot Snapshot C layout mismatch");
}
