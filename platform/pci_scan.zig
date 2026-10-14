// Rust owns scan/coverage/ECAM rules; the original fixtures use a C trampoline.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const bus_kind_legacy: u8 = 1;
pub const bus_kind_ecam: u8 = 2;
pub const max_devices: usize = 64;
pub const Device = extern struct {
    bus_kind: u8 = 0,
    bus: u8 = 0,
    device: u8 = 0,
    function: u8 = 0,
    vendor_id: u16 = 0,
    device_id: u16 = 0,
    class_code: u8 = 0,
    subclass: u8 = 0,
    prog_if: u8 = 0,
};
pub const Metrics = extern struct {
    found: u32 = 0,
    stored: u32 = 0,
    dropped: u32 = 0,
    truncated: bool = false,
    vendor_probes: u64 = 0,
    class_reads: u64 = 0,
    header_reads: u64 = 0,
    config_reads: u64 = 0,
    function_pages: u64 = 0,
    early_stops: u64 = 0,
};
pub const Result = extern struct { devices: [max_devices]Device = .{Device{}} ** max_devices, metrics: Metrics = .{} };
pub const Reader = struct { context: ?*anyopaque = null, read_config32: *const fn (?*anyopaque, u8, u8, u8, u16) u32 };
const CReader = extern struct { context: ?*anyopaque, read_config32: *const fn (?*anyopaque, u8, u8, u8, u16) callconv(.c) u32 };
pub const Range = extern struct { bus_kind: u8 = 0, start_bus: u8 = 0, end_bus: u8 = 0 };
pub const CoveragePlan = extern struct {
    ranges: [3]Range = .{Range{}} ** 3,
    count: u8 = 0,
    ecam_used: bool = false,
    legacy_used: bool = false,
    partial: bool = false,
};
pub const EcamWindow = extern struct { base: u64, bytes: u64 };
extern fn r4os_rust_pci_scan_plan(ready: bool, segment: u16, start: u8, end: u8, output: *CoveragePlan) void;
extern fn r4os_rust_pci_scan_range(result: *Result, range: Range, reader: CReader) bool;
extern fn r4os_rust_pci_scan_find(result: *const Result, class: u8, subclass: u8, start: u64, output: *u64) bool;
extern fn r4os_rust_pci_ecam_offset(start: u8, bus: u8, device: u8, function: u8, offset: u16, output: *u64) bool;
extern fn r4os_rust_pci_ecam_window(base: u64, start: u8, end: u8, output: *EcamWindow) bool;
pub fn planCoverage(ready: bool, segment: u16, start: u8, end: u8) CoveragePlan {
    var output: CoveragePlan = undefined;
    r4os_rust_pci_scan_plan(ready, segment, start, end, &output);
    return output;
}
export fn r4os_zig_pci_scan_read(context: ?*anyopaque, bus: u8, device: u8, function: u8, offset: u16) callconv(.c) u32 {
    const reader: *const Reader = @ptrCast(@alignCast(context.?));
    return reader.read_config32(reader.context, bus, device, function, offset);
}
pub fn scanRange(result: *Result, range: Range, reader: Reader) bool {
    const adapter = reader;
    return r4os_rust_pci_scan_range(result, range, .{ .context = @ptrCast(@constCast(&adapter)), .read_config32 = r4os_zig_pci_scan_read });
}
pub fn findByClass(result: *const Result, class_code: u8, subclass: u8, start_index: usize) ?usize {
    var output: u64 = undefined;
    return if (r4os_rust_pci_scan_find(result, class_code, subclass, start_index, &output)) @intCast(output) else null;
}
pub fn ecamOffset(start: u8, bus: u8, device: u8, function: u8, offset: u16) ?u64 {
    var output: u64 = undefined;
    return if (r4os_rust_pci_ecam_offset(start, bus, device, function, offset, &output)) output else null;
}
pub fn ecamWindow(base: u64, start: u8, end: u8) ?EcamWindow {
    var output: EcamWindow = undefined;
    return if (r4os_rust_pci_ecam_window(base, start, end, &output)) output else null;
}
comptime {
    if (@sizeOf(Device) != 12 or @sizeOf(Metrics) != 64 or @sizeOf(Result) != 832 or
        @sizeOf(Range) != 3 or @sizeOf(CoveragePlan) != 13 or @sizeOf(CReader) != 16)
        @compileError("Rust PCI scanner layout differs");
}

test "coverage plan uses one full ECAM range" {
    const std = @import("std");
    const plan = planCoverage(true, 0, 0, 255);
    try std.testing.expectEqual(@as(u8, 1), plan.count);
    try std.testing.expect(plan.ecam_used);
    try std.testing.expect(!plan.legacy_used);
    try std.testing.expect(!plan.partial);
    try std.testing.expectEqual(bus_kind_ecam, plan.ranges[0].bus_kind);
}

test "coverage plan limits legacy fallback to uncovered buses" {
    const std = @import("std");
    const plan = planCoverage(true, 0, 32, 63);
    try std.testing.expectEqual(@as(u8, 3), plan.count);
    try std.testing.expect(plan.ecam_used);
    try std.testing.expect(plan.legacy_used);
    try std.testing.expect(plan.partial);
    try std.testing.expectEqual(bus_kind_ecam, plan.ranges[0].bus_kind);
    try std.testing.expectEqual(@as(u8, 31), plan.ranges[1].end_bus);
    try std.testing.expectEqual(@as(u8, 64), plan.ranges[2].start_bus);
}

