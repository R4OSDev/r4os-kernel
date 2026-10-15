// Stateless facade: canonical storage metadata and resolver binding belong to Rust.
const core = @import("access_state.zig");
pub const Owner = core.Owner;
pub const DeviceRef = core.DeviceRef;
pub const MountRef = core.MountRef;
pub const Region = core.Region;
pub const Error = core.Error;
pub const UseKind = core.UseKind;
pub const OwnerResolver = *const fn () ?Owner;
pub const UseToken = struct { id: u64, owner: Owner };
extern fn r4os_rust_storage_access_init() callconv(.c) void;
extern fn r4os_rust_storage_access_resolver(*const fn (*Owner) callconv(.c) bool) callconv(.c) void;
extern fn r4os_rust_storage_access_current(*Owner) callconv(.c) bool;
extern fn r4os_rust_storage_access_runtime(u8, bool, *const core.Input, *u64, ?*anyopaque) callconv(.c) i32;
fn call(op: u8, locked: bool, input: core.Input, snapshot: ?*anyopaque) Error!u64 {
    var out: u64 = undefined;
    try core.check(r4os_rust_storage_access_runtime(op, locked, &input, &out, snapshot));
    return out;
}
pub fn initForBoot() void {
    r4os_rust_storage_access_init();
}
pub fn setOwnerResolver(comptime resolver: OwnerResolver) void {
    const Bridge = struct {
        fn resolve(out: *Owner) callconv(.c) bool {
            const owner = resolver() orelse return false;
            out.* = owner;
            return true;
        }
    };
    r4os_rust_storage_access_resolver(Bridge.resolve);
}
pub fn currentOwner() ?Owner {
    var out: Owner = undefined;
    return if (r4os_rust_storage_access_current(&out)) out else null;
}
pub fn registerDeviceLocked(slot: u32, sectors: u64) Error!DeviceRef {
    return .{ .slot = slot, .generation = try call(1, true, .{ .reference = .{ .slot = slot, .generation = 0 }, .value = sectors }, null) };
}
pub fn retireDeviceLocked(ref: DeviceRef) Error!void {
    _ = try call(4, true, .{ .reference = ref }, null);
}
pub fn beginRetirementLocked(ref: DeviceRef) Error!void {
    _ = try call(2, true, .{ .reference = ref }, null);
}
pub fn cancelRetirementLocked(ref: DeviceRef) void {
    _ = call(3, true, .{ .reference = ref }, null) catch unreachable;
}
pub fn bindMountLocked(slot: u32, region: Region, required: bool) Error!MountRef {
    return .{ .slot = slot, .generation = try call(7, true, .{ .reference = .{ .slot = slot, .generation = 0 }, .region = region, .value = @intFromBool(required) }, null) };
}
pub fn unbindMountLocked(ref: MountRef) Error!void {
    _ = try call(9, true, .{ .reference = .{ .slot = ref.slot, .generation = ref.generation } }, null);
}
pub fn topologyGeneration() u64 {
    return call(19, false, .{}, null) catch unreachable;
}
pub fn topologyChanged() Error!void {
    _ = try call(0, false, .{}, null);
}
pub fn deviceReference(slot: u32) ?DeviceRef {
    return .{ .slot = slot, .generation = call(20, false, .{ .reference = .{ .slot = slot, .generation = 0 } }, null) catch return null };
}
pub fn bindMount(slot: u32, region: Region, required: bool) Error!MountRef {
    return .{ .slot = slot, .generation = try call(7, false, .{ .reference = .{ .slot = slot, .generation = 0 }, .region = region, .value = @intFromBool(required) }, null) };
}
pub fn unbindMount(ref: MountRef) Error!void {
    _ = try call(9, false, .{ .reference = .{ .slot = ref.slot, .generation = ref.generation } }, null);
}
pub fn mountSnapshot(ref: MountRef) Error!core.Mount {
    var out: core.Mount = undefined;
    _ = try call(8, false, .{ .reference = .{ .slot = ref.slot, .generation = ref.generation } }, &out);
    return out;
}
pub fn mountReference(slot: u32) ?MountRef {
    return .{ .slot = slot, .generation = call(21, false, .{ .reference = .{ .slot = slot, .generation = 0 } }, null) catch return null };
}
pub fn beginUse(ref: MountRef, kind: UseKind) Error!UseToken {
    const owner = currentOwner() orelse return error.Invalid;
    return .{ .id = try call(10, false, .{ .reference = .{ .slot = ref.slot, .generation = ref.generation }, .owner = owner, .kind = @intFromEnum(kind) }, null), .owner = owner };
}
pub fn endUse(token: UseToken) Error!void {
    _ = try call(12, false, .{ .id = token.id, .owner = token.owner }, null);
}
pub fn beginRawRead(region: Region) Error!UseToken {
    const owner = currentOwner() orelse return error.Invalid;
    return .{ .id = try call(11, false, .{ .region = region, .owner = owner }, null), .owner = owner };
}
pub fn prepareClaim(region: Region) Error!u64 {
    const owner = currentOwner() orelse return error.Invalid;
    return call(13, false, .{ .region = region, .owner = owner }, null);
}
pub fn claimSnapshot(id: u64, owner: Owner) Error!core.Claim {
    var out: core.Claim = undefined;
    _ = try call(14, false, .{ .id = id, .owner = owner }, &out);
    return out;
}
pub fn activateClaim(id: u64, owner: Owner) Error!void {
    _ = try call(15, false, .{ .id = id, .owner = owner }, null);
}
pub fn beginClaimIo(id: u64, region: Region) Error!UseToken {
    const owner = currentOwner() orelse return error.Invalid;
    return .{ .id = try call(16, false, .{ .id = id, .owner = owner, .region = region }, null), .owner = owner };
}
pub fn finishClaim(id: u64, owner: Owner) Error!void {
    _ = try call(17, false, .{ .id = id, .owner = owner }, null);
}
pub fn releaseClaim(id: u64, owner: Owner) Error!void {
    _ = try call(18, false, .{ .id = id, .owner = owner }, null);
}
pub fn regionClaimed(region: Region) bool {
    return (call(24, false, .{ .region = region }, null) catch unreachable) != 0;
}
pub fn releaseOwnerLeases(program: u32, generation: u64, task_id: u32, task_generation: u64) void {
    _ = call(22, false, .{ .owner = .{ .program = program, .program_generation = generation, .task = task_id, .task_generation = task_generation } }, null) catch unreachable;
}
pub fn ownerHasClaims(program: u32, generation: u64, task_id: u32, task_generation: u64) bool {
    return (call(23, false, .{ .owner = .{ .program = program, .program_generation = generation, .task = task_id, .task_generation = task_generation } }, null) catch unreachable) != 0;
}
