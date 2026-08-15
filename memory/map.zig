const boot_info = @import("../bootloader/boot_info.zig");
const k = @import("../kernel/log.zig");

pub const Summary = struct {
    entries: u64 = 0,
    invalid_entries: u64 = 0,
    usable_bytes: u64 = 0,
    usable_page_bytes: u64 = 0,
    largest_usable_base: u64 = 0,
    largest_usable_len: u64 = 0,
    overflow: bool = false,
};

pub fn summarize() Summary {
    const entries = boot_info.memoryMap();
    var summary: Summary = .{ .entries = entries.len };

    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        if (!entry.valid) {
            summary.invalid_entries += 1;
            continue;
        }
        if (entry.kind == .usable) {
            checkedAddInto(&summary.usable_bytes, entry.length, &summary.overflow);
            checkedAddInto(&summary.usable_page_bytes, entry.usable_len, &summary.overflow);
            if (entry.usable_len > summary.largest_usable_len) {
                summary.largest_usable_base = entry.usable_base;
                summary.largest_usable_len = entry.usable_len;
            }
        }
    }

    return summary;
}

pub fn dump() void {
    const entries = boot_info.memoryMap();
    if (entries.len == 0) {
        k.puts("  Memory map: unavailable\r\n");
        return;
    }

    k.puts("  Memory map entries: ");
    k.putDec(entries.len);
    const info = boot_info.get();
    if (info.memory_map_truncated) {
        k.puts(" truncated=yes");
    }
    if (info.memory_map_invalid_entries != 0) {
        k.puts(" invalid=");
        k.putDec(info.memory_map_invalid_entries);
    }
    k.puts("\r\n");

    var i: usize = 0;
    while (i < entries.len) : (i += 1) {
        const entry = entries[i];
        k.puts("    [");
        k.putDec(i);
        k.puts("] ");
        k.puts(kindName(entry.kind));
        k.puts(" base=0x");
        k.putHex(entry.base, 16);
        k.puts(" len=0x");
        k.putHex(entry.length, 16);
        if (entry.kind == .usable) {
            k.puts(" usable-pages=0x");
            k.putHex(entry.usable_base, 16);
            k.puts("+0x");
            k.putHex(entry.usable_len, 16);
        }
        k.puts("\r\n");
    }

    const summary = summarize();
    k.puts("  Usable page-aligned bytes: ");
    k.putDec(summary.usable_page_bytes);
    k.puts(" overflow=");
    k.puts(if (summary.overflow) "yes" else "no");
    k.puts("\r\n");
}

pub fn kindName(kind: boot_info.MemoryKind) []const u8 {
    return boot_info.memoryKindName(kind);
}

fn checkedAdd(a: u64, b: u64) ?u64 {
    const result = a +% b;
    if (result < a) return null;
    return result;
}

fn checkedAddInto(target: *u64, value: u64, overflow: *bool) void {
    if (checkedAdd(target.*, value)) |next| {
        target.* = next;
    } else {
        overflow.* = true;
        target.* = ~@as(u64, 0);
    }
}
