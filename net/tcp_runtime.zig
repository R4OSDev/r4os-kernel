// Native network helper; caller retains its original owner/storage.
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
extern fn r4os_rust_net_tcp_identity_eq(*const ConnectionIdentity, *const ConnectionIdentity) bool;
extern fn r4os_rust_net_tcp_segment_payload(*const SentSegment, *usize) [*]const u8;
extern fn r4os_rust_net_tcp_segment_sequence(*const SentSegment) u32;
extern fn r4os_rust_net_tcp_catalog_reset(*SendCatalog) void;
extern fn r4os_rust_net_tcp_catalog_capacity(*const SendCatalog) bool;
extern fn r4os_rust_net_tcp_catalog_count(*const SendCatalog) usize;
extern fn r4os_rust_net_tcp_catalog_free(*const SendCatalog) usize;
extern fn r4os_rust_net_tcp_catalog_bytes(*const SendCatalog) usize;
extern fn r4os_rust_net_tcp_catalog_track(*SendCatalog, u32, u32, u16, [*]const u8, usize, u64) u64;
extern fn r4os_rust_net_tcp_catalog_ack(*SendCatalog, u32, *AckResult) void;
extern fn r4os_rust_net_tcp_catalog_oldest(*const SendCatalog, u8) ?*const SentSegment;
extern fn r4os_rust_net_tcp_catalog_retransmit(*SendCatalog, u64, u64) bool;
extern fn r4os_rust_net_rto_reset(*RtoTable) void;
extern fn r4os_rust_net_rto_bind(*RtoTable, *const ConnectionIdentity) ?*RtoState;
extern fn r4os_rust_net_rto_get(*RtoTable, *const ConnectionIdentity) ?*RtoState;
extern fn r4os_rust_net_rto_release(*RtoTable, *const ConnectionIdentity) void;
extern fn r4os_rust_net_rto_release_slot(*RtoTable, usize) void;
extern fn r4os_rust_net_tcp_tracking(u16, usize) bool;
extern fn r4os_rust_net_tcp_allowance(u32, usize, bool) usize;
extern fn r4os_rust_net_tcp_sequence(u16, usize) u32;
extern fn r4os_rust_net_tcp_seq_after(u32, u32) bool;
extern fn r4os_rust_net_tcp_deadline(u64, u64) bool;
pub const CONNECTION_CAPACITY: usize = 16;
// One bounded catalog can cover a complete unscaled TCP receive window.
// 48 * 1460 = 70,080 bytes, so a sender is no longer artificially stopped
// after the historical eight MSS while ownership of every retransmittable
// byte remains explicit and bounded.
pub const SENT_SEGMENT_CAPACITY: usize = 48;
pub const SENT_PAYLOAD_CAPACITY: usize = 1460;

pub const FLAG_FIN: u16 = 0x001;
pub const FLAG_SYN: u16 = 0x002;

pub const SegmentKind = enum(u8) {
    any,
    data,
    syn,
    fin,
};

pub const ConnectionIdentity = extern struct {
    connection_id: u32,
    slot: usize,
    generation: u32,

    pub fn eql(a: ConnectionIdentity, b: ConnectionIdentity) bool {
        return r4os_rust_net_tcp_identity_eq(&a, &b);
    }
};

pub const SentSegment = extern struct {
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
        var len: usize = 0;
        const data = r4os_rust_net_tcp_segment_payload(self, &len);
        return data[0..len];
    }

    pub fn sequenceLength(self: *const SentSegment) u32 {
        return r4os_rust_net_tcp_segment_sequence(self);
    }
};

pub const AckResult = extern struct {
    progressed: bool = false,
    fully_acked: u8 = 0,
    remaining: u8 = 0,
};

