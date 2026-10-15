// Native bounded admission owner; only private fixed C layouts cross this facade.
const std = @import("std");
const builtin = @import("builtin");
comptime {
    if (builtin.is_test) {
        _ = @import("rust_owner_test");
    }
}
pub const max_devices = 16;
pub const max_mounts = 27;
pub const max_uses = 256;
pub const max_claims = 16;
pub const Error = error{ Invalid, Stale, Busy, Protected, Capacity, WrongOwner };
pub const Owner = extern struct {
    task: u32,
    task_generation: u64,
    program: u32 = 0,
    program_generation: u64 = 0,
    pub fn eql(a: Owner, b: Owner) bool {
        return r4os_rust_storage_owner_equal(&a, &b);
    }
};
pub const DeviceRef = extern struct { slot: u32, generation: u64 };
pub const MountRef = extern struct { slot: u32, generation: u64 };
pub const Region = extern struct {
    device: DeviceRef,
    first: u64,
    count: u64,
    pub fn overlaps(a: Region, b: Region) bool {
        return r4os_rust_storage_region(0, &a, &b);
    }
    pub fn contains(a: Region, b: Region) bool {
        return r4os_rust_storage_region(1, &a, &b);
    }
};
const empty_owner: Owner = .{ .task = 0, .task_generation = 0 };
const empty_region: Region = .{ .device = .{ .slot = 0, .generation = 0 }, .first = 0, .count = 0 };
pub const Device = extern struct { active: bool = false, retiring: bool = false, generation: u64 = 0, sectors: u64 = 0 };
pub const Mount = extern struct { active: bool = false, generation: u64 = 0, region: Region = empty_region, runtime_required: bool = false };
pub const UseKind = enum(u8) { request, lease, raw };
pub const Use = extern struct { id: u64 = 0, owner: Owner = empty_owner, region: Region = empty_region, kind: UseKind = .request };
pub const Phase = enum(u8) { preparing, active, finishing };
pub const Claim = extern struct { id: u64 = 0, owner: Owner = empty_owner, region: Region = empty_region, phase: Phase = .preparing };
pub const Input = extern struct { owner: Owner = empty_owner, region: Region = empty_region, reference: DeviceRef = .{ .slot = 0, .generation = 0 }, id: u64 = 0, value: u64 = 0, kind: u8 = 0 };
pub fn check(status: i32) Error!void {
    switch (status) {
        0 => {},
        1 => return error.Invalid,
        2 => return error.Stale,
        3 => return error.Busy,
        4 => return error.Protected,
        5 => return error.Capacity,
        6 => return error.WrongOwner,
        else => unreachable,
    }
}
extern fn r4os_rust_storage_owner_equal(*const Owner, *const Owner) callconv(.c) bool;
extern fn r4os_rust_storage_region(u8, *const Region, *const Region) callconv(.c) bool;
extern fn r4os_rust_storage_access(*State, u8, *const Input, *u64) callconv(.c) i32;
extern fn r4os_rust_storage_access_region(*const State, bool, *const Region) callconv(.c) bool;
pub const State = extern struct {
    devices: [max_devices]Device = .{Device{}} ** max_devices,
    mounts: [max_mounts]Mount = .{Mount{}} ** max_mounts,
    uses: [max_uses]Use = .{Use{}} ** max_uses,
    claims: [max_claims]Claim = .{Claim{}} ** max_claims,
    sequence: u64 = 0,
    topology_generation: u64 = 1,
    fn call(self: *State, op: u8, input: Input) Error!u64 {
        var out: u64 = undefined;
        try check(r4os_rust_storage_access(self, op, &input, &out));
        return out;
    }
    pub fn changed(self: *State) Error!void {
        _ = try self.call(0, .{});
    }
    pub fn registerDevice(self: *State, slot: u32, sectors: u64) Error!DeviceRef {
        return .{ .slot = slot, .generation = try self.call(1, .{ .reference = .{ .slot = slot, .generation = 0 }, .value = sectors }) };
    }
    pub fn beginRetirement(self: *State, ref: DeviceRef) Error!void {
        _ = try self.call(2, .{ .reference = ref });
    }
    pub fn cancelRetirement(self: *State, ref: DeviceRef) void {
        _ = self.call(3, .{ .reference = ref }) catch unreachable;
    }
    pub fn retireDevice(self: *State, ref: DeviceRef) Error!void {
        _ = try self.call(4, .{ .reference = ref });
    }
    pub fn deviceValue(self: *State, ref: DeviceRef) Error!*Device {
        return &self.devices[try self.call(5, .{ .reference = ref })];
    }
    pub fn validateRegion(self: *State, region: Region) Error!void {
        _ = try self.call(6, .{ .region = region });
    }
    pub fn bindMount(self: *State, slot: u32, region: Region, required: bool) Error!MountRef {
        return .{ .slot = slot, .generation = try self.call(7, .{ .reference = .{ .slot = slot, .generation = 0 }, .region = region, .value = @intFromBool(required) }) };
    }
    pub fn mountValue(self: *State, ref: MountRef) Error!*Mount {
        return &self.mounts[try self.call(8, .{ .reference = .{ .slot = ref.slot, .generation = ref.generation } })];
    }
    pub fn unbindMount(self: *State, ref: MountRef) Error!void {
        _ = try self.call(9, .{ .reference = .{ .slot = ref.slot, .generation = ref.generation } });
    }
    pub fn beginUse(self: *State, ref: MountRef, owner: Owner, kind: UseKind) Error!u64 {
        return self.call(10, .{ .reference = .{ .slot = ref.slot, .generation = ref.generation }, .owner = owner, .kind = @intFromEnum(kind) });
    }
    pub fn beginRawRead(self: *State, region: Region, owner: Owner) Error!u64 {
        return self.call(11, .{ .region = region, .owner = owner });
    }
    pub fn endUse(self: *State, id: u64, owner: Owner) Error!void {
        _ = try self.call(12, .{ .id = id, .owner = owner });
    }
    pub fn prepareClaim(self: *State, region: Region, owner: Owner) Error!u64 {
        return self.call(13, .{ .region = region, .owner = owner });
    }
    pub fn claimValue(self: *State, id: u64, owner: Owner) Error!*Claim {
        return &self.claims[try self.call(14, .{ .id = id, .owner = owner })];
    }
    pub fn activateClaim(self: *State, id: u64, owner: Owner) Error!void {
        _ = try self.call(15, .{ .id = id, .owner = owner });
    }
    pub fn beginClaimIo(self: *State, id: u64, owner: Owner, region: Region) Error!u64 {
        return self.call(16, .{ .id = id, .owner = owner, .region = region });
    }
    pub fn finishClaim(self: *State, id: u64, owner: Owner) Error!void {
        _ = try self.call(17, .{ .id = id, .owner = owner });
    }
    pub fn releaseClaim(self: *State, id: u64, owner: Owner) Error!void {
        _ = try self.call(18, .{ .id = id, .owner = owner });
    }
    pub fn regionUsed(self: *const State, region: Region) bool {
        return r4os_rust_storage_access_region(self, false, &region);
    }
    pub fn regionClaimed(self: *const State, region: Region) bool {
        return r4os_rust_storage_access_region(self, true, &region);
    }
};
comptime {
    if (@sizeOf(Owner) != 32 or @sizeOf(DeviceRef) != 16 or @sizeOf(MountRef) != 16 or @sizeOf(Region) != 32 or @sizeOf(Device) != 24 or @sizeOf(Mount) != 56 or @sizeOf(Use) != 80 or @sizeOf(Claim) != 80 or @sizeOf(State) != 23672 or @offsetOf(State, "topology_generation") != 23664 or @sizeOf(Input) != 104) @compileError("Native storage admission C layout differs");
}
const testing = std.testing;
const test_owner: Owner = .{ .task = 1, .task_generation = 10, .program = 1, .program_generation = 20 };
const test_other: Owner = .{ .task = 2, .task_generation = 11, .program = 2, .program_generation = 21 };

