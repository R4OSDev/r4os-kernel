const std = @import("std");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}

// The kernel and its R4D callbacks use the AMD64 System V ABI on either build
// host. This is one ordinary call/return boundary, not a returns-twice setjmp
// contract inferred by the Zig optimizer. Admission belongs to driver_threads.
pub const Handler = *const fn (usize) callconv(.{ .x86_64_sysv = .{} }) i32;
pub const Frame = extern struct {
    rsp: u64 = 0,
    rbx: u64 = 0,
    rbp: u64 = 0,
    r12: u64 = 0,
    r13: u64 = 0,
    r14: u64 = 0,
    r15: u64 = 0,
    mxcsr: u32 = 0,
    x87_control: u16 = 0,
    reserved: u16 = 0,
};
comptime {
    if (@sizeOf(Frame) != 64 or @offsetOf(Frame, "mxcsr") != 56 or @offsetOf(Frame, "x87_control") != 60)
        @compileError("callback abort assembly layout drift");
}

extern fn r4os_rust_callback_invoke(handler: Handler, context: usize, frame: *Frame) callconv(.{ .x86_64_sysv = .{} }) i32;
extern fn r4os_rust_callback_resume(frame: *const Frame, result: i32) callconv(.{ .x86_64_sysv = .{} }) noreturn;
pub fn invoke(handler: Handler, context: usize, frame: *Frame) i32 {
    return r4os_rust_callback_invoke(handler, context, frame);
}
// Same current-task live frame and fully returned kernel owners as before.
pub fn returnToCaller(frame: *const Frame, result: i32) noreturn {
    r4os_rust_callback_resume(frame, result);
}

const Probe = struct {
    frame: Frame = .{},
    result: i32,
    calls: usize = 0,
    returned: bool = false,
    deferred: bool = false,
};
noinline fn deepAbort(value: *Probe, depth: usize) i32 {
    var canary: [137]u8 = undefined;
    const bytes: *volatile [137]u8 = &canary;
    for (0..137) |i| bytes[i] = @truncate(depth + i);
    value.calls += 1;
    if (depth == 0) returnToCaller(&value.frame, value.result);
    const result = deepAbort(value, depth - 1);
    for (0..137) |i| std.debug.assert(bytes[i] == @as(u8, @truncate(depth + i)));
    return result;
}
fn aborting(context: usize) callconv(.{ .x86_64_sysv = .{} }) i32 {
    const value: *Probe = @ptrFromInt(context);
    defer value.deferred = true;
    const result = deepAbort(value, 12);
    value.returned = true;
    return result;
}
fn normal(context: usize) callconv(.{ .x86_64_sysv = .{} }) i32 {
    const value: *Probe = @ptrFromInt(context);
    value.calls += 1;
    return value.result;
}
test "callback abort restores a live SysV call frame without pretending to run defers" {
    var caller_canary: [257]u64 = @splat(0xdeadbeef12345678);
    const observed: *volatile [257]u64 = &caller_canary;
    for ([_]i32{ -1, -76001, std.math.minInt(i32), std.math.maxInt(i32), 0 }) |result| {
        var value = Probe{ .result = result };
        try std.testing.expectEqual(result, invoke(normal, @intFromPtr(&value), &value.frame));
        try std.testing.expectEqual(@as(usize, 1), value.calls);
        value.calls = 0;
        try std.testing.expectEqual(result, invoke(aborting, @intFromPtr(&value), &value.frame));
        try std.testing.expectEqual(@as(usize, 13), value.calls);
        try std.testing.expect(!value.returned and !value.deferred);
        for (0..257) |i| try std.testing.expectEqual(@as(u64, 0xdeadbeef12345678), observed[i]);
    }
}

fn readControls() Frame {
    var frame: Frame = .{};
    asm volatile (
        \\ stmxcsr 56(%[frame])
        \\ fnstcw 60(%[frame])
        :
        : [frame] "r" (&frame),
        : .{ .memory = true });
    return frame;
}
const ControlProbe = struct { frame: Frame = .{}, abort: bool };
fn changeControls(context: usize) callconv(.{ .x86_64_sysv = .{} }) i32 {
    const probe: *ControlProbe = @ptrFromInt(context);
    var changed = probe.frame;
    changed.mxcsr ^= 0x6000;
    changed.x87_control ^= 0x0c00;
    asm volatile (
        \\ ldmxcsr 56(%[frame])
        \\ fldcw 60(%[frame])
        :
        : [frame] "r" (&changed),
        : .{ .memory = true });
    if (probe.abort) returnToCaller(&probe.frame, -1);
    return 1;
}
test "callback boundary restores MXCSR and x87 control after normal and aborted execution" {
    const before = readControls();
    for ([_]bool{ false, true }) |aborting_call| {
        var value = ControlProbe{ .abort = aborting_call };
        const result = invoke(changeControls, @intFromPtr(&value), &value.frame);
        const after = readControls();
        try std.testing.expectEqual(@as(i32, if (aborting_call) -1 else 1), result);
        try std.testing.expectEqual(before.mxcsr, after.mxcsr);
        try std.testing.expectEqual(before.x87_control, after.x87_control);
    }
}
