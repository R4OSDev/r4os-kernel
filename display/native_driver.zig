//! Temporary fixed projection to the sole native privileged display bridge.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const display = @import("display.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const queue = @import("queue.zig");
const resources = @import("queue_resources.zig");
pub const Error = @import("outputs.zig").Error || display.TransitionError || @import("native_additional_mode.zig").Error;
const Args = extern struct {
    owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    backend: abi.GfxBackendBinding = .{},
    generation: u64 = 0,
    held: u64 = 0,
    reset: u64 = 0,
    value: u32 = 0,
    other: u32 = 0,
    input: ?*const anyopaque = null,
    second: ?*const anyopaque = null,
};
extern fn r4os_rust_display_native_driver(u32, *const Args, ?*anyopaque) i32;
fn checked(value: i32) Error!void {
    return switch (value) {
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
        15 => error.Disabled,
        16 => error.AlreadyCompleted,
        17 => error.Bandwidth,
        18 => error.Routing,
        19 => error.Dependency,
        20 => error.DeviceLost,
        21 => error.RestoreFailed,
        else => unreachable,
    };
}
fn call(op: u32, args: Args) Error!void {
    try checked(r4os_rust_display_native_driver(op, &args, null));
}
fn result(comptime T: type, op: u32, args: Args) Error!T {
    var out: T = undefined;
    try checked(r4os_rust_display_native_driver(op, &args, &out));
    return out;
}
pub fn code(err: Error) i32 {
    return switch (err) {
        error.Busy => abi.gfx_output_error_busy,
        error.Stale, error.WrongOwner => abi.gfx_output_error_stale,
        error.Disabled, error.Unsupported => abi.gfx_output_error_unsupported,
        error.Unavailable => abi.gfx_output_error_unavailable,
        error.OutOfMemory, error.Capacity => abi.gfx_output_error_capacity,
        else => abi.gfx_output_error_invalid,
    };
}
pub fn bootInfo(output: *abi.GfxNativeBootInfo) i32 {
    return r4os_rust_display_native_driver(0, &Args{}, output);
}
pub fn retained(id: u32) bool {
    return result(bool, 1, .{ .value = id }) catch unreachable;
}
pub fn modePending() bool {
    return result(bool, 2, .{}) catch unreachable;
}
pub fn validAdapter(adapter: u32) bool {
    return result(bool, 3, .{ .value = adapter }) catch unreachable;
}
pub fn bootDescription(generation: u64, saved: *const display.BootSnapshot) abi.GfxNativeBootInfo {
    return result(abi.GfxNativeBootInfo, 4, .{ .generation = generation, .input = saved }) catch unreachable;
}
pub fn prepare(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, output: *abi.GfxNativeState) i32 {
    return r4os_rust_display_native_driver(5, &Args{ .owner = identity, .input = input }, output);
}
pub fn prepareHeld(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, generation: u64, output: *abi.GfxNativeState) i32 {
    return r4os_rust_display_native_driver(6, &Args{ .owner = identity, .input = input, .held = generation }, output);
}
pub fn prepareReset(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, held: u64, generation: u64, output: *abi.GfxNativeState) i32 {
    return r4os_rust_display_native_driver(7, &Args{ .owner = identity, .input = input, .held = held, .reset = generation }, output);
}
pub fn transition(id: u32, generation: u64, operation: u32, output: *abi.GfxNativeState) i32 {
    return r4os_rust_display_native_driver(8, &Args{ .value = id, .generation = generation, .other = operation }, output);
}
pub fn outputTarget(adapter: u32, head: u32) Error!abi.GfxOutputTarget {
    return result(abi.GfxOutputTarget, 9, .{ .value = adapter, .other = head });
}
const Image = extern struct { request: *const resources.NativeRequest, deadline: u64, frame_key: u64, dependencies: [*]const queue.model.Fence, dependencies_len: usize };
pub fn submitImage(caller: buffers.Owner, timeline: u64, submission: queue.model.Submission, request: resources.Request) queue.Error!queue.model.Status {
    const value = resources.NativeRequest.from(request);
    const image = Image{ .request = &value, .deadline = submission.deadline_ns, .frame_key = submission.frame_key, .dependencies = submission.dependencies.ptr, .dependencies_len = submission.dependencies.len };
    return result(queue.model.Status, 10, .{ .owner = caller, .generation = timeline, .input = &image }) catch |err| return switch (err) {
        error.Bandwidth, error.Routing, error.Dependency, error.Disabled, error.RestoreFailed => unreachable,
        else => |e| e,
    };
}
pub const ModeBinding = struct { driver: buffers.Owner, backend: abi.GfxBackendBinding, generation: u64, additional: ?@import("native_additional_mode.zig").Binding = null };
pub const CursorBinding = extern struct { driver: buffers.Owner, backend: abi.GfxBackendBinding, generation: u64, head: u32, timeline: u64 = 0, point: u64 = 0 };
const AdditionalBinding = @import("output_runtime.zig").ModeBinding;
const NativeModeBinding = extern struct { driver: buffers.Owner, backend: abi.GfxBackendBinding, generation: u64, has_additional: bool, additional: AdditionalBinding };
fn wire(value: ModeBinding) NativeModeBinding {
    return .{ .driver = value.driver, .backend = value.backend, .generation = value.generation, .has_additional = value.additional != null, .additional = value.additional orelse .{ .driver = .{ .kind = .kernel, .id = 0, .generation = 0 }, .backend = .{}, .target = .{}, .width = 0, .height = 0, .active = false } };
}
fn binding(value: NativeModeBinding) ModeBinding {
    return .{ .driver = value.driver, .backend = value.backend, .generation = value.generation, .additional = if (value.has_additional) value.additional else null };
}
pub fn cursorBinding() Error!CursorBinding {
    return result(CursorBinding, 20, .{});
}
pub fn enableModes(identity: buffers.Owner, backend: abi.GfxBackendBinding) Error!void {
    try call(11, .{ .owner = identity, .backend = backend });
}
pub fn modesEnabled(id: u32, backend: abi.GfxBackendBinding) bool {
    return result(bool, 12, .{ .value = id, .backend = backend }) catch unreachable;
}
pub fn modeBinding(assignment: abi.GfxScanoutState) Error!ModeBinding {
    return binding(try result(NativeModeBinding, 13, .{ .input = &assignment }));
}
const Prepare = extern struct { assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode, has_format: bool, format: u32 };
pub fn prepareMode(caller: buffers.Owner, assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode) Error!abi.GfxBufferReference {
    return prepareModeEncoding(caller, assignment, mode, null);
}
pub fn prepareModeEncoding(caller: buffers.Owner, assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode, format: ?u32) Error!abi.GfxBufferReference {
    const input = Prepare{ .assignment = assignment, .mode = mode, .has_format = format != null, .format = format orelse 0 };
    return result(abi.GfxBufferReference, 14, .{ .owner = caller, .input = &input });
}
pub fn startMode(operation: u32, expected: ModeBinding) Error!void {
    const input = wire(expected);
    try call(15, .{ .value = operation, .input = &input });
}
pub fn settleMode(operation: u32, outcome: u32) Error!void {
    try call(16, .{ .value = operation, .other = outcome });
}
pub fn settleModeFor(operation: u32, outcome: u32, expected: ModeBinding) Error!void {
    const input = wire(expected);
    try call(17, .{ .value = operation, .other = outcome, .input = &input });
}
pub fn abortPreparedMode(expected: ModeBinding) void {
    const input = wire(expected);
    call(18, .{ .input = &input }) catch unreachable;
}
pub fn retireModeAfterReset(identity: buffers.Owner, backend: abi.GfxBackendBinding) Error!void {
    try call(19, .{ .owner = identity, .backend = backend });
}
pub fn deviceReset(identity: buffers.Owner, input: *const abi.GfxBackendBinding, generation: u64, quiesced: u32, output: *abi.GfxNativeState) i32 {
    return r4os_rust_display_native_driver(21, &Args{ .owner = identity, .input = input, .generation = generation, .value = quiesced }, output);
}
pub fn resumeHeadless(identity: buffers.Owner, input: *const abi.GfxBackendBinding, generation: u64, output: *abi.GfxNativeState) i32 {
    return r4os_rust_display_native_driver(22, &Args{ .owner = identity, .input = input, .generation = generation }, output);
}
pub fn releaseTerminal(identity: buffers.Owner, input: *const abi.GfxBackendBinding, generation: u64, output: *abi.GfxNativeState) i32 {
    return r4os_rust_display_native_driver(23, &Args{ .owner = identity, .input = input, .generation = generation }, output);
}
export fn r4os_zig_native_display_log(bytes: [*]const u8, len: usize) callconv(.c) void {
    @import("../kernel/log.zig").serialWriteRaw(bytes[0..len]);
    @import("../kernel/bootlog.zig").puts(bytes[0..len]);
}
extern fn r4os_rust_native_driver_fixture(u32, *const Args) i32;
fn ownsRestoredConsole(identity: buffers.Owner, generation: u64, adapter: u32) bool {
    return r4os_rust_native_driver_fixture(4, &Args{ .owner = identity, .generation = generation, .value = adapter }) != 0;
}
pub fn checkConsoleRestoreForTest(id: u32, generation: u64, adapter: u32) !void {
    std.debug.assert(@import("builtin").is_test);
    const t = std.testing;
    _ = r4os_rust_native_driver_fixture(0, &Args{});
    defer _ = r4os_rust_native_driver_fixture(1, &Args{});
    const identity: buffers.Owner = .{ .kind = .driver, .id = id, .generation = 19 };
    _ = r4os_rust_native_driver_fixture(2, &Args{ .owner = identity, .generation = generation, .value = adapter });
    try checked(r4os_rust_native_driver_fixture(3, &Args{ .value = id, .generation = generation }));
    const restored = display.backendState().generation;
    try t.expect(r4os_rust_native_driver_fixture(5, &Args{}) != 0 and ownsRestoredConsole(identity, restored, adapter));
    try t.expect(!ownsRestoredConsole(.{ .kind = .driver, .id = id, .generation = 20 }, restored, adapter));
    try t.expect(!ownsRestoredConsole(.{ .kind = .driver, .id = id + 1, .generation = 19 }, restored, adapter));
    try t.expect(!ownsRestoredConsole(identity, restored - 1, adapter));
    try t.expect(!ownsRestoredConsole(identity, restored, adapter + 1));
}

comptime {
    if (@sizeOf(Args) != 104 or @alignOf(Args) != 8) @compileError("Native display Args layout");
    if (@offsetOf(Args, "backend") != 24 or @offsetOf(Args, "generation") != 56 or @offsetOf(Args, "held") != 64 or @offsetOf(Args, "reset") != 72 or @offsetOf(Args, "value") != 80 or @offsetOf(Args, "other") != 84 or @offsetOf(Args, "input") != 88 or @offsetOf(Args, "second") != 96) @compileError("Native display Args offsets");
    if (@sizeOf(CursorBinding) != 88 or @sizeOf(Image) != 40) @compileError("Native display binding/image layout");
}