test "read lease and in-flight request block a claim before admission closes" {
    var s: State = .{};
    const device = try s.registerDevice(0, 10000);
    const region: Region = .{ .device = device, .first = 100, .count = 500 };
    const mount = try s.bindMount(4, region, false);
    for ([_]UseKind{ .request, .lease }) |kind| {
        const use = try s.beginUse(mount, test_other, kind);
        try testing.expectError(error.Busy, s.prepareClaim(region, test_owner));
        try testing.expect(!s.regionClaimed(region));
        try testing.expectError(error.WrongOwner, s.endUse(use, test_owner));
        try s.endUse(use, test_other);
        try testing.expectError(error.Stale, s.endUse(use, test_other));
    }
    _ = try s.prepareClaim(region, test_owner);
    try testing.expectError(error.Busy, s.beginUse(mount, test_other, .lease));
    try testing.expectError(error.Busy, s.beginRawRead(region, test_other));
}

test "partition claim preserves a disjoint volume and protects the RAM runtime" {
    var s: State = .{};
    const disk = try s.registerDevice(0, 10000);
    const ram = try s.registerDevice(1, 1000);
    const a: Region = .{ .device = disk, .first = 100, .count = 500 };
    const b: Region = .{ .device = disk, .first = 600, .count = 500 };
    const mb = try s.bindMount(5, b, false);
    const memory: Region = .{ .device = ram, .first = 0, .count = 1000 };
    const mc = try s.bindMount(2, memory, true);
    _ = try s.prepareClaim(a, test_owner);
    const use = try s.beginUse(mb, test_other, .request);
    try s.endUse(use, test_other);
    try testing.expectError(error.Busy, s.prepareClaim(.{ .device = disk, .first = 0, .count = 10000 }, test_other));
    try testing.expectError(error.Protected, s.prepareClaim(memory, test_owner));
    try testing.expectError(error.Protected, s.unbindMount(mc));
}

