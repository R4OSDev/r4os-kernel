// Bounded storage admission state. The runtime owner serializes calls; this
// layer never waits, calls a driver, touches a filesystem or allocates memory.
const std = @import("std");

pub const max_devices = 16;
pub const max_mounts = 27; // A..Z plus the internal boot-volume mount.
pub const max_uses = 256;
pub const max_claims = 16;
pub const Error = error{ Invalid, Stale, Busy, Protected, Capacity, WrongOwner };

pub const Owner = struct {
    task: u32,
    task_generation: u64,
    program: u32 = 0,
    program_generation: u64 = 0,

    pub fn eql(a: Owner, b: Owner) bool {
        return std.meta.eql(a, b);
    }
};

pub const DeviceRef = struct { slot: u32, generation: u64 };
pub const MountRef = struct { slot: u32, generation: u64 };
pub const Region = struct {
    device: DeviceRef,
    first: u64,
    count: u64,

    pub fn overlaps(a: Region, b: Region) bool {
        if (!std.meta.eql(a.device, b.device) or a.count == 0 or b.count == 0) return false;
        // Difference comparisons also remain well-defined for rejected input
        // whose first+count would overflow.
        return if (a.first <= b.first) b.first - a.first < a.count else a.first - b.first < b.count;
    }

    pub fn contains(outer: Region, inner: Region) bool {
        return std.meta.eql(outer.device, inner.device) and inner.count != 0 and
            inner.first >= outer.first and inner.first - outer.first <= outer.count and
            inner.count <= outer.count - (inner.first - outer.first);
    }
};

pub const Device = struct { active: bool = false, retiring: bool = false, generation: u64 = 0, sectors: u64 = 0 };
pub const Mount = struct {
    active: bool = false,
    generation: u64 = 0,
    region: Region = undefined,
    runtime_required: bool = false,
};
pub const UseKind = enum { request, lease, raw };
pub const Use = struct {
    id: u64 = 0,
    owner: Owner = undefined,
    region: Region = undefined,
    kind: UseKind = .request,
};
pub const Phase = enum { preparing, active, finishing };
pub const Claim = struct {
    id: u64 = 0,
    owner: Owner = undefined,
    region: Region = undefined,
    phase: Phase = .preparing,
};

