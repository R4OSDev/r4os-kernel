pub const CONNECTION_CAPACITY: usize = 16;
pub const SENT_SEGMENT_CAPACITY: usize = 8;
pub const SENT_PAYLOAD_CAPACITY: usize = 2048;

pub const FLAG_FIN: u16 = 0x001;
pub const FLAG_SYN: u16 = 0x002;

pub const SegmentKind = enum {
    any,
    data,
    syn,
    fin,
};

pub const ConnectionIdentity = struct {
    connection_id: u32,
    slot: usize,
    generation: u32,

    pub fn eql(a: ConnectionIdentity, b: ConnectionIdentity) bool {
        return a.connection_id == b.connection_id and
            a.slot == b.slot and
            a.generation == b.generation;
    }
};

pub const SentSegment = struct {
    valid: bool = false,
    token: u64 = 0,
    seq: u32 = 0,
    ack: u32 = 0,
    flags: u16 = 0,
    payload_offset: u16 = 0,
    payload_len: u16 = 0,
    sent_tick: u64 = 0,
    retransmits: u8 = 0,
    payload: [SENT_PAYLOAD_CAPACITY]u8 = .{0} ** SENT_PAYLOAD_CAPACITY,

    pub fn payloadSlice(self: *const SentSegment) []const u8 {
        const start: usize = self.payload_offset;
        const len: usize = self.payload_len;
        return self.payload[start .. start + len];
    }

    pub fn sequenceLength(self: *const SentSegment) u32 {
        return sequenceLengthFor(self.flags, self.payload_len);
    }

    fn matches(self: *const SentSegment, kind: SegmentKind) bool {
        if (!self.valid) return false;
        return switch (kind) {
            .any => true,
            .data => self.payload_len != 0,
            .syn => (self.flags & FLAG_SYN) != 0,
            .fin => (self.flags & FLAG_FIN) != 0,
        };
    }
};

pub const AckResult = struct {
    progressed: bool = false,
    fully_acked: u8 = 0,
    remaining: u8 = 0,
};

pub const SendCatalog = struct {
    segments: [SENT_SEGMENT_CAPACITY]SentSegment = .{SentSegment{}} ** SENT_SEGMENT_CAPACITY,
    next_token: u64 = 1,

    pub fn reset(self: *SendCatalog) void {
        for (&self.segments) |*segment| segment.valid = false;
        self.next_token = 1;
    }

    pub fn hasCapacity(self: *const SendCatalog) bool {
        for (self.segments) |segment| {
            if (!segment.valid) return true;
        }
        return false;
    }

    pub fn count(self: *const SendCatalog) usize {
        var result: usize = 0;
        for (self.segments) |segment| {
            if (segment.valid) result += 1;
        }
        return result;
    }

    pub fn track(
        self: *SendCatalog,
        seq: u32,
        ack: u32,
        flags: u16,
        payload: []const u8,
        sent_tick: u64,
    ) ?u64 {
        if (!needsTracking(flags, payload.len) or payload.len > SENT_PAYLOAD_CAPACITY) return null;
        for (&self.segments) |*segment| {
            if (segment.valid) continue;
            var token = self.next_token;
            self.next_token +%= 1;
            if (self.next_token == 0) self.next_token = 1;
            if (token == 0) token = 1;
            // Die 2-KB-Nutzlast nicht pro Sendung nullen. valid/Laenge sind
            // die Besitzgrenze; alte Bytes hinter payload_len sind unsichtbar.
            segment.valid = true;
            segment.token = token;
            segment.seq = seq;
            segment.ack = ack;
            segment.flags = flags;
            segment.payload_offset = 0;
            segment.payload_len = @intCast(payload.len);
            segment.sent_tick = sent_tick;
            segment.retransmits = 0;
            if (payload.len != 0) @memcpy(segment.payload[0..payload.len], payload);
            return token;
        }
        return null;
    }

    pub fn acknowledge(self: *SendCatalog, ack: u32) AckResult {
        var result: AckResult = .{};
        for (&self.segments) |*segment| {
            if (!segment.valid or !seqAfter(ack, segment.seq)) continue;
            const distance = ack -% segment.seq;
            const total = segment.sequenceLength();
            if (distance >= total) {
                segment.valid = false;
                result.progressed = true;
                result.fully_acked +|= 1;
                continue;
            }

            var acknowledged = distance;
            if ((segment.flags & FLAG_SYN) != 0 and acknowledged != 0) {
                segment.flags &= ~FLAG_SYN;
                segment.seq +%= 1;
                acknowledged -= 1;
                result.progressed = true;
            }
            if (acknowledged != 0 and segment.payload_len != 0) {
                const trim: u16 = @intCast(@min(acknowledged, segment.payload_len));
                segment.payload_offset += trim;
                segment.payload_len -= trim;
                segment.seq +%= trim;
                acknowledged -= trim;
                result.progressed = true;
            }
            if ((segment.flags & FLAG_FIN) != 0 and acknowledged != 0) {
                segment.valid = false;
                result.progressed = true;
                result.fully_acked +|= 1;
            }
        }
        result.remaining = @intCast(self.count());
        return result;
    }

    pub fn oldest(self: *const SendCatalog, kind: SegmentKind) ?*const SentSegment {
        var result: ?*const SentSegment = null;
        for (&self.segments) |*segment| {
            if (!segment.matches(kind)) continue;
            if (result == null or segment.token < result.?.token) result = segment;
        }
        return result;
    }

    pub fn markRetransmitted(self: *SendCatalog, token: u64, sent_tick: u64) bool {
        for (&self.segments) |*segment| {
            if (!segment.valid or segment.token != token) continue;
            segment.sent_tick = sent_tick;
            segment.retransmits +|= 1;
            return true;
        }
        return false;
    }
};