pub const SendCatalog = extern struct {
    segments: [SENT_SEGMENT_CAPACITY]SentSegment = .{SentSegment{}} ** SENT_SEGMENT_CAPACITY,
    next_token: u64 = 1,

    pub fn reset(self: *SendCatalog) void {
        r4os_rust_net_tcp_catalog_reset(self);
    }

    pub fn hasCapacity(self: *const SendCatalog) bool {
        return r4os_rust_net_tcp_catalog_capacity(self);
    }

    pub fn count(self: *const SendCatalog) usize {
        return r4os_rust_net_tcp_catalog_count(self);
    }

    pub fn freeCount(self: *const SendCatalog) usize {
        return r4os_rust_net_tcp_catalog_free(self);
    }

    pub fn outstandingBytes(self: *const SendCatalog) usize {
        return r4os_rust_net_tcp_catalog_bytes(self);
    }

    pub fn track(
        self: *SendCatalog,
        seq: u32,
        ack: u32,
        flags: u16,
        payload: []const u8,
        sent_tick: u64,
    ) ?u64 {
        const token = r4os_rust_net_tcp_catalog_track(self, seq, ack, flags, payload.ptr, payload.len, sent_tick);
        return if (token == 0) null else token;
    }

    pub fn acknowledge(self: *SendCatalog, ack: u32) AckResult {
        var out: AckResult = undefined;
        r4os_rust_net_tcp_catalog_ack(self, ack, &out);
        return out;
    }

    pub fn oldest(self: *const SendCatalog, kind: SegmentKind) ?*const SentSegment {
        return r4os_rust_net_tcp_catalog_oldest(self, @intFromEnum(kind));
    }

    pub fn markRetransmitted(self: *SendCatalog, token: u64, sent_tick: u64) bool {
        return r4os_rust_net_tcp_catalog_retransmit(self, token, sent_tick);
    }
};

pub const RtoState = extern struct {
    bound: bool = false,
    connection_id: u32 = 0,
    generation: u32 = 0,
    srtt_ticks: u64 = 0,
    sent_tick: u64 = 0,
    sent_tx_ack: u32 = 0,
};

pub const RtoTable = extern struct {
    slots: [CONNECTION_CAPACITY]RtoState = .{RtoState{}} ** CONNECTION_CAPACITY,

    pub fn reset(self: *RtoTable) void {
        r4os_rust_net_rto_reset(self);
    }

    pub fn bind(self: *RtoTable, identity: ConnectionIdentity) ?*RtoState {
        return r4os_rust_net_rto_bind(self, &identity);
    }

    pub fn get(self: *RtoTable, identity: ConnectionIdentity) ?*RtoState {
        return r4os_rust_net_rto_get(self, &identity);
    }

    pub fn release(self: *RtoTable, identity: ConnectionIdentity) void {
        r4os_rust_net_rto_release(self, &identity);
    }

    pub fn releaseSlot(self: *RtoTable, slot: usize) void {
        r4os_rust_net_rto_release_slot(self, slot);
    }
};

pub fn needsTracking(flags: u16, payload_len: usize) bool {
    return r4os_rust_net_tcp_tracking(flags, payload_len);
}

pub fn sendAllowance(remote_window: u32, requested: usize, catalog_has_capacity: bool) usize {
    return r4os_rust_net_tcp_allowance(remote_window, requested, catalog_has_capacity);
}

pub fn sequenceLengthFor(flags: u16, payload_len: usize) u32 {
    return r4os_rust_net_tcp_sequence(flags, payload_len);
}

pub fn seqAfter(a: u32, b: u32) bool {
    return r4os_rust_net_tcp_seq_after(a, b);
}

pub fn deadlineReached(now: u64, deadline: u64) bool {
    return r4os_rust_net_tcp_deadline(now, deadline);
}

test "remote window and catalog capacity bound each write" {
    const testing = @import("std").testing;
    try testing.expectEqual(@as(usize, 0), sendAllowance(0, 1460, true));
    try testing.expectEqual(@as(usize, 0), sendAllowance(1460, 1460, false));
    try testing.expectEqual(@as(usize, 512), sendAllowance(512, 1460, true));
    try testing.expectEqual(@as(usize, 1460), sendAllowance(4096, 1460, true));
}

