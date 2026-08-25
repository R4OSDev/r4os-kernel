// Pure SMP admission policy.  It is shared by the kernel implementation and
// host tests so partial AP failures and deterministic 1-CPU fallback do not
// depend on QEMU timing.

pub const max_cpus: usize = 32;

pub const Candidate = struct {
    apic_id: u32 = 0,
    enabled: bool = false,
    online_capable: bool = false,
};

pub const Plan = struct {
    apic_ids: [max_cpus]u32 = .{0} ** max_cpus,
    count: u32 = 0,
    duplicates: u32 = 0,
    disabled: u32 = 0,
};

pub fn buildPlan(bsp_apic_id: u32, candidates: []const Candidate) Plan {
    var plan: Plan = .{};
    plan.apic_ids[0] = bsp_apic_id;
    plan.count = 1;
    for (candidates) |candidate| {
        if (!candidate.enabled and !candidate.online_capable) {
            plan.disabled += 1;
            continue;
        }
        var duplicate = false;
        var index: usize = 0;
        while (index < plan.count) : (index += 1) {
            if (plan.apic_ids[index] == candidate.apic_id) {
                duplicate = true;
                break;
            }
        }
        if (duplicate) {
            plan.duplicates += 1;
            continue;
        }
        if (plan.count >= max_cpus) break;
        plan.apic_ids[plan.count] = candidate.apic_id;
        plan.count += 1;
    }
    return plan;
}

pub fn leastLoadedCpu(schedulable_mask: u64, loads: []const usize) u32 {
    if ((schedulable_mask & 1) == 0 or loads.len == 0) return 0;
    var selected: u32 = 0;
    var selected_load = loads[0];
    var index: u32 = 1;
    while (index < max_cpus and index < loads.len) : (index += 1) {
        const mask = @as(u64, 1) << @intCast(index);
        if ((schedulable_mask & mask) == 0) continue;
        if (loads[index] < selected_load) {
            selected = index;
            selected_load = loads[index];
        }
    }
    return selected;
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