pub const RtoState = struct {
    bound: bool = false,
    connection_id: u32 = 0,
    generation: u32 = 0,
    srtt_ticks: u64 = 0,
    sent_tick: u64 = 0,
    sent_tx_ack: u32 = 0,
};

pub const RtoTable = struct {
    slots: [CONNECTION_CAPACITY]RtoState = .{RtoState{}} ** CONNECTION_CAPACITY,

    pub fn reset(self: *RtoTable) void {
        self.* = .{};
    }

    pub fn bind(self: *RtoTable, identity: ConnectionIdentity) ?*RtoState {
        if (identity.connection_id == 0 or identity.generation == 0 or identity.slot >= self.slots.len) return null;
        const state = &self.slots[identity.slot];
        if (!state.bound or
            state.connection_id != identity.connection_id or
            state.generation != identity.generation)
        {
            state.* = .{
                .bound = true,
                .connection_id = identity.connection_id,
                .generation = identity.generation,
            };
        }
        return state;
    }

    pub fn get(self: *RtoTable, identity: ConnectionIdentity) ?*RtoState {
        if (identity.slot >= self.slots.len) return null;
        const state = &self.slots[identity.slot];
        if (!state.bound or
            state.connection_id != identity.connection_id or
            state.generation != identity.generation) return null;
        return state;
    }

    pub fn release(self: *RtoTable, identity: ConnectionIdentity) void {
        const state = self.get(identity) orelse return;
        state.* = .{};
    }

    pub fn releaseSlot(self: *RtoTable, slot: usize) void {
        if (slot < self.slots.len) self.slots[slot] = .{};
    }
};

pub fn needsTracking(flags: u16, payload_len: usize) bool {
    return payload_len != 0 or (flags & (FLAG_SYN | FLAG_FIN)) != 0;
}

pub fn sendAllowance(remote_window: u32, requested: usize, catalog_has_capacity: bool) usize {
    if (!catalog_has_capacity or remote_window == 0 or requested == 0) return 0;
    return @min(requested, @as(usize, remote_window));
}

pub fn sequenceLengthFor(flags: u16, payload_len: usize) u32 {
    var result: u32 = @intCast(payload_len);
    if ((flags & FLAG_SYN) != 0) result +%= 1;
    if ((flags & FLAG_FIN) != 0) result +%= 1;
    return result;
}

pub fn seqAfter(a: u32, b: u32) bool {
    const diff: i32 = @bitCast(a -% b);
    return diff > 0;
}

pub fn deadlineReached(now: u64, deadline: u64) bool {
    const diff: i64 = @bitCast(now -% deadline);
    return diff >= 0;
}

