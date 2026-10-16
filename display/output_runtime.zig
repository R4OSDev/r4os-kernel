//! Fixed projection of the sole native additional-output runtime.
const abi = @import("r4os_kernel_contract");
const buffers = @import("../memory/gfx_buffers.zig");
const queue = @import("queue.zig");
const resources = @import("queue_resources.zig");
pub const Error = @import("outputs.zig").Error || @import("output_runtime_state.zig").Error;
pub const ModeBinding = extern struct {
    driver: buffers.Owner,
    backend: abi.GfxBackendBinding,
    target: abi.GfxOutputTarget,
    width: u32,
    height: u32,
    active: bool,
};
const empty_binding: ModeBinding = .{ .driver = .{ .kind = .kernel, .id = 0, .generation = 0 }, .backend = .{}, .target = .{}, .width = 0, .height = 0, .active = false };
const Args = extern struct { owner: buffers.Owner = .{ .kind = .kernel, .id = 0, .generation = 0 }, target: abi.GfxOutputTarget = .{},
    backend: abi.GfxBackendBinding = .{}, binding: ModeBinding = empty_binding, timeline: u64 = 0, value: u32 = 0, other: u32 = 0,
    width: u32 = 0, height: u32 = 0, flag: bool = false, lost: bool = false, input: ?*const anyopaque = null };
extern fn r4os_rust_display_additional_runtime(u32, *const Args, ?*anyopaque) i32;
fn checked(value: i32) Error!void { return switch (value) {
    0 => {}, 1 => error.Invalid, 2 => error.Overflow, 3 => error.Unsupported, 4 => error.Exhausted, 5 => error.Budget, 6 => error.Capacity,
    7 => error.Stale, 8 => error.Busy, 9 => error.Closed, 10 => error.WrongOwner, 11 => error.OutOfMemory, 12 => error.Unavailable,
    13 => error.WaitTimeout, 14 => error.WaitCancelled, 16 => error.AlreadyCompleted, 17 => error.Bandwidth,
    18 => error.Routing, 19 => error.Dependency, 20 => error.DeviceLost, else => unreachable,
}; }



fn call(op: u32, args: Args) Error!void { try checked(r4os_rust_display_additional_runtime(op, &args, null)); }
fn result(comptime T: type, op: u32, args: Args) Error!T { var out: T = undefined; try checked(r4os_rust_display_additional_runtime(op, &args, &out)); return out; }
pub fn register(driver: buffers.Owner, request: abi.GfxAdditionalOutput) Error!abi.GfxOutputTarget { return result(abi.GfxOutputTarget, 0, .{ .owner = driver, .input = &request }); }
pub fn transition(driver: buffers.Owner, target: abi.GfxOutputTarget, operation: u32, quiesced: u32) Error!void { try call(1, .{ .owner = driver, .target = target, .value = operation, .other = quiesced }); }
const OptionalTarget = extern struct { has: bool, value: abi.GfxOutputTarget };
pub fn targetAt(adapter: u32, head: u32) Error!?abi.GfxOutputTarget { const out = try result(OptionalTarget, 2, .{ .value = adapter, .other = head }); return if (out.has) out.value else null; }
pub fn contains(target: abi.GfxOutputTarget) bool { return result(bool, 3, .{ .target = target }) catch unreachable; }
const Image = extern struct { request: *const resources.NativeRequest, deadline: u64, frame_key: u64, dependencies: [*]const queue.model.Fence, dependencies_len: usize };
pub fn submitImage(caller: buffers.Owner, timeline: u64, submission: queue.model.Submission, request: resources.Request) queue.Error!queue.model.Status {
    const retained = resources.NativeRequest.from(request);
    const input: Image = .{ .request = &retained, .deadline = submission.deadline_ns, .frame_key = submission.frame_key, .dependencies = submission.dependencies.ptr, .dependencies_len = submission.dependencies.len };
    return result(queue.model.Status, 4, .{ .owner = caller, .timeline = timeline, .input = &input }) catch |err| switch (err) { error.Bandwidth, error.Routing, error.Dependency => unreachable, else => |failure| return failure };
}
pub fn info(target: abi.GfxOutputTarget) Error!abi.DisplayPresentationInfo { return result(abi.DisplayPresentationInfo, 5, .{ .target = target }); }
pub fn feedback(target: abi.GfxOutputTarget, source_fence: abi.GfxFence) Error!abi.DisplayPresentationStats { return result(abi.DisplayPresentationStats, 6, .{ .target = target, .input = &source_fence }); }
pub fn modeBinding(assignment: abi.GfxScanoutState) Error!ModeBinding { return result(ModeBinding, 7, .{ .input = &assignment }); }
pub fn beginMode(expected: ModeBinding) Error!void { try call(8, .{ .binding = expected }); }
pub fn finishMode(expected: ModeBinding, width: u32, height: u32, active: bool, lost: bool) Error!void { try call(9, .{ .binding = expected, .width = width, .height = height, .flag = active, .lost = lost }); }
pub fn publishInfo(driver: buffers.Owner, value: abi.DisplayPresentationInfo) Error!bool { return result(bool, 10, .{ .owner = driver, .input = &value }); }
pub fn publishStats(driver: buffers.Owner, value: abi.DisplayPresentationStats) Error!bool { return result(bool, 11, .{ .owner = driver, .input = &value }); }
pub fn stoppedDriver(id: u32) void { call(12, .{ .owner = .{ .kind = .driver, .id = id, .generation = 0 } }) catch unreachable; }
pub fn retireAfterReset(driver: buffers.Owner, backend: abi.GfxBackendBinding) Error!void { try call(13, .{ .owner = driver, .backend = backend }); }

// Typed, nonblocking peers to the remaining queue runtime. No output state.





fn peerCode(err: anyerror) i32 { return switch (err) {
    error.Invalid => 1, error.Overflow => 2, error.Unsupported => 3, error.Exhausted => 4, error.Budget => 5, error.Capacity => 6,
    error.Stale => 7, error.Busy => 8, error.Closed => 9, error.WrongOwner => 10, error.OutOfMemory => 11, error.Unavailable => 12,
    error.WaitTimeout => 13, error.WaitCancelled => 14, error.Disabled => 15, error.AlreadyCompleted => 16, error.Bandwidth => 17,
    error.Routing => 18, error.Dependency => 19, error.DeviceLost => 20, error.RestoreFailed => 21, else => unreachable,
}; }