test "coverage plan rejects nonzero ECAM segment" {
    const std = @import("std");
    const plan = planCoverage(true, 1, 0, 255);
    try std.testing.expectEqual(@as(u8, 1), plan.count);
    try std.testing.expect(!plan.ecam_used);
    try std.testing.expect(plan.legacy_used);
}

test "scanner handles multifunction devices with bounded reads" {
    const std = @import("std");
    const Mock = struct {
        fn read(_: ?*anyopaque, bus: u8, device: u8, function: u8, offset: u16) u32 {
            if (bus != 0 or device != 0) return 0xFFFF_FFFF;
            if (function == 0) return switch (offset) {
                0x00 => 0x1111_1234,
                0x08 => 0x0106_0100,
                0x0C => 0x0080_0000,
                else => 0,
            };
            if (function == 1) return switch (offset) {
                0x00 => 0x2222_1234,
                0x08 => 0x0200_0000,
                else => 0,
            };
            return 0xFFFF_FFFF;
        }
    };
    var result: Result = .{};
    const complete = scanRange(&result, .{ .bus_kind = bus_kind_ecam, .start_bus = 0, .end_bus = 0 }, .{ .read_config32 = Mock.read });
    try std.testing.expect(complete);
    try std.testing.expectEqual(@as(u32, 2), result.metrics.found);
    try std.testing.expectEqual(@as(u32, 2), result.metrics.stored);
    try std.testing.expectEqual(@as(u64, 39), result.metrics.vendor_probes);
    try std.testing.expectEqual(@as(u64, 2), result.metrics.class_reads);
    try std.testing.expectEqual(@as(u64, 1), result.metrics.header_reads);
    try std.testing.expectEqual(@as(u64, 42), result.metrics.config_reads);
    const reads_before_find = result.metrics.config_reads;
    try std.testing.expectEqual(@as(?usize, 1), findByClass(&result, 0x02, 0x00, 0));
    try std.testing.expectEqual(reads_before_find, result.metrics.config_reads);
}

test "scanner stops after first device beyond retained capacity" {
    const std = @import("std");
    const Mock = struct {
        fn read(_: ?*anyopaque, _: u8, _: u8, _: u8, offset: u16) u32 {
            return switch (offset) {
                0x00 => 0x5678_1234,
                0x08 => 0x0106_0100,
                0x0C => 0,
                else => 0,
            };
        }
    };
    var result: Result = .{};
    const complete = scanRange(&result, .{ .bus_kind = bus_kind_legacy, .start_bus = 0, .end_bus = 255 }, .{ .read_config32 = Mock.read });
    try std.testing.expect(!complete);
    try std.testing.expectEqual(@as(u32, 65), result.metrics.found);
    try std.testing.expectEqual(@as(u32, 64), result.metrics.stored);
    try std.testing.expectEqual(@as(u32, 1), result.metrics.dropped);
    try std.testing.expect(result.metrics.truncated);
    try std.testing.expectEqual(@as(u64, 1), result.metrics.early_stops);
    try std.testing.expectEqual(@as(u64, 65), result.metrics.vendor_probes);
    try std.testing.expectEqual(@as(u64, 65), result.metrics.class_reads);
    try std.testing.expectEqual(@as(u64, 64), result.metrics.header_reads);
    try std.testing.expectEqual(@as(u64, 194), result.metrics.config_reads);
}

test "ECAM addresses and mapped window retain the bus-zero MCFG origin" {
    const std = @import("std");
    try std.testing.expectEqual(@as(?u64, 0x0200_0000), ecamOffset(32, 32, 0, 0, 0));
    try std.testing.expectEqual(@as(?u64, 0x0211_A0FC), ecamOffset(32, 33, 3, 2, 0x0FF));
    try std.testing.expectEqual(@as(?u64, null), ecamOffset(32, 31, 0, 0, 0));
    try std.testing.expectEqual(@as(?u64, null), ecamOffset(0, 0, 32, 0, 0));
    try std.testing.expectEqual(@as(?u64, null), ecamOffset(0, 0, 0, 8, 0));
    try std.testing.expectEqual(@as(?u64, null), ecamOffset(0, 0, 0, 0, 0x1000));
    const partial = ecamWindow(0xE000_0000, 32, 63).?;
    try std.testing.expectEqual(@as(u64, 0xE200_0000), partial.base);
    try std.testing.expectEqual(@as(u64, 0x0200_0000), partial.bytes);
    const full = ecamWindow(0xE000_0000, 0, 255).?;
    try std.testing.expectEqual(@as(u64, 0xE000_0000), full.base);
    try std.testing.expectEqual(@as(u64, 0x1000_0000), full.bytes);
    const last = ecamWindow(0xE000_0000, 255, 255).?;
    try std.testing.expectEqual(@as(u64, 0xEFF0_0000), last.base);
    try std.testing.expectEqual(@as(u64, 0x0010_0000), last.bytes);
    try std.testing.expect(ecamWindow(0xE000_0000, 63, 32) == null);
    try std.testing.expect(ecamWindow(0, 0, 255) == null);
    try std.testing.expect(ecamWindow(~@as(u64, 0) - 0x0100_0000, 32, 63) == null);
}
