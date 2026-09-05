// Runtime serialization for storage admission metadata. This owner is always
// released before filesystem, cache, driver or scheduler work begins.
const core = @import("access_state.zig");
const locks = @import("../memory/owner_locks.zig");
const scheduler = @import("../sched/scheduler.zig");

pub const Owner = core.Owner;
pub const DeviceRef = core.DeviceRef;
pub const MountRef = core.MountRef;
pub const Region = core.Region;
pub const Error = core.Error;
pub const UseKind = core.UseKind;
pub const OwnerResolver = *const fn () ?Owner;
pub const UseToken = struct { id: u64, owner: Owner };

var state: core.State = .{};
var owner_resolver: ?OwnerResolver = null;

// Same boot-only reset boundary as block.init; no live resources may survive.
pub fn initForBoot() void {
    state = .{};
    owner_resolver = null;
}

pub fn setOwnerResolver(resolver: OwnerResolver) void {
    owner_resolver = resolver;
}

pub fn currentOwner() ?Owner {
    if (owner_resolver) |resolve| if (resolve()) |owner| return owner;
    const running = scheduler.current() orelse return null;
    return .{ .task = running.id, .task_generation = running.generation };
}

// The block registry already owns locks.storage at these two boundaries.
// Neither helper acquires another owner or performs I/O.
pub fn registerDeviceLocked(slot: u32, sectors: u64) Error!DeviceRef {
    return state.registerDevice(slot, sectors);
}
pub fn retireDeviceLocked(ref: DeviceRef) Error!void {
    return state.retireDevice(ref);
}
pub fn beginRetirementLocked(ref: DeviceRef) Error!void {
    return state.beginRetirement(ref);
}
pub fn cancelRetirementLocked(ref: DeviceRef) void {
    state.cancelRetirement(ref);
}
pub fn bindMountLocked(slot: u32, region: Region, required: bool) Error!MountRef {
    return state.bindMount(slot, region, required);
}
pub fn unbindMountLocked(ref: MountRef) Error!void {
    return state.unbindMount(ref);
}
pub fn topologyGeneration() u64 {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.topology_generation;
}
pub fn topologyChanged() Error!void {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    try state.changed();
}

pub fn deviceReference(slot: u32) ?DeviceRef {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    if (slot >= state.devices.len or !state.devices[slot].active) return null;
    return .{ .slot = slot, .generation = state.devices[slot].generation };
}

pub fn bindMount(slot: u32, region: Region, runtime_required: bool) Error!MountRef {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.bindMount(slot, region, runtime_required);
}

pub fn unbindMount(ref: MountRef) Error!void {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.unbindMount(ref);
}

pub fn mountSnapshot(ref: MountRef) Error!core.Mount {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return (try state.mountValue(ref)).*;
}

pub fn mountReference(slot: u32) ?MountRef {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    if (slot >= state.mounts.len or !state.mounts[slot].active) return null;
    return .{ .slot = slot, .generation = state.mounts[slot].generation };
}

pub fn beginUse(ref: MountRef, kind: UseKind) Error!UseToken {
    const owner = currentOwner() orelse return error.Invalid;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return .{ .id = try state.beginUse(ref, owner, kind), .owner = owner };
}

pub fn endUse(token: UseToken) Error!void {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.endUse(token.id, token.owner);
}

pub fn beginRawRead(region: Region) Error!UseToken {
    const owner = currentOwner() orelse return error.Invalid;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return .{ .id = try state.beginRawRead(region, owner), .owner = owner };
}

pub fn prepareClaim(region: Region) Error!u64 {
    const owner = currentOwner() orelse return error.Invalid;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.prepareClaim(region, owner);
}

pub fn claimSnapshot(id: u64, owner: Owner) Error!core.Claim {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return (try state.claimValue(id, owner)).*;
}

pub fn activateClaim(id: u64, owner: Owner) Error!void {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.activateClaim(id, owner);
}

pub fn beginClaimIo(id: u64, region: Region) Error!UseToken {
    const owner = currentOwner() orelse return error.Invalid;
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return .{ .id = try state.beginClaimIo(id, owner, region), .owner = owner };
}

pub fn finishClaim(id: u64, owner: Owner) Error!void {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.finishClaim(id, owner);
}

pub fn releaseClaim(id: u64, owner: Owner) Error!void {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.releaseClaim(id, owner);
}

pub fn regionClaimed(region: Region) bool {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    return state.regionClaimed(region);
}

// Program lifecycle has already drained actual I/O and kernel streams.
// Only persistent user leases are retired here; never discard a live request.
pub fn releaseOwnerLeases(program: u32, generation: u64, task_id: u32, task_generation: u64) void {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    for (&state.uses) |*use| {
        if (use.id == 0 or use.kind != .lease or use.owner.program != program or use.owner.program_generation != generation) continue;
        if (task_id != 0 and (use.owner.task != task_id or use.owner.task_generation != task_generation)) continue;
        use.id = 0;
    }
}

pub fn ownerHasClaims(program: u32, generation: u64, task_id: u32, task_generation: u64) bool {
    const guard = locks.storage.acquire();
    defer locks.storage.release(guard);
    for (state.claims) |claim| {
        if (claim.id == 0 or claim.owner.program != program or claim.owner.program_generation != generation) continue;
        if (task_id != 0 and (claim.owner.task != task_id or claim.owner.task_generation != task_generation)) continue;
        return true;
    }
    return false;
}
