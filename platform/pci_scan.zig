pub const bus_kind_legacy: u8 = 1;
pub const bus_kind_ecam: u8 = 2;
pub const max_devices: usize = 64;

pub const Device = struct {
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

pub const Metrics = struct {
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

pub const Result = struct {
    devices: [max_devices]Device = .{Device{}} ** max_devices,
    metrics: Metrics = .{},
};

pub const Reader = struct {
    context: ?*anyopaque = null,
    read_config32: *const fn (?*anyopaque, u8, u8, u8, u16) u32,
};

pub const Range = struct {
    bus_kind: u8 = 0,
    start_bus: u8 = 0,
    end_bus: u8 = 0,
};

pub const CoveragePlan = struct {
    ranges: [3]Range = .{Range{}} ** 3,
    count: u8 = 0,
    ecam_used: bool = false,
    legacy_used: bool = false,
    partial: bool = false,

    fn append(self: *CoveragePlan, range: Range) void {
        if (self.count >= self.ranges.len) return;
        self.ranges[@intCast(self.count)] = range;
        self.count += 1;
        if (range.bus_kind == bus_kind_ecam) self.ecam_used = true;
        if (range.bus_kind == bus_kind_legacy) self.legacy_used = true;
    }
};

pub fn planCoverage(ecam_ready: bool, segment: u16, start_bus: u8, end_bus: u8) CoveragePlan {
    var plan: CoveragePlan = .{};
    if (!ecam_ready or segment != 0 or start_bus > end_bus) {
        plan.append(.{ .bus_kind = bus_kind_legacy, .start_bus = 0, .end_bus = 255 });
        return plan;
    }

    plan.append(.{ .bus_kind = bus_kind_ecam, .start_bus = start_bus, .end_bus = end_bus });
    if (start_bus > 0) {
        plan.append(.{ .bus_kind = bus_kind_legacy, .start_bus = 0, .end_bus = start_bus - 1 });
    }
    if (end_bus < 255) {
        plan.append(.{ .bus_kind = bus_kind_legacy, .start_bus = end_bus + 1, .end_bus = 255 });
    }
    plan.partial = plan.legacy_used;
    return plan;
}

pub fn scanRange(result: *Result, range: Range, reader: Reader) bool {
    var bus: u16 = range.start_bus;
    while (bus <= range.end_bus) : (bus += 1) {
        var device: u8 = 0;
        while (device < 32) : (device += 1) {
            var function: u8 = 0;
            while (function < 8) : (function += 1) {
                const vendor_device = read(result, reader, @intCast(bus), device, function, 0x00, .vendor);
                const vendor: u16 = @truncate(vendor_device);
                if (vendor == 0xFFFF) {
                    if (function == 0) break;
                    continue;
                }

                const class_reg = read(result, reader, @intCast(bus), device, function, 0x08, .class);
                const found = Device{
                    .bus_kind = range.bus_kind,
                    .bus = @intCast(bus),
                    .device = device,
                    .function = function,
                    .vendor_id = vendor,
                    .device_id = @truncate(vendor_device >> 16),
                    .class_code = @truncate(class_reg >> 24),
                    .subclass = @truncate(class_reg >> 16),
                    .prog_if = @truncate(class_reg >> 8),
                };
                if (!store(result, found)) return false;

                if (function == 0) {
                    const header_type: u8 = @truncate(read(result, reader, @intCast(bus), device, function, 0x0C, .header) >> 16);
                    if ((header_type & 0x80) == 0) break;
                }
            }
        }
    }
    return true;
}

pub fn findByClass(result: *const Result, class_code: u8, subclass: u8, start_index: usize) ?usize {
    var index = start_index;
    const stored: usize = @intCast(result.metrics.stored);
    while (index < stored and index < result.devices.len) : (index += 1) {
        const device = result.devices[index];
        if (device.class_code == class_code and device.subclass == subclass) return index;
    }
    return null;
}

pub fn ecamOffset(start_bus: u8, bus: u8, device: u8, function: u8, offset: u16) ?u64 {
    if (bus < start_bus or device >= 32 or function >= 8 or offset > 0x0FFF) return null;
    return ((@as(u64, bus) - @as(u64, start_bus)) << 20) |
        (@as(u64, device) << 15) |
        (@as(u64, function) << 12) |
        @as(u64, offset & 0x0FFC);
}

const ReadPurpose = enum {
    vendor,
    class,
    header,
};

fn read(result: *Result, reader: Reader, bus: u8, device: u8, function: u8, offset: u16, purpose: ReadPurpose) u32 {
    result.metrics.config_reads +%= 1;
    switch (purpose) {
        .vendor => {
            result.metrics.vendor_probes +%= 1;
            result.metrics.function_pages +%= 1;
        },
        .class => result.metrics.class_reads +%= 1,
        .header => result.metrics.header_reads +%= 1,
    }
    return reader.read_config32(reader.context, bus, device, function, offset);
}

fn store(result: *Result, device: Device) bool {
    result.metrics.found +%= 1;
    if (result.metrics.stored >= result.devices.len) {
        result.metrics.dropped +%= 1;
        result.metrics.truncated = true;
        result.metrics.early_stops +%= 1;
        return false;
    }
    result.devices[@intCast(result.metrics.stored)] = device;
    result.metrics.stored += 1;
    return true;
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

test "ECAM offsets are relative to the advertised start bus" {
    const std = @import("std");
    try std.testing.expectEqual(@as(?u64, 0), ecamOffset(32, 32, 0, 0, 0));
    try std.testing.expectEqual(@as(?u64, 0x0011_A0FC), ecamOffset(32, 33, 3, 2, 0x0FF));
    try std.testing.expectEqual(@as(?u64, null), ecamOffset(32, 31, 0, 0, 0));
    try std.testing.expectEqual(@as(?u64, null), ecamOffset(0, 0, 32, 0, 0));
    try std.testing.expectEqual(@as(?u64, null), ecamOffset(0, 0, 0, 8, 0));
}