test "remote window and catalog capacity bound each write" {
    const testing = @import("std").testing;
    try testing.expectEqual(@as(usize, 0), sendAllowance(0, 1460, true));
    try testing.expectEqual(@as(usize, 0), sendAllowance(1460, 1460, false));
    try testing.expectEqual(@as(usize, 512), sendAllowance(512, 1460, true));
    try testing.expectEqual(@as(usize, 1460), sendAllowance(4096, 1460, true));
}

test "loss recovery retains every segment and advances on cumulative and partial ACKs" {
    const testing = @import("std").testing;
    var catalog: SendCatalog = .{};
    const first = catalog.track(1000, 500, 0x018, "ABCD", 10) orelse return error.TestUnexpectedResult;
    _ = catalog.track(1004, 500, 0x018, "EFGH", 11) orelse return error.TestUnexpectedResult;
    _ = catalog.track(1008, 500, 0x018, "IJKL", 12) orelse return error.TestUnexpectedResult;
    try testing.expectEqual(@as(usize, 3), catalog.count());
    try testing.expectEqual(first, catalog.oldest(.data).?.token);

    const cumulative = catalog.acknowledge(1004);
    try testing.expect(cumulative.progressed);
    try testing.expectEqual(@as(u8, 1), cumulative.fully_acked);
    try testing.expectEqual(@as(u32, 1004), catalog.oldest(.data).?.seq);

    const partial = catalog.acknowledge(1006);
    try testing.expect(partial.progressed);
    const trimmed = catalog.oldest(.data).?;
    try testing.expectEqual(@as(u32, 1006), trimmed.seq);
    try testing.expectEqualStrings("GH", trimmed.payloadSlice());
    try testing.expect(catalog.markRetransmitted(trimmed.token, 30));
    try testing.expectEqual(@as(u8, 1), catalog.oldest(.data).?.retransmits);

    const final = catalog.acknowledge(1012);
    try testing.expectEqual(@as(u8, 0), final.remaining);
    try testing.expect(catalog.oldest(.any) == null);
}

test "acknowledged payload stays absent during an empty receive wait" {
    const testing = @import("std").testing;
    var catalog: SendCatalog = .{};
    _ = catalog.track(2000, 700, 0x018, "historical", 40) orelse return error.TestUnexpectedResult;
    _ = catalog.acknowledge(2010);
    try testing.expect(catalog.oldest(.data) == null);

    // Ein receive-only Wait veraendert den Sendekatalog nicht. Damit kann
    // der fruehere Read-Pfad bestaetigte Nutzdaten nicht erneut bewaffnen.
    try testing.expectEqual(@as(usize, 0), catalog.count());
    try testing.expect(catalog.oldest(.data) == null);
}

test "RTO ownership cannot alias connection IDs or reused slots" {
    const testing = @import("std").testing;
    var table: RtoTable = .{};
    const first = ConnectionIdentity{ .connection_id = 1, .slot = 0, .generation = 1 };
    const sixteenth = ConnectionIdentity{ .connection_id = 16, .slot = 15, .generation = 1 };
    const reused = ConnectionIdentity{ .connection_id = 17, .slot = 0, .generation = 2 };

    const first_state = table.bind(first).?;
    first_state.srtt_ticks = 77;
    first_state.sent_tick = 100;
    table.bind(sixteenth).?.srtt_ticks = 88;
    try testing.expectEqual(@as(u64, 77), table.get(first).?.srtt_ticks);
    try testing.expectEqual(@as(u64, 88), table.get(sixteenth).?.srtt_ticks);

    const reused_state = table.bind(reused).?;
    try testing.expectEqual(@as(u64, 0), reused_state.srtt_ticks);
    try testing.expectEqual(@as(u64, 0), reused_state.sent_tick);
    try testing.expect(table.get(first) == null);
}

test "TIME_WAIT deadlines use monotonic ticks directly" {
    const testing = @import("std").testing;
    const start: u64 = 10_000;
    const deadline = start + 3_000;
    try testing.expect(!deadlineReached(1_000, deadline));
    try testing.expect(!deadlineReached(12_999, deadline));
    try testing.expect(deadlineReached(13_000, deadline));
}