pub const State = struct {
    devices: [max_devices]Device = .{Device{}} ** max_devices,
    mounts: [max_mounts]Mount = .{Mount{}} ** max_mounts,
    uses: [max_uses]Use = .{Use{}} ** max_uses,
    claims: [max_claims]Claim = .{Claim{}} ** max_claims,
    sequence: u64 = 0,
    topology_generation: u64 = 1,

    pub fn changed(self: *State) Error!void {
        if (self.topology_generation == std.math.maxInt(u64)) return error.Capacity;
        self.topology_generation += 1;
    }

    fn next(self: *State) Error!u64 {
        if (self.sequence == std.math.maxInt(u64)) return error.Capacity;
        self.sequence += 1;
        return self.sequence;
    }

    pub fn registerDevice(self: *State, slot: u32, sectors: u64) Error!DeviceRef {
        if (slot >= max_devices or sectors == 0) return error.Invalid;
        if (self.devices[slot].active) return error.Busy;
        const generation = try self.next();
        try self.changed();
        self.devices[slot] = .{ .active = true, .generation = generation, .sectors = sectors };
        return .{ .slot = slot, .generation = generation };
    }

    fn canRetire(self: *State, ref: DeviceRef) Error!*Device {
        if (ref.slot >= max_devices or ref.generation == 0) return error.Stale;
        const device = &self.devices[ref.slot];
        if (!device.active or device.generation != ref.generation) return error.Stale;
        const all: Region = .{ .device = ref, .first = 0, .count = device.sectors };
        if (self.regionUsed(all) or self.regionClaimed(all)) return error.Busy;
        for (self.mounts) |mount| {
            if (mount.active and mount.region.overlaps(all)) return error.Busy;
        }
        return device;
    }

    pub fn beginRetirement(self: *State, ref: DeviceRef) Error!void {
        const device = try self.canRetire(ref);
        if (device.retiring) return error.Busy;
        device.retiring = true;
    }

    pub fn cancelRetirement(self: *State, ref: DeviceRef) void {
        if (ref.slot < max_devices and self.devices[ref.slot].generation == ref.generation)
            self.devices[ref.slot].retiring = false;
    }

    pub fn retireDevice(self: *State, ref: DeviceRef) Error!void {
        const device = try self.canRetire(ref);
        try self.changed();
        device.active = false;
    }

    pub fn deviceValue(self: *State, ref: DeviceRef) Error!*Device {
        if (ref.slot >= max_devices or ref.generation == 0) return error.Stale;
        const value = &self.devices[ref.slot];
        if (!value.active or value.retiring or value.generation != ref.generation) return error.Stale;
        return value;
    }

    pub fn validateRegion(self: *State, region: Region) Error!void {
        const d = try self.deviceValue(region.device);
        if (region.count == 0 or region.first >= d.sectors or region.count > d.sectors - region.first) return error.Invalid;
    }

    pub fn bindMount(self: *State, slot: u32, region: Region, runtime_required: bool) Error!MountRef {
        try self.validateRegion(region);
        if (slot >= max_mounts) return error.Invalid;
        if (self.mounts[slot].active) return error.Busy;
        const generation = try self.next();
        try self.changed();
        self.mounts[slot] = .{ .active = true, .generation = generation, .region = region, .runtime_required = runtime_required };
        return .{ .slot = slot, .generation = generation };
    }

    pub fn mountValue(self: *State, ref: MountRef) Error!*Mount {
        if (ref.slot >= max_mounts or ref.generation == 0) return error.Stale;
        const value = &self.mounts[ref.slot];
        if (!value.active or value.generation != ref.generation) return error.Stale;
        try self.validateRegion(value.region);
        return value;
    }

    // Orchestration calls this after admission closes and its flush succeeds.
    // An unrelated mount on the same physical device keeps its generation.
    pub fn unbindMount(self: *State, ref: MountRef) Error!void {
        const value = try self.mountValue(ref);
        if (value.runtime_required) return error.Protected;
        if (self.regionUsed(value.region)) return error.Busy;
        try self.changed();
        value.active = false;
    }

    pub fn beginUse(self: *State, ref: MountRef, owner: Owner, kind: UseKind) Error!u64 {
        if (kind == .raw) return error.Invalid;
        const value = try self.mountValue(ref);
        if (self.regionClaimed(value.region)) return error.Busy;
        return self.reserveUse(value.region, owner, kind);
    }

    pub fn beginRawRead(self: *State, region: Region, owner: Owner) Error!u64 {
        try self.validateRegion(region);
        if (self.regionClaimed(region)) return error.Busy;
        return self.reserveUse(region, owner, .raw);
    }

    fn reserveUse(self: *State, region: Region, owner: Owner, kind: UseKind) Error!u64 {
        if (owner.task == 0 or owner.task_generation == 0 or
            (owner.program == 0) != (owner.program_generation == 0)) return error.Invalid;
        for (&self.uses) |*use| {
            if (use.id != 0) continue;
            use.* = .{ .id = try self.next(), .region = region, .owner = owner, .kind = kind };
            return use.id;
        }
        return error.Capacity;
    }

    pub fn endUse(self: *State, id: u64, owner: Owner) Error!void {
        if (id == 0) return error.Stale;
        for (&self.uses) |*use| {
            if (use.id != id) continue;
            if (!use.owner.eql(owner)) return error.WrongOwner;
            use.id = 0;
            return;
        }
        return error.Stale;
    }

    pub fn prepareClaim(self: *State, region: Region, owner: Owner) Error!u64 {
        try self.validateRegion(region);
        if (owner.task == 0 or owner.task_generation == 0 or
            (owner.program == 0) != (owner.program_generation == 0)) return error.Invalid;
        for (self.mounts) |value| {
            if (value.active and value.runtime_required and value.region.overlaps(region)) return error.Protected;
        }
        if (self.regionUsed(region) or self.regionClaimed(region)) return error.Busy;
        for (&self.claims) |*claim| {
            if (claim.id != 0) continue;
            try self.changed();
            claim.* = .{ .id = try self.next(), .owner = owner, .region = region };
            return claim.id;
        }
        return error.Capacity;
    }

    pub fn claimValue(self: *State, id: u64, owner: Owner) Error!*Claim {
        if (id == 0) return error.Stale;
        for (&self.claims) |*value| {
            if (value.id != id) continue;
            if (!value.owner.eql(owner)) return error.WrongOwner;
            try self.validateRegion(value.region);
            return value;
        }
        return error.Stale;
    }

    pub fn activateClaim(self: *State, id: u64, owner: Owner) Error!void {
        const value = try self.claimValue(id, owner);
        if (value.phase != .preparing) return error.Invalid;
        value.phase = .active;
    }

    pub fn beginClaimIo(self: *State, id: u64, owner: Owner, region: Region) Error!u64 {
        const value = try self.claimValue(id, owner);
        if (value.phase != .active) return error.Busy;
        if (!value.region.contains(region)) return error.Invalid;
        return self.reserveUse(region, owner, .raw);
    }

    pub fn finishClaim(self: *State, id: u64, owner: Owner) Error!void {
        const value = try self.claimValue(id, owner);
        // Reject new claim I/O even while the caller waits for an outstanding
        // request to physically finish; dropping its buffers is not a drain.
        value.phase = .finishing;
        if (self.regionUsed(value.region)) return error.Busy;
    }

    // Called only after the orchestrator records flush/rescan/remount results.
    // A failed preparation also uses finish/release, with no raw write allowed.
    pub fn releaseClaim(self: *State, id: u64, owner: Owner) Error!void {
        const value = try self.claimValue(id, owner);
        if (value.phase != .finishing or self.regionUsed(value.region)) return error.Busy;
        // Exhaustion forbids new topology publication, never final cleanup.
        self.changed() catch {};
        value.id = 0;
    }

    pub fn regionUsed(self: *const State, region: Region) bool {
        for (self.uses) |use| if (use.id != 0 and use.region.overlaps(region)) return true;
        return false;
    }

    pub fn regionClaimed(self: *const State, region: Region) bool {
        for (self.claims) |value| if (value.id != 0 and value.region.overlaps(region)) return true;
        return false;
    }
};

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