test "bounded catalog spans more than one unscaled receive window burst" {
    const testing = @import("std").testing;
    try testing.expect(SENT_SEGMENT_CAPACITY * SENT_PAYLOAD_CAPACITY >= 65_535);

    var catalog: SendCatalog = .{};
    var seq: u32 = 1000;
    var payload: [SENT_PAYLOAD_CAPACITY]u8 = .{0x5A} ** SENT_PAYLOAD_CAPACITY;
    var index: usize = 0;
    while (index < SENT_SEGMENT_CAPACITY) : (index += 1) {
        _ = catalog.track(seq, 500, 0x018, payload[0..], @intCast(index + 1)) orelse return error.TestUnexpectedResult;
        seq +%= SENT_PAYLOAD_CAPACITY;
    }
    try testing.expectEqual(SENT_SEGMENT_CAPACITY, catalog.count());
    try testing.expectEqual(@as(usize, 0), catalog.freeCount());
    try testing.expectEqual(SENT_SEGMENT_CAPACITY * SENT_PAYLOAD_CAPACITY, catalog.outstandingBytes());
    try testing.expect(catalog.track(seq, 500, 0x018, payload[0..], 99) == null);

    _ = catalog.acknowledge(1000 + 9 * SENT_PAYLOAD_CAPACITY);
    try testing.expectEqual(@as(usize, 9), catalog.freeCount());
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

comptime {
    if (@sizeOf(ConnectionIdentity) != 24 or @alignOf(ConnectionIdentity) != 8 or @offsetOf(ConnectionIdentity, "connection_id") != 0 or @offsetOf(ConnectionIdentity, "slot") != 8 or @offsetOf(ConnectionIdentity, "generation") != 16) @compileError("Native network ConnectionIdentity layout");
}

comptime {
    if (@sizeOf(SentSegment) != 1504 or @alignOf(SentSegment) != 8 or @offsetOf(SentSegment, "valid") != 0 or @offsetOf(SentSegment, "token") != 8 or @offsetOf(SentSegment, "seq") != 16 or @offsetOf(SentSegment, "ack") != 20 or @offsetOf(SentSegment, "flags") != 24 or @offsetOf(SentSegment, "payload_offset") != 26 or @offsetOf(SentSegment, "payload_len") != 28 or @offsetOf(SentSegment, "sent_tick") != 32 or @offsetOf(SentSegment, "retransmits") != 40 or @offsetOf(SentSegment, "payload") != 41) @compileError("Native network SentSegment layout");
}

comptime {
    if (@sizeOf(AckResult) != 3 or @alignOf(AckResult) != 1 or @offsetOf(AckResult, "progressed") != 0 or @offsetOf(AckResult, "fully_acked") != 1 or @offsetOf(AckResult, "remaining") != 2) @compileError("Native network AckResult layout");
}

comptime {
    if (@sizeOf(SendCatalog) != 72200 or @alignOf(SendCatalog) != 8 or @offsetOf(SendCatalog, "segments") != 0 or @offsetOf(SendCatalog, "next_token") != 72192) @compileError("Native network SendCatalog layout");
}

comptime {
    if (@sizeOf(RtoState) != 40 or @alignOf(RtoState) != 8 or @offsetOf(RtoState, "bound") != 0 or @offsetOf(RtoState, "connection_id") != 4 or @offsetOf(RtoState, "generation") != 8 or @offsetOf(RtoState, "srtt_ticks") != 16 or @offsetOf(RtoState, "sent_tick") != 24 or @offsetOf(RtoState, "sent_tx_ack") != 32) @compileError("Native network RtoState layout");
}

comptime {
    if (@sizeOf(RtoTable) != 640 or @alignOf(RtoTable) != 8 or @offsetOf(RtoTable, "slots") != 0) @compileError("Native network RtoTable layout");
}
