//! Fixed projection of the sole native output catalog/publication runtime.
pub const model = @import("output_state.zig");
pub const abi = model.abi;
const buffers = @import("../memory/gfx_buffers.zig");
const queue = @import("queue.zig");
pub const Error = model.Error || queue.Error;
pub const NativeAdmission = extern struct { ticket: model.Ticket, mode: abi.GfxOutputMode, revision: u64 };
const Frame = @import("framebuffer.zig").Framebuffer;
const Args = extern struct {
    owner: u32 = 0,
    actor: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 },
    identity: abi.GfxOutputId = .{},
    target: abi.GfxOutputTarget = .{},
    binding: abi.GfxBackendBinding = .{},
    index: u32 = 0,
    head: u32 = 0,
    width: u32 = 0,
    height: u32 = 0,
    value: u64 = 0,
    other: u64 = 0,
    ticket: model.Ticket = .{},
    operation: u32 = 0,
    outcome: u32 = 0,
    quiesced: u32 = 0,
    flag: bool = false,
    other_flag: bool = false,
    input: ?*const anyopaque = null,
    frame: ?*const Frame = null,
};
const OptionalInfo = extern struct { has: bool, value: abi.GfxOutputInfo };
const OptionalMode = extern struct { has: bool, value: abi.GfxOutputMode };
const OptionalEdid = extern struct { has: bool, value: abi.GfxEdidBlock };
const OptionalAudio = extern struct { has: bool, value: abi.GfxAudioRoute };
const OptionalId = extern struct { has: bool, value: abi.GfxOutputId };
extern fn r4os_rust_display_outputs(u32, *const Args, ?*anyopaque) i32;
fn status(code: i32) Error!void {
    return switch (code) {
        0 => {},
        1 => error.Invalid,
        2 => error.Overflow,
        3 => error.Unsupported,
        4 => error.Exhausted,
        5 => error.Budget,
        6 => error.Capacity,
        7 => error.Stale,
        8 => error.Busy,
        9 => error.Closed,
        10 => error.WrongOwner,
        11 => error.OutOfMemory,
        12 => error.Unavailable,
        13 => error.WaitTimeout,
        14 => error.WaitCancelled,
        16 => error.AlreadyCompleted,
        17 => error.Bandwidth,
        18 => error.Routing,
        19 => error.Dependency,
        20 => error.DeviceLost,
        else => unreachable,
    };
}
fn call(op: u32, args: Args) Error!void {
    try status(r4os_rust_display_outputs(op, &args, null));
}
fn result(comptime T: type, op: u32, args: Args) Error!T {
    var out: T = undefined;
    try status(r4os_rust_display_outputs(op, &args, &out));
    return out;
}
pub fn initBoot(frame: *const Frame) void {
    call(0, .{ .frame = frame }) catch unreachable;
}
pub fn revision() abi.GfxDisplayRevision {
    return result(abi.GfxDisplayRevision, 1, .{}) catch unreachable;
}
pub fn infoAt(index: u32) ?abi.GfxOutputInfo {
    const out = result(OptionalInfo, 2, .{ .index = index }) catch unreachable;
    return if (out.has) out.value else null;
}
pub fn colorAt(identity: abi.GfxOutputId) Error!abi.GfxOutputColorState {
    return result(abi.GfxOutputColorState, 3, .{ .identity = identity });
}
pub fn publishColor(owner: u32, value: abi.GfxOutputColorState) Error!void {
    try call(4, .{ .owner = owner, .input = &value });
}
pub fn refreshAt(target: abi.GfxOutputTarget) Error!abi.GfxOutputRefresh {
    return result(abi.GfxOutputRefresh, 5, .{ .target = target });
}
pub fn publishRefresh(owner: u32, value: abi.GfxOutputRefresh) Error!void {
    try call(6, .{ .owner = owner, .input = &value });
}
pub fn requestRefresh(actor: buffers.Owner, input: abi.GfxRefreshRequest) Error!abi.GfxRefreshRequest {
    return result(abi.GfxRefreshRequest, 7, .{ .actor = actor, .input = &input });
}
pub fn readRefresh(owner: u32, target: abi.GfxOutputTarget) Error!abi.GfxRefreshRequest {
    return result(abi.GfxRefreshRequest, 8, .{ .owner = owner, .target = target });
}
pub fn refreshStopped(actor: buffers.Owner) void {
    call(9, .{ .actor = actor }) catch unreachable;
}
pub fn powerAt(identity: abi.GfxOutputId) Error!abi.GfxOutputPower {
    return result(abi.GfxOutputPower, 10, .{ .identity = identity });
}
pub fn publishPower(owner: u32, value: abi.GfxOutputPower) Error!void {
    try call(11, .{ .owner = owner, .input = &value });
}
pub fn requestPower(actor: buffers.Owner, input: abi.GfxPowerRequest) Error!abi.GfxPowerRequest {
    return result(abi.GfxPowerRequest, 12, .{ .actor = actor, .input = &input });
}
pub fn readPower(owner: u32, identity: abi.GfxOutputId) Error!abi.GfxPowerRequest {
    return result(abi.GfxPowerRequest, 13, .{ .owner = owner, .identity = identity });
}
pub fn powerStopped(actor: buffers.Owner) void {
    call(14, .{ .actor = actor }) catch unreachable;
}
pub fn modeAt(identity: abi.GfxOutputId, index: u32) Error!?abi.GfxOutputMode {
    const out = try result(OptionalMode, 15, .{ .identity = identity, .index = index });
    return if (out.has) out.value else null;
}
pub fn edidAt(identity: abi.GfxOutputId, index: u32) Error!?abi.GfxEdidBlock {
    const out = try result(OptionalEdid, 16, .{ .identity = identity, .index = index });
    return if (out.has) out.value else null;
}
pub fn publish(owner: u32, input: *const abi.GfxOutputPublication) Error!abi.GfxOutputId {
    return result(abi.GfxOutputId, 17, .{ .owner = owner, .input = input });
}
pub fn withdraw(owner: u32, identity: abi.GfxOutputId) Error!void {
    try call(18, .{ .owner = owner, .identity = identity });
}
pub fn pause(owner: u32, identity: abi.GfxOutputId, paused: bool) Error!void {
    try call(19, .{ .owner = owner, .identity = identity, .flag = paused });
}
pub fn nativePaused(owner: u32, identity: abi.GfxOutputId) bool {
    return result(bool, 20, .{ .owner = owner, .identity = identity }) catch unreachable;
}
pub fn receiverEpoch() u64 {
    return result(u64, 21, .{}) catch unreachable;
}
pub fn registerReceiverSource(owner: buffers.Owner, adapter: u32, epoch: u64) Error!abi.GfxReceiverSource {
    return result(abi.GfxReceiverSource, 22, .{ .actor = owner, .index = adapter, .value = epoch });
}
pub fn replaceReceivers(owner: buffers.Owner, input: *const abi.GfxReceiverUpdate) Error!void {
    try call(23, .{ .actor = owner, .input = input });
}
pub fn closeReceiverSource(owner: buffers.Owner, binding: abi.GfxReceiverSource) Error!void {
    try call(24, .{ .actor = owner, .input = &binding });
}
pub fn publishAudio(owner: buffers.Owner, input: *const abi.GfxAudioRoute) Error!void {
    try call(25, .{ .actor = owner, .input = input });
}
pub fn queryAudio(location: u32, device: u32, index: u32) ?abi.GfxAudioRoute {
    const out = result(OptionalAudio, 26, .{ .owner = location, .head = device, .index = index }) catch unreachable;
    return if (out.has) out.value else null;
}
pub fn stoppedDriver(owner: u32) void {
    call(27, .{ .owner = owner }) catch unreachable;
}
pub fn bootActive(active: bool) void {
    call(28, .{ .flag = active }) catch unreachable;
}
pub fn validateNative(owner: u32, backend: abi.GfxBackendBinding, identity: abi.GfxOutputId, width: u32, height: u32, held: bool) Error!void {
    try call(29, .{ .owner = owner, .binding = backend, .identity = identity, .width = width, .height = height, .flag = held });
}
pub fn cursorHead(owner: u32, identity: abi.GfxOutputId) Error!u32 {
    return result(u32, 30, .{ .owner = owner, .identity = identity });
}
pub fn activeTarget(owner: u32, identity: abi.GfxOutputId, generation: u64) error{Stale}!abi.GfxOutputTarget {
    return result(abi.GfxOutputTarget, 31, .{ .owner = owner, .identity = identity, .value = generation }) catch |err| {
        if (err == error.Stale) return error.Stale;
        unreachable;
    };
}
pub fn activeTargetLocked(owner: u32, identity: abi.GfxOutputId, generation: u64) error{Stale}!abi.GfxOutputTarget {
    return result(abi.GfxOutputTarget, 32, .{ .owner = owner, .identity = identity, .value = generation }) catch |err| {
        if (err == error.Stale) return error.Stale;
        unreachable;
    };
}
pub fn validateTargetLocked(owner: u32, target: abi.GfxOutputTarget) queue.Error!void {
    call(33, .{ .owner = owner, .target = target }) catch |err| {
        if (err == error.Stale) return error.Stale;
        unreachable;
    };
}
pub fn registeredHeadLocked(owner: u32, identity: abi.GfxOutputId, head: u32) Error!void {
    try call(34, .{ .owner = owner, .identity = identity, .head = head });
}
pub fn validateAdditionalLocked(owner: u32, request: abi.GfxAdditionalOutput) Error!void {
    try call(35, .{ .owner = owner, .input = &request });
}
pub fn validateAdditionalActivationLocked(owner: u32, identity: abi.GfxOutputId, head: u32, width: u32, height: u32) Error!void {
    try call(36, .{ .owner = owner, .identity = identity, .head = head, .width = width, .height = height });
}
pub fn pauseAdditionalLocked(owner: u32, identity: abi.GfxOutputId, paused: bool) Error!bool {
    return result(bool, 37, .{ .owner = owner, .identity = identity, .flag = paused });
}
pub fn currentIdentityLocked(owner: u32, identity: abi.GfxOutputId) ?abi.GfxOutputId {
    const out = result(OptionalId, 38, .{ .owner = owner, .identity = identity }) catch unreachable;
    return if (out.has) out.value else null;
}
pub fn connectedLocked(owner: u32, identity: abi.GfxOutputId) bool {
    return result(bool, 39, .{ .owner = owner, .identity = identity }) catch unreachable;
}
pub fn forgetNativePortLocked(owner: u32, identity: abi.GfxOutputId) void {
    call(40, .{ .owner = owner, .identity = identity }) catch unreachable;
}
pub fn nativeActive(owner: u32, identity: abi.GfxOutputId, active: bool) void {
    call(41, .{ .owner = owner, .identity = identity, .flag = active }) catch unreachable;
}
pub fn setNativeActiveLocked(owner: u32, identity: abi.GfxOutputId, head: u32, active: bool) Error!bool {
    return result(bool, 42, .{ .owner = owner, .identity = identity, .head = head, .flag = active });
}
pub fn atomic(owner: buffers.Owner, input: *const abi.GfxAtomicState, commit: bool) Error!abi.GfxAtomicResult {
    return result(abi.GfxAtomicResult, 43, .{ .actor = owner, .input = input, .flag = commit });
}
pub fn nativeMode(owner: buffers.Owner, input: *const abi.GfxAtomicState) Error!abi.GfxOutputMode {
    return result(abi.GfxOutputMode, 44, .{ .actor = owner, .input = input });
}
pub fn beginNative(owner: buffers.Owner, input: *const abi.GfxAtomicState) Error!NativeAdmission {
    return result(NativeAdmission, 45, .{ .actor = owner, .input = input });
}
pub fn finishNative(ticket: model.Ticket, operation: u32, outcome: u32, quiesced: u32) Error!abi.GfxAtomicResult {
    return result(abi.GfxAtomicResult, 46, .{ .ticket = ticket, .operation = operation, .outcome = outcome, .quiesced = quiesced });
}
pub fn canFinishNative(ticket: model.Ticket, operation: u32) Error!void {
    try call(47, .{ .ticket = ticket, .operation = operation });
}
pub fn retireNativeAfterReset(ticket: model.Ticket) Error!void {
    try call(48, .{ .ticket = ticket });
}
pub fn brightnessAt(identity: abi.GfxOutputId) Error!abi.GfxOutputBrightness {
    return result(abi.GfxOutputBrightness, 49, .{ .identity = identity });
}
pub fn publishBrightness(owner: u32, value: abi.GfxOutputBrightness) Error!void {
    try call(50, .{ .owner = owner, .input = &value });
}
pub fn requestBrightness(actor: buffers.Owner, input: abi.GfxBrightnessRequest) Error!abi.GfxBrightnessRequest {
    return result(abi.GfxBrightnessRequest, 51, .{ .actor = actor, .input = &input });
}
pub fn readBrightness(owner: u32, identity: abi.GfxOutputId) Error!abi.GfxBrightnessRequest {
    return result(abi.GfxBrightnessRequest, 52, .{ .owner = owner, .identity = identity });
}
comptime {
    if (@sizeOf(Args) != 208 or @offsetOf(Args, "actor") != 8 or @offsetOf(Args, "binding") != 104 or @offsetOf(Args, "input") != 192 or @offsetOf(Args, "frame") != 200 or @sizeOf(NativeAdmission) != 80 or @offsetOf(NativeAdmission, "revision") != 72) @compileError("Native output runtime C layout");
}

// Temporary typed peers to the still-Zig queue/display/mode/cursor runtime
// owners. They own no catalog/epoch state and are removed as those owners move.
fn peerCode(err: anyerror) i32 {
    return switch (err) {
        error.Invalid => 1,
        error.Overflow => 2,
        error.Unsupported => 3,
        error.Exhausted => 4,
        error.Budget => 5,
        error.Capacity => 6,
        error.Stale => 7,
        error.Busy => 8,
        error.Closed => 9,
        error.WrongOwner => 10,
        error.OutOfMemory => 11,
        error.Unavailable => 12,
        error.WaitTimeout => 13,
        error.WaitCancelled => 14,
        error.Disabled => 15,
        error.AlreadyCompleted => 16,
        error.Bandwidth => 17,
        error.Routing => 18,
        error.Dependency => 19,
        error.DeviceLost => 20,
        error.RestoreFailed => 21,
        else => unreachable,
    };
}
