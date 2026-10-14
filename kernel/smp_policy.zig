// Native bounded policy; original host cases call the same C owner as boot.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

pub const max_cpus: usize = 32;

pub const Candidate = extern struct {
    apic_id: u32 = 0,
    enabled: bool = false,
    online_capable: bool = false,
};

pub const Plan = extern struct {
    apic_ids: [max_cpus]u32 = .{0} ** max_cpus,
    count: u32 = 0,
    duplicates: u32 = 0,
    disabled: u32 = 0,
};

extern fn r4os_rust_smp_plan(bsp: u32, candidates: [*]const Candidate, len: usize, output: *Plan) void;
extern fn r4os_rust_smp_least_loaded(mask: u64, loads: [*]const usize, len: usize) u32;
comptime {
    if (@sizeOf(Candidate) != 8 or @offsetOf(Candidate, "enabled") != 4 or
        @sizeOf(Plan) != 140 or @offsetOf(Plan, "count") != 128)
        @compileError("Native SMP policy C layout differs");
}
pub fn buildPlan(bsp_apic_id: u32, candidates: []const Candidate) Plan {
    var plan: Plan = undefined;
    r4os_rust_smp_plan(bsp_apic_id, candidates.ptr, candidates.len, &plan);
    return plan;
}
pub fn leastLoadedCpu(schedulable_mask: u64, loads: []const usize) u32 {
    return r4os_rust_smp_least_loaded(schedulable_mask, loads.ptr, loads.len);
}

test "topology removes BSP duplicates and disabled entries" {
    const testing = @import("std").testing;
    const candidates = [_]Candidate{
        .{ .apic_id = 0, .enabled = true },
        .{ .apic_id = 1, .enabled = true },
        .{ .apic_id = 1, .online_capable = true },
        .{ .apic_id = 2 },
        .{ .apic_id = 3, .online_capable = true },
    };
    const plan = buildPlan(0, &candidates);
    try testing.expectEqual(@as(u32, 3), plan.count);
    try testing.expectEqual(@as(u32, 2), plan.duplicates);
    try testing.expectEqual(@as(u32, 1), plan.disabled);
    try testing.expectEqual(@as(u32, 3), plan.apic_ids[2]);
}

test "load policy keeps permanent CPU zero fallback" {
    const testing = @import("std").testing;
    const loads = [_]usize{ 8, 3, 1, 0 };
    try testing.expectEqual(@as(u32, 0), leastLoadedCpu(1, &loads));
    try testing.expectEqual(@as(u32, 2), leastLoadedCpu(0b0111, &loads));
    try testing.expectEqual(@as(u32, 0), leastLoadedCpu(0, &loads));
}

test "topology never exceeds the per-CPU storage bound" {
    const testing = @import("std").testing;
    var candidates: [max_cpus + 8]Candidate = undefined;
    for (&candidates, 0..) |*candidate, index| {
        candidate.* = .{ .apic_id = @intCast(index + 1), .enabled = true };
    }
    const plan = buildPlan(0, &candidates);
    try testing.expectEqual(@as(u32, max_cpus), plan.count);
    try testing.expectEqual(@as(u32, max_cpus - 1), plan.apic_ids[max_cpus - 1]);
}
