// Canonical platform input facts. Hardware decoding and screen policy are
// external; consumers independently observe counters without stealing events.
const std = @import("std");
const a = @import("r4os_kernel_contract");
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Exhausted };
pub const State = struct {
    value: a.PlatformInputSnapshot = .{},
    owner: u32 = 0,
    epoch: u64 = 0,
    acpi_caps: u32 = 0,
    usb_present: bool = false,
    fn advance(self: *State, now: u64) Error!void {
        if (now == 0 or now == std.math.maxInt(u64) or now < self.value.since_ns) return error.Invalid;
        if (self.value.sequence == std.math.maxInt(u64)) return error.Exhausted;
        self.value.sequence += 1; self.value.since_ns = now;
    }
    fn capabilities(self: *State) void {
        self.value.capabilities = self.acpi_caps | @as(u32, @intFromBool(self.usb_present));
        self.value.sources = @as(u32, @intFromBool(self.owner != 0)) | (@as(u32, @intFromBool(self.usb_present)) << 1);
    }
    pub fn submit(self: *State, owner: u32, epoch: u64, kind: u32, value: u32, now: u64) Error!bool {
        if (owner == 0 or epoch == 0 or kind == 0 or kind > 4) return error.Invalid;
        if (kind == 4) {
            if (value > 3) return error.Invalid;
            if (self.owner != 0 and (owner != self.owner or epoch != self.epoch)) return error.Busy;
            if (value == 0) return self.remove(owner, now);
            if (self.owner == owner and self.epoch == epoch and self.acpi_caps == value) return false;
            try self.advance(now);
            self.owner = owner; self.epoch = epoch; self.acpi_caps = value;
            if (value & 2 == 0 and self.value.lid_state != 0) {
                self.value.lid_state = 0; self.value.lid_sequence = self.value.sequence;
            }
            self.capabilities(); return true;
        }
        if (self.owner != owner or self.epoch != epoch) return error.Stale;
        if (kind == 3) {
            if (value > 2) return error.Invalid;
            if (self.acpi_caps & 2 == 0) return error.Unsupported;
            if (self.value.lid_state == value) return false;
            try self.advance(now); self.value.lid_state = value; self.value.lid_sequence = self.value.sequence;
            return true;
        }
        if (self.acpi_caps & 1 == 0) return error.Unsupported;
        return self.button(kind, value, now);
    }
    fn button(self: *State, kind: u32, value: u32, now: u64) Error!bool {
        if (value != 1 or (kind != 1 and kind != 2)) return error.Invalid;
        const count = if (kind == 1) &self.value.brightness_up else &self.value.brightness_down;
        if (count.* == std.math.maxInt(u64)) return error.Exhausted;
        try self.advance(now); count.* += 1; return true;
    }
    pub fn usb(self: *State, present: bool, now: u64) Error!bool {
        if (self.usb_present == present) return false;
        try self.advance(now); self.usb_present = present; self.capabilities(); return true;
    }
    pub fn consumer(self: *State, usage: u32, now: u64) Error!bool {
        if (!self.usb_present) return error.Unsupported;
        return self.button(switch (usage) { 0x6f => 1, 0x70 => 2, else => return error.Unsupported }, 1, now);
    }
    pub fn remove(self: *State, owner: u32, now: u64) Error!bool {
        if (self.owner == 0 or owner != self.owner) return false;
        try self.advance(now);
        self.owner = 0; self.epoch = 0; self.acpi_caps = 0;
        self.value.lid_state = 0; self.value.lid_sequence = self.value.sequence;
        self.capabilities(); return true;
    }
};

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
