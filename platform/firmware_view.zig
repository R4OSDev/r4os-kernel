// Borrow already permanent platform mappings. AML/EC and operation-region
// ownership live in the external driver; this primitive creates no aliases.
const std = @import("std");
const boot = @import("../bootloader/boot_info.zig");
const paging = @import("../memory/paging.zig");
const tables = @import("firmware_tables.zig");
pub fn root() u64 {
    const info = @import("acpi.zig").info();
    return if (info.rsdt_phys != 0 or info.xsdt_phys != 0) info.rsdp_phys else 0;
}
pub fn view(base: u64, bytes: u64) ?u64 {
    if (base == 0 or bytes == 0 or bytes > 16 * 1024 * 1024) return null;
    const end = std.math.add(u64, base, bytes) catch return null;
    if (end > (@as(u64, 1) << 52)) return null;
    const address = boot.physToHhdm(base) orelse return null;
    _ = std.math.add(u64, address, bytes) catch return null;
    const retained = tables.retainedBacking(base, bytes, boot.memoryMap());
    const cpu = @import("cpu.zig");
    var cursor = base & ~@as(u64, 4095);
    while (cursor < end) : (cursor += 4096) {
        const virtual = boot.physToHhdm(cursor) orelse return null;
        if (paging.physicalAddress(virtual) != cursor) return null;
        if (!retained) {
            // Only existing UC device apertures outside retained firmware
            // spans; never lend allocator-owned RAM to an operation region.
            for (boot.memoryMap()) |entry| if (entry.valid and cursor < entry.end and entry.base < cursor + 4096 and
                entry.kind != .reserved) return null;
            const selector = paging.cacheSelector(virtual) orelse return null;
            if (!cpu.patAvailable() or @as(u8, @truncate(cpu.status().pat_msr >> (@as(u6, selector) * 8))) != 0) return null;
        }
    }
    return address;
}