test "raw claim I/O is owner and range bound; finish closes admission before draining" {
    var s: State = .{};
    const disk = try s.registerDevice(0, 10000);
    const region: Region = .{ .device = disk, .first = 100, .count = 500 };
    const id = try s.prepareClaim(region, test_owner);
    try testing.expectError(error.Busy, s.beginClaimIo(id, test_owner, region));
    try testing.expectError(error.WrongOwner, s.activateClaim(id, test_other));
    try s.activateClaim(id, test_owner);
    try testing.expectError(error.Invalid, s.beginClaimIo(id, test_owner, .{ .device = disk, .first = 99, .count = 2 }));
    const use = try s.beginClaimIo(id, test_owner, .{ .device = disk, .first = 599, .count = 1 });
    try testing.expectError(error.Busy, s.finishClaim(id, test_owner));
    try testing.expectError(error.Busy, s.releaseClaim(id, test_owner));
    try testing.expectError(error.Busy, s.beginClaimIo(id, test_owner, region));
    try s.endUse(use, test_owner);
    try s.finishClaim(id, test_owner);
    try s.releaseClaim(id, test_owner);
    try testing.expectError(error.Stale, s.claimValue(id, test_owner));
    _ = try s.beginRawRead(region, test_other);
}

test "remount and physical slot reuse never resurrect old references" {
    var s: State = .{};
    const first = try s.registerDevice(0, 1000);
    const region: Region = .{ .device = first, .first = 1, .count = 500 };
    const old_mount = try s.bindMount(4, region, false);
    try s.unbindMount(old_mount);
    const new_mount = try s.bindMount(4, region, false);
    try testing.expectError(error.Stale, s.beginUse(old_mount, test_owner, .lease));
    try testing.expectError(error.Busy, s.retireDevice(first));
    try s.unbindMount(new_mount);
    try s.retireDevice(first);
    const second = try s.registerDevice(0, 1000);
    try testing.expect(first.generation != second.generation);
    try testing.expectError(error.Stale, s.beginRawRead(region, test_owner));
    try testing.expectError(error.Invalid, s.validateRegion(.{ .device = second, .first = std.math.maxInt(u64), .count = 2 }));
}

test "bounded capacity and exhausted generations fail without aliasing a live use" {
    var s: State = .{};
    const disk = try s.registerDevice(0, 1000);
    const region: Region = .{ .device = disk, .first = 1, .count = 500 };
    const mount = try s.bindMount(4, region, false);
    var ids: [max_uses]u64 = undefined;
    for (&ids) |*id| id.* = try s.beginUse(mount, test_owner, .lease);
    try testing.expectError(error.Capacity, s.beginUse(mount, test_owner, .lease));
    for (ids) |id| try s.endUse(id, test_owner);
    s.sequence = std.math.maxInt(u64);
    try testing.expectError(error.Capacity, s.prepareClaim(region, test_owner));
    try testing.expect(!s.regionClaimed(region));
    s.sequence = 1000;
    const claim = try s.prepareClaim(region, test_owner);
    s.topology_generation = std.math.maxInt(u64);
    try s.finishClaim(claim, test_owner);
    try s.releaseClaim(claim, test_owner);
    try testing.expect(!s.regionClaimed(region));
    try testing.expectError(error.Capacity, s.prepareClaim(region, test_owner));
}

test "topology generations ignore file traffic and retiring registration closes new claims" {
    var s: State = .{};
    const disk = try s.registerDevice(0, 1000);
    const region: Region = .{ .device = disk, .first = 1, .count = 500 };
    const mounted = try s.bindMount(4, region, false);
    const before = s.topology_generation;
    const use = try s.beginUse(mounted, test_owner, .request);
    try s.endUse(use, test_owner);
    try testing.expectEqual(before, s.topology_generation);
    try s.unbindMount(mounted);
    try s.beginRetirement(disk);
    try testing.expectError(error.Stale, s.prepareClaim(region, test_owner));
    s.cancelRetirement(disk);
    const claim = try s.prepareClaim(region, test_owner);
    try testing.expectError(error.Busy, s.beginRetirement(disk));
    try s.finishClaim(claim, test_owner);
    try s.releaseClaim(claim, test_owner);
    try s.beginRetirement(disk);
    try s.retireDevice(disk);
    try testing.expectError(error.Stale, s.prepareClaim(region, test_owner));
}
