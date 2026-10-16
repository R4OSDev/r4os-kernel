// Native canonical fact model; caller owns this exact C storage.
const std = @import("std");
const a = @import("r4os_kernel_contract");
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted };
extern fn r4os_rust_input_platform_model(*State, u8, u32, u64, u32, u32, u64) i32;
fn result(code: i32) Error!bool {
    return switch (code) {
        0 => false,
        1 => true,
        -1 => error.Invalid,
        -2 => error.Stale,
        -3 => error.Busy,
        -4 => error.Unsupported,
        -5 => error.Exhausted,
        else => unreachable,
    };
}
pub const State = extern struct {
    value: a.PlatformInputSnapshot = .{},
    owner: u32 = 0,
    epoch: u64 = 0,
    acpi_caps: u32 = 0,
    usb_present: bool = false,
    pub fn submit(self: *State, owner: u32, epoch: u64, kind: u32, value: u32, now: u64) Error!bool {
        return result(r4os_rust_input_platform_model(self, 0, owner, epoch, kind, value, now));
    }
    pub fn usb(self: *State, present: bool, now: u64) Error!bool {
        return result(r4os_rust_input_platform_model(self, 1, 0, 0, 0, @intFromBool(present), now));
    }
    pub fn consumer(self: *State, usage: u32, now: u64) Error!bool {
        return result(r4os_rust_input_platform_model(self, 2, 0, 0, 0, usage, now));
    }
    pub fn remove(self: *State, owner: u32, now: u64) Error!bool {
        return result(r4os_rust_input_platform_model(self, 3, owner, 0, 0, 0, now));
    }
};
comptime {
    if (@sizeOf(State) != 88 or @alignOf(State) != 8 or @offsetOf(State, "owner") != 64 or @offsetOf(State, "epoch") != 72 or @offsetOf(State, "acpi_caps") != 80 or @offsetOf(State, "usb_present") != 84) @compileError("Native platform input State layout");
}

test "platform input preserves independent counters and rejects stale producers and invented events" {
    const t = std.testing;
    var s: State = .{};
    try t.expectError(error.Stale, s.submit(1, 4, 1, 1, 1));
    try t.expect(try s.submit(1, 4, 4, 3, 2));
    try t.expectError(error.Busy, s.submit(2, 4, 4, 3, 3));
    try t.expectError(error.Stale, s.submit(1, 5, 1, 1, 3));
    try t.expect(try s.submit(1, 4, 3, 2, 4));
    const closed = s.value;
    try t.expect(!try s.submit(1, 4, 3, 2, 5));
    try t.expectEqualDeep(closed, s.value);
    try t.expect(try s.submit(1, 4, 1, 1, 6));
    try t.expect(try s.usb(true, 7));
    try t.expect(try s.consumer(0x70, 8));
    try t.expectError(error.Unsupported, s.consumer(0xe9, 9));
    try t.expect(s.value.brightness_up == 1 and s.value.brightness_down == 1 and s.value.sources == 3);
    try t.expect(!try s.remove(2, 10));
    try t.expect(try s.remove(1, 10));
    try t.expect(s.value.lid_state == 0 and s.value.lid_sequence > closed.lid_sequence and s.value.capabilities == 1 and s.value.sources == 2);
    try t.expect(try s.submit(1, 5, 4, 2, 11));
    try t.expectError(error.Unsupported, s.submit(1, 5, 1, 1, 12));
    try t.expectError(error.Stale, s.submit(1, 4, 3, 1, 12));
    try t.expect(try s.submit(1, 5, 3, 1, 12));
    s.value.sequence = std.math.maxInt(u64);
    const exhausted = s.value;
    try t.expectError(error.Exhausted, s.consumer(0x6f, 13));
    try t.expectEqualDeep(exhausted, s.value);
}
