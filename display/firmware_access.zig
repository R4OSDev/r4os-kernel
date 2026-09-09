// Admission for legacy writers that still use the firmware framebuffer.
// A takeover closes admission only between complete writers. This boundary
// can be used from the boot-log/fatal paths without taking a lower-rank lock.
const builtin = @import("builtin");
const ownership = @import("ownership.zig");

const revoked: u32 = 1 << 31;

pub const Gate = struct {
    state: u32 = 0,

    pub fn tryAcquire(self: *Gate) bool {
        const observed = @atomicLoad(u32, &self.state, .acquire);
        if (observed >= revoked - 1) return false;
        return @cmpxchgStrong(u32, &self.state, observed, observed + 1, .acq_rel, .acquire) == null;
    }

    pub fn release(self: *Gate) void {
        if (builtin.os.tag == .freestanding) asm volatile ("sfence" ::: .{ .memory = true });
        const old = @atomicRmw(u32, &self.state, .Sub, 1, .release);
        @import("std").debug.assert(old > 0 and old < revoked);
    }

    pub fn tryRevoke(self: *Gate) bool {
        return @cmpxchgStrong(u32, &self.state, 0, revoked, .acq_rel, .acquire) == null;
    }

    // Only the display transition owner may reopen after confirmed rollback.
    pub fn restore(self: *Gate) void {
        @import("std").debug.assert(@atomicLoad(u32, &self.state, .acquire) == revoked);
        @atomicStore(u32, &self.state, 0, .release);
    }

    pub fn isRevoked(self: *const Gate) bool {
        return (@atomicLoad(u32, &self.state, .acquire) & revoked) != 0;
    }
};

pub var gate: Gate = .{};

pub const Lease = struct {
    call: ownership.CallToken,

    pub fn release(self: Lease) void {
        gate.release();
        ownership.releaseCall(self.call);
    }
};

pub fn acquire() ?Lease {
    const call = ownership.retainCall();
    if (!call.admitted()) return null;
    if (!gate.tryAcquire()) {
        ownership.releaseCall(call);
        return null;
    }
    return .{ .call = call };
}

test "firmware revoke waits for every admitted writer and rejects stale access" {
    const testing = @import("std").testing;
    var local: Gate = .{};
    try testing.expect(local.tryAcquire());
    try testing.expect(local.tryAcquire());
    try testing.expect(!local.tryRevoke());
    local.release();
    try testing.expect(!local.tryRevoke());
    local.release();
    try testing.expect(local.tryRevoke());
    try testing.expect(!local.tryAcquire());
    try testing.expect(!local.tryRevoke());
    local.restore();
    try testing.expect(local.tryAcquire());
    local.release();
}
