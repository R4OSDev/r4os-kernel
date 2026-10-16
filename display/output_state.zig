comptime { if (@import("builtin").is_test) _ = @import("rust_owner_test"); }
// Pure bounded connector catalog and atomic validation. No device accesses,
// callback invocation, allocation or resource release is permitted here.
const std = @import("std");
pub const abi = @import("r4os_kernel_contract");
const bo = @import("../memory/gfx_buffer_layout.zig");
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Capacity, Exhausted, Bandwidth, Routing, Dependency };
pub const capacity = abi.gfx_output_catalog_capacity;
pub const DriverOwner = @import("../memory/gfx_buffer_owner.zig").Owner;
const Source = extern struct {
    owner: DriverOwner = .{ .kind = .driver, .id = 0, .generation = 0 },
    binding: abi.GfxReceiverSource = .{},
    sequence: u64 = 0,
};
pub const Entry = extern struct {
    owner: u32 = 0,
    receiver_source: u64 = 0,
    paused: bool = false,
    info: abi.GfxOutputInfo = .{},
    has_color: bool = false, color_data: abi.GfxOutputColorState = .{},
    refresh: @import("refresh_state.zig").State = .{},
    power: @import("power_state.zig").State = .{},
    brightness: @import("brightness_state.zig").State = .{},
    retained_sleep: bool = false,
    modes: [abi.gfx_output_max_modes]abi.GfxOutputMode = .{abi.GfxOutputMode{}} ** abi.gfx_output_max_modes,
    edid: [abi.gfx_output_max_edid_bytes]u8 = .{0} ** abi.gfx_output_max_edid_bytes,
};
pub const Fact = struct { buffer: ?bo.Descriptor = null, dependency: enum { none, ready, pending, failed } = .none };
pub const Ticket = extern struct { id: u64 = 0 };
pub const Store = extern struct {
    entries: [capacity]Entry = .{Entry{}} ** capacity,
    audio: @import("audio_routes.zig").Store = .{},
    sources: [16]Source = @splat(.{}),
    source_serial: u64 = 0,
    revision: u64 = 0,
    receiver_serial: u64 = 0,
    attempt: u64 = 0,
    commit_sequence: u64 = 0,
    pending: Ticket = .{},
    retained: u32 = 0,
    reset_retired: Ticket = .{},

    fn call(self: *const Store, op: u32, args: Args, out: ?*anyopaque) Error!void { try status(r4os_rust_display_output_state(self, op, &args, out)); }
    fn result(self: *const Store, comptime T: type, op: u32, args: Args) Error!T { var out: T = undefined; try self.call(op, args, &out); return out; }
    pub fn publish(self: *Store, owner: u32, info: abi.GfxOutputInfo, modes: []const abi.GfxOutputMode, edid: []const u8) Error!abi.GfxOutputId {
        return self.result(abi.GfxOutputId, 0, .{ .owner = owner, .input = &info, .modes = modes.ptr, .modes_len = modes.len, .edid = edid.ptr, .edid_len = edid.len });
    }
    pub fn publishModeset(self: *Store, owner: u32, info: abi.GfxOutputInfo, modes: []const abi.GfxOutputMode, edid: []const u8) Error!abi.GfxOutputId {
        return self.result(abi.GfxOutputId, 0, .{ .owner = owner, .input = &info, .modes = modes.ptr, .modes_len = modes.len, .edid = edid.ptr, .edid_len = edid.len, .flag = true });
    }
    pub fn find(self: *const Store, identity: abi.GfxOutputId) Error!*const Entry { return &self.entries[try self.result(usize, 1, .{ .identity = identity })]; }
    pub fn infoAt(self: *const Store, index: u32) ?abi.GfxOutputInfo { const value = self.result(OptionalInfo, 2, .{ .index = index }) catch unreachable; return if (value.has) value.value else null; }
    pub fn publishColor(self: *Store, owner: u32, value: abi.GfxOutputColorState) Error!bool { return self.result(bool, 3, .{ .owner = owner, .input = &value }); }
    pub fn colorAt(self: *const Store, identity: abi.GfxOutputId) Error!abi.GfxOutputColorState { return self.result(abi.GfxOutputColorState, 4, .{ .identity = identity }); }
    pub fn modeAt(self: *const Store, identity: abi.GfxOutputId, index: u32) Error!?abi.GfxOutputMode { const value = try self.result(OptionalMode, 5, .{ .identity = identity, .index = index }); return if (value.has) value.value else null; }
    pub fn refreshEntry(self: *Store, target: abi.GfxOutputTarget) Error!*Entry { return &self.entries[try self.result(usize, 6, .{ .target = target })]; }
    pub fn publishRefresh(self: *Store, owner: u32, value: abi.GfxOutputRefresh) Error!bool { return self.result(bool, 7, .{ .owner = owner, .input = &value }); }
    pub fn refreshAt(self: *Store, target: abi.GfxOutputTarget) Error!abi.GfxOutputRefresh { return self.result(abi.GfxOutputRefresh, 8, .{ .target = target }); }
    pub fn requestRefresh(self: *Store, actor: DriverOwner, input: abi.GfxRefreshRequest, now: u64) Error!abi.GfxRefreshRequest { return self.result(abi.GfxRefreshRequest, 9, .{ .actor = actor, .input = &input, .now = now }); }
    pub fn readRefresh(self: *Store, owner: u32, target: abi.GfxOutputTarget, now: u64) Error!abi.GfxRefreshRequest { return self.result(abi.GfxRefreshRequest, 10, .{ .owner = owner, .target = target, .now = now }); }
    pub fn powerEntry(self: *Store, identity: abi.GfxOutputId) Error!*Entry { return &self.entries[try self.result(usize, 11, .{ .identity = identity })]; }
    pub fn publishPower(self: *Store, owner: u32, value: abi.GfxOutputPower) Error!bool { return self.result(bool, 12, .{ .owner = owner, .input = &value }); }
    pub fn powerAt(self: *Store, identity: abi.GfxOutputId) Error!abi.GfxOutputPower { return self.result(abi.GfxOutputPower, 13, .{ .identity = identity }); }
    pub fn requestPower(self: *Store, actor: DriverOwner, input: abi.GfxPowerRequest, now: u64) Error!abi.GfxPowerRequest { return self.result(abi.GfxPowerRequest, 14, .{ .actor = actor, .input = &input, .now = now }); }
    pub fn readPower(self: *Store, owner: u32, identity: abi.GfxOutputId, now: u64) Error!abi.GfxPowerRequest { return self.result(abi.GfxPowerRequest, 15, .{ .owner = owner, .identity = identity, .now = now }); }
    pub fn publishBrightness(self: *Store, owner: u32, value: abi.GfxOutputBrightness) Error!bool { return self.result(bool, 16, .{ .owner = owner, .input = &value }); }
    pub fn brightnessAt(self: *Store, identity: abi.GfxOutputId) Error!abi.GfxOutputBrightness { return self.result(abi.GfxOutputBrightness, 17, .{ .identity = identity }); }
    pub fn requestBrightness(self: *Store, actor: DriverOwner, input: abi.GfxBrightnessRequest) Error!abi.GfxBrightnessRequest { return self.result(abi.GfxBrightnessRequest, 18, .{ .actor = actor, .input = &input }); }
    pub fn readBrightness(self: *Store, owner: u32, identity: abi.GfxOutputId) Error!abi.GfxBrightnessRequest { return self.result(abi.GfxBrightnessRequest, 19, .{ .owner = owner, .identity = identity }); }
    pub fn withdraw(self: *Store, owner: u32, identity: abi.GfxOutputId) Error!void { try self.call(20, .{ .owner = owner, .identity = identity }, null); }
    pub fn pause(self: *Store, owner: u32, identity: abi.GfxOutputId, paused: bool, primary: bool) Error!bool { return self.result(bool, 21, .{ .owner = owner, .identity = identity, .flag = paused, .other_flag = primary }); }
    pub fn stop(self: *Store, owner: u32) Error!bool { return self.result(bool, 22, .{ .owner = owner }); }
    pub fn registerSource(self: *Store, owner: DriverOwner, adapter: u32) Error!abi.GfxReceiverSource { return self.result(abi.GfxReceiverSource, 23, .{ .actor = owner, .index = adapter }); }
    pub fn closeSource(self: *Store, owner: DriverOwner, binding: abi.GfxReceiverSource) Error!bool { return self.result(bool, 24, .{ .actor = owner, .binding = binding }); }
    pub fn replaceReceivers(self: *Store, owner: DriverOwner, binding: abi.GfxReceiverSource, sequence: u64, records: []const abi.GfxReceiverInfo) Error!void { try self.call(25, .{ .actor = owner, .binding = binding, .sequence = sequence, .records = records.ptr, .records_len = records.len }, null); }
    pub fn publishAudio(self: *Store, owner: DriverOwner, value: *const abi.GfxAudioRoute) Error!void { try self.call(26, .{ .actor = owner, .input = value }, null); }
    pub fn cursor(self: *const Store) abi.GfxDisplayRevision { return self.result(abi.GfxDisplayRevision, 27, .{}) catch unreachable; }
    pub fn validate(self: *const Store, state: *const abi.GfxAtomicState, facts: []const Fact) Error!abi.GfxAtomicResult {
        if (facts.len > abi.gfx_output_max_assignments) return error.Invalid;
        var projected: [abi.gfx_output_max_assignments]NativeFact = undefined; projectFacts(facts, &projected);
        return self.result(abi.GfxAtomicResult, 28, .{ .input = state, .facts = &projected, .facts_len = facts.len });
    }
    pub fn begin(self: *Store, state: *const abi.GfxAtomicState, facts: []const Fact) Error!Ticket {
        if (facts.len > abi.gfx_output_max_assignments) return error.Invalid;
        var projected: [abi.gfx_output_max_assignments]NativeFact = undefined; projectFacts(facts, &projected);
        return self.result(Ticket, 29, .{ .input = state, .facts = &projected, .facts_len = facts.len });
    }
    pub fn finish(self: *Store, ticket: Ticket, outcome: u32, quiesced: u32) Error!abi.GfxAtomicResult { return self.result(abi.GfxAtomicResult, 30, .{ .ticket = ticket, .outcome = outcome, .quiesced = quiesced }); }
    pub fn retireAfterReset(self: *Store, ticket: Ticket) Error!void { try self.call(31, .{ .ticket = ticket }, null); }
};
const NativeFact = extern struct { has_buffer: bool, buffer: bo.Descriptor, dependency: u32 };
fn projectFacts(input: []const Fact, output: *[abi.gfx_output_max_assignments]NativeFact) void { for (input, 0..) |value, i| output[i] = .{ .has_buffer = value.buffer != null, .buffer = value.buffer orelse .{ .bytes = 0 }, .dependency = @intFromEnum(value.dependency) }; }
const Args = extern struct {
    owner: u32 = 0, actor: DriverOwner = .{ .kind = .kernel, .id = 0, .generation = 0 },
    identity: abi.GfxOutputId = .{}, target: abi.GfxOutputTarget = .{}, binding: abi.GfxReceiverSource = .{}, sequence: u64 = 0, index: u32 = 0,
    ticket: Ticket = .{}, outcome: u32 = 0, quiesced: u32 = 0, flag: bool = false, other_flag: bool = false, now: u64 = 0,
    input: ?*const anyopaque = null, modes: ?[*]const abi.GfxOutputMode = null, modes_len: usize = 0, edid: ?[*]const u8 = null, edid_len: usize = 0,
    facts: ?[*]const NativeFact = null, facts_len: usize = 0, records: ?[*]const abi.GfxReceiverInfo = null, records_len: usize = 0,
};
const OptionalInfo = extern struct { has: bool, value: abi.GfxOutputInfo };
const OptionalMode = extern struct { has: bool, value: abi.GfxOutputMode };
extern fn r4os_rust_display_output_state(*const Store, u32, *const Args, ?*anyopaque) i32;
fn status(code: i32) Error!void { return switch (code) { 0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 4 => error.Exhausted, 6 => error.Capacity, 7 => error.Stale, 8 => error.Busy, 17 => error.Bandwidth, 18 => error.Routing, 19 => error.Dependency, else => unreachable }; }
comptime {
    if (@sizeOf(Source) != 48 or @sizeOf(NativeFact) != 144 or @offsetOf(NativeFact, "buffer") != 8 or @offsetOf(NativeFact, "dependency") != 136 or @sizeOf(Ticket) != 8) @compileError("Native output owner/facts C layout");
    if (@sizeOf(Entry) != 9360 or @alignOf(Entry) != 8 or @offsetOf(Entry, "info") != 24 or @offsetOf(Entry, "color_data") != 200 or @offsetOf(Entry, "refresh") != 392 or @offsetOf(Entry, "power") != 800 or @offsetOf(Entry, "brightness") != 1008 or @offsetOf(Entry, "modes") != 1168 or @offsetOf(Entry, "edid") != 5264) @compileError("Native output entry C layout");
    if (@sizeOf(Store) != 611136 or @alignOf(Store) != 8 or @offsetOf(Store, "audio") != 599040 or @offsetOf(Store, "sources") != 610304 or @offsetOf(Store, "source_serial") != 611072 or @offsetOf(Store, "reset_retired") != 611128) @compileError("Native output catalog C layout");
    if (@sizeOf(Args) != 240 or @offsetOf(Args, "actor") != 8 or @offsetOf(Args, "input") != 168 or @offsetOf(Args, "records_len") != 232 or @sizeOf(OptionalInfo) != 176 or @sizeOf(OptionalMode) != 72) @compileError("Native output call C layout");
}
pub const unknown_limits = blk: {
    var value = std.mem.zeroes(abi.GfxDisplayLimits);
    value.version = 1;
    value.size = @sizeOf(abi.GfxDisplayLimits);
    break :blk value;
};
pub fn driverTable(output: *abi.GfxDriverOutputApi, functions: abi.GfxDriverOutputApi) i32 { return r4os_rust_display_output_driver_table(output, &functions); }
pub fn validMode(value: abi.GfxOutputMode) bool { return r4os_rust_display_output_valid_mode(&value); }
extern fn r4os_rust_display_output_driver_table(*abi.GfxDriverOutputApi, *const abi.GfxDriverOutputApi) i32;
extern fn r4os_rust_display_output_valid_mode(*const abi.GfxOutputMode) bool;

fn testMode() abi.GfxOutputMode {
    return .{ .mode_id = 1, .flags = 8, .width = 640, .height = 480, .pixel_clock_hz = 25175000,
        .h_total = 800, .h_sync_start = 656, .h_sync_end = 752, .v_total = 525, .v_sync_start = 490, .v_sync_end = 492, .refresh_millihz = 59940 };
}
fn testInfo(port: u32) abi.GfxOutputInfo {
    return .{ .identity = .{ .adapter_id = 4, .connector_id = port, .device_generation = 7 }, .flags = 1,
        .connector_kind = 4, .mode_count = 1, .preferred_mode_id = 1, .possible_heads = 3, .possible_planes = 3, .possible_plls = 3,
        .limits = .{ .flags = abi.gfx_output_limit_modeset, .head_mask = 3, .plane_mask = 3, .pll_mask = 3, .max_width = 1920, .max_height = 1080,
            .bandwidth_bytes_per_second = 200_000_000, .max_pixel_clock_hz = 150_000_000, .total_pixel_clock_hz = 200_000_000 } };
}
fn testAssignment(identity: abi.GfxOutputId, head: u32) abi.GfxScanoutState {
    return .{ .output = identity, .mode_id = 1, .head_id = head, .plane_id = head, .pll_id = head,
        .source_width = 640, .source_height = 480, .destination_width = 640, .destination_height = 480,
        .buffer = .{ .id = head + 1, .generation = 1 } };
}
fn testFact() Fact {
    return .{ .buffer = .{ .bytes = 2560 * 480, .width = 640, .height = 480, .format = .xrgb8888, .plane_count = 1,
        .planes = .{ .{ .pitch = 2560 }, .{}, .{}, .{} }, .usage = bo.Usage.scanout } };
}
test "output unplug and identical replug invalidate every old receiver mode and EDID identity" {
    const t = std.testing;
    var store: Store = .{};
    var info = testInfo(9); info.edid_bytes = 128;
    var data: [128]u8 = .{0x79} ** 128;
    const first = try store.publish(14, info, &.{testMode()}, &data);
    try t.expectError(error.Unsupported, store.colorAt(first));
    var color: abi.GfxOutputColorState = .{ .identity = first, .flags = abi.gfx_output_color_known,
        .formats = 1, .depths = 1, .color_spaces = 1, .transfers = 1, .ranges = 1 };
    try t.expectError(error.Stale, store.publishColor(15, color));
    try t.expect(try store.publishColor(14, color));
    const color_revision = store.revision;
    try t.expect(!(try store.publishColor(14, color)) and store.revision == color_revision);
    color.reserved0 = 1;
    try t.expectError(error.Invalid, store.publishColor(14, color));
    color.reserved0 = 0;
    color.flags |= abi.gfx_output_color_active | abi.gfx_output_color_identity;
    color.format = abi.gfx_buffer_format_xrgb8888; color.bpc = 8; color.primaries = 1; color.transfer = 1; color.range = 1;
    color.reference_white = 1_000_000; color.peak = 1_000_000;
    color.link_kind = abi.gfx_output_link_dp_sst; color.link_flags = 3; color.dsc_depths = 3;
    color.compressed_bpp_x16 = 256; color.link_lanes = 4; color.link_rate_mbps = 8100;
    color.link_payload_bits_per_second = 25_142_400_000; color.h_active = 640; color.h_total = 800; color.v_active = 480;
    color.pixel_clock_numerator = 25_175_000; color.pixel_clock_denominator = 1;
    _ = try store.pause(14, first, true, true);
    try t.expectError(error.Stale, store.publishColor(14, color));
    _ = try store.pause(14, first, false, true);
    try t.expect(try store.publishColor(14, color));
    const active_color = try store.colorAt(first);
    try t.expect(active_color.revision == store.revision and active_color.bpc == 8 and active_color.gamma_entries == 0 and active_color.transfers == 1);
    _ = try store.pause(14, first, true, true);
    const paused_color = try store.colorAt(first);
    try t.expect(paused_color.flags & abi.gfx_output_color_active == 0 and paused_color.bpc == 0 and paused_color.formats == 1);
    try t.expect(paused_color.link_kind == 0 and paused_color.link_flags == 0 and paused_color.compressed_bpp_x16 == 0 and
        paused_color.link_payload_bits_per_second == 0 and paused_color.pixel_clock_numerator == 0 and paused_color.dsc_depths == 3);
    _ = try store.pause(14, first, false, true);
    try @import("refresh_state_test.zig").check(&store, first);
    try @import("power_state_check.zig").check(&store, first);
    const revision = store.revision;
    try store.withdraw(14, first);
    try t.expect(store.revision > revision);
    try t.expectError(error.Stale, store.find(first));
    const empty = store.infoAt(0).?;
    try t.expectError(error.Stale, store.colorAt(first));
    try t.expectError(error.Unsupported, store.colorAt(empty.identity));
    try t.expectEqual(@as(u32, 0), empty.edid_bytes);
    try t.expectEqual(@as(u32, 0), empty.mode_count);
    try t.expectEqual(@as(u32, 0), empty.preferred_mode_id);
    const second = try store.publish(14, info, &.{testMode()}, &data);
    try t.expectError(error.Unsupported, store.colorAt(second));
    try t.expectError(error.Unsupported, store.brightnessAt(second));
    try t.expectError(error.Stale, store.brightnessAt(first));
    try t.expect(first.connector_id == second.connector_id and second.connection_generation > first.connection_generation);
    try t.expectError(error.Stale, store.modeAt(first, 0));
    try t.expect((try store.modeAt(second, 0)).?.width == 640);
    var changed = info; changed.limits.max_width = 2560;
    try t.expectError(error.Invalid, store.publishModeset(14, changed, &.{testMode()}, &data));
    try store.withdraw(14, second);
    try t.expectError(error.Invalid, store.publishModeset(15, changed, &.{testMode()}, &data));
    const restored = try store.publishModeset(14, changed, &.{testMode()}, &data);
    try t.expect(restored.connection_generation > second.connection_generation and (try store.find(restored)).info.limits.max_width == 2560);
    var sibling = changed; sibling.identity.connector_id += 1;
    _ = try store.publish(14, sibling, &.{testMode()}, &data);
    try store.withdraw(14, restored);
    changed.limits.max_width = 1920;
    try t.expectError(error.Invalid, store.publishModeset(14, changed, &.{testMode()}, &data));
    try t.expect(try store.stop(14));
    try t.expectError(error.Stale, store.find(second));
}
test "atomic composition rejects routing bandwidth buffer arithmetic and active dependencies before begin" {
    const t = std.testing;
    var store: Store = .{};
    const a = try store.publish(14, testInfo(1), &.{testMode()}, &.{});
    const b = try store.publish(14, testInfo(2), &.{testMode()}, &.{});
    var state = abi.GfxAtomicState{ .count = 2, .topology_revision = store.revision };
    state.assignments[0] = testAssignment(a, 0); state.assignments[1] = testAssignment(b, 1);
    var facts = [_]Fact{ testFact(), testFact() };
    _ = try store.validate(&state, &facts);
    const revision = store.revision;
    state.assignments[1].pll_id = 0;
    try t.expectError(error.Routing, store.begin(&state, &facts));
    try t.expect(store.attempt == 0 and store.pending.id == 0 and store.revision == revision);
    state.assignments[1].pll_id = 1;
    state.assignments[0].destination_x = 0xffff_ffff;
    try t.expectError(error.Invalid, store.begin(&state, &facts));
    state.assignments[0].destination_x = 0;
    facts[0].buffer.?.planes[0].pitch = std.math.maxInt(u64) - 3;
    try t.expectError(error.Invalid, store.begin(&state, &facts));
    facts[0] = testFact(); facts[0].buffer.?.modifier = 1;
    try t.expectError(error.Invalid, store.begin(&state, &facts));
    facts[0] = testFact(); facts[0].buffer.?.binding = .{ .adapter = 4, .driver_owner = 14, .device_generation = 8 };
    try t.expectError(error.Stale, store.begin(&state, &facts));
    facts[0] = testFact(); facts[0].dependency = .pending;
    state.assignments[0].ready_fence = .{ .slot = 1, .timeline = 3, .point = 4, .device_generation = 1, .reset_generation = 1 };
    try t.expectError(error.Busy, store.begin(&state, &facts));
    facts[0].dependency = .failed;
    try t.expectError(error.Dependency, store.begin(&state, &facts));
    facts[0].dependency = .ready;
    for (&store.entries) |*entry| if (entry.info.identity.connector_id != 0) { entry.info.limits.bandwidth_bytes_per_second = 100_000_000; };
    try t.expectError(error.Bandwidth, store.begin(&state, &facts));
    try t.expect(store.attempt == 0 and store.pending.id == 0 and store.revision == revision);
}
test "atomic rollback and uncertain hardware outcomes retain resources until explicit quiescence" {
    const t = std.testing;
    var store: Store = .{};
    const id = try store.publish(14, testInfo(1), &.{testMode()}, &.{});
    var state = abi.GfxAtomicState{ .count = 1, .topology_revision = store.revision };
    state.assignments[0] = testAssignment(id, 0);
    const ticket = try store.begin(&state, &.{testFact()});
    try t.expect(try store.pause(14, id, true, true));
    try t.expect(!try store.pause(14, id, true, true));
    try t.expect((try store.find(id)).paused and store.pending.id == ticket.id);
    try t.expectError(error.Stale, store.pause(15, id, false, true));
    try t.expectError(error.Busy, store.withdraw(14, id));
    try t.expectError(error.Busy, store.finish(ticket, abi.gfx_output_outcome_old_preserved, 0));
    const rollback = try store.finish(ticket, abi.gfx_output_outcome_old_preserved, abi.gfx_output_retain_new);
    try t.expectEqual(@as(u32, 1), rollback.retained);
    try t.expectEqual(@as(u64, 0), rollback.commit_sequence);
    try t.expect((try store.find(id)).paused);
    try t.expect(try store.pause(14, id, false, true));
    try t.expect(!(try store.find(id)).paused and (try store.find(id)).info.flags & abi.gfx_output_flag_active != 0);
    try t.expectError(error.Stale, store.begin(&state, &.{testFact()}));
    state.topology_revision = store.revision;
    const next = try store.begin(&state, &.{testFact()});
    try t.expectError(error.Stale, store.finish(ticket, abi.gfx_output_outcome_applied, 3));
    const lost = try store.finish(next, abi.gfx_output_outcome_lost, 0);
    try t.expectEqual(@as(u32, 3), lost.retained);
    state.topology_revision = store.revision;
    try t.expectError(error.Busy, store.begin(&state, &.{testFact()}));
    try t.expectError(error.Stale, store.retireAfterReset(ticket));
    try store.retireAfterReset(next);
    const reset_revision = store.revision;
    try store.retireAfterReset(next);
    try t.expect(store.retained == 0 and store.pending.id == 0 and store.revision == reset_revision);
    state.topology_revision = store.revision;
    const fresh = try store.begin(&state, &.{testFact()});
    try t.expectError(error.Stale, store.retireAfterReset(next));
    try t.expectError(error.Stale, store.finish(next, abi.gfx_output_outcome_applied, 3));
    try t.expect(store.pending.id == fresh.id);
    try store.retireAfterReset(fresh);
    try t.expect(store.pending.id == 0 and store.retained == 0);
}
test "firmware mode only retains actual geometry without invented refresh or replacement BO" {
    const t = std.testing;
    var store: Store = .{};
    var info = testInfo(1);
    info.identity.adapter_id = 0; info.identity.device_generation = 1;
    info.flags = abi.gfx_output_flag_connected | abi.gfx_output_flag_firmware_snapshot | abi.gfx_output_flag_fixed_geometry;
    info.limits.flags = 0; info.limits.bandwidth_bytes_per_second = 0; info.limits.max_pixel_clock_hz = 0; info.limits.total_pixel_clock_hz = 0;
    const mode = abi.GfxOutputMode{ .mode_id = 1, .flags = abi.gfx_output_mode_geometry_only, .width = 1280, .height = 720 };
    const id = try store.publish(0, info, &.{mode}, &.{});
    var state = abi.GfxAtomicState{ .count = 1, .topology_revision = store.revision };
    state.assignments[0] = .{ .output = id, .mode_id = 1, .source_width = 1280, .source_height = 720, .destination_width = 1280, .destination_height = 720 };
    _ = try store.validate(&state, &.{.{}});
    state.assignments[0].buffer = .{ .id = 1, .generation = 1 };
    try t.expectError(error.Unsupported, store.begin(&state, &.{testFact()}));
    state.assignments[0].buffer = .{}; state.assignments[0].destination_width = 640;
    try t.expectError(error.Unsupported, store.begin(&state, &.{.{}}));
}

test "receiver batches preserve boot, reject partial generations and revoke metadata despite retained hardware" {
    const t = std.testing;
    const store = try t.allocator.create(Store);
    defer t.allocator.destroy(store);
    store.* = .{};
    var boot = testInfo(1);
    boot.identity.adapter_id = 0;
    const boot_id = try store.publish(0, boot, &.{testMode()}, &.{});
    const owner: DriverOwner = .{ .kind = .driver, .id = 3, .generation = 9 };
    const binding = try store.registerSource(owner, 9);
    const records = try t.allocator.alloc(abi.GfxReceiverInfo, 32);
    defer t.allocator.free(records);
    for (records, 0..) |*record, i| {
        record.* = .{ .connector_id = @intCast(i + 1), .flags = abi.gfx_output_flag_connected, .mode_count = 1, .edid_bytes = 128 };
        record.modes[0] = testMode();
        record.edid[1] = 79;
    }
    records[0].connector_kind = abi.gfx_output_kind_dvi;
    records[1].connector_kind = abi.gfx_output_kind_displayport;
    try store.replaceReceivers(owner, binding, 1, records);
    try t.expectEqual(@as(u32, 33), store.cursor().present);
    const first = store.infoAt(1).?;
    try t.expect(first.connector_kind == abi.gfx_output_kind_dvi and store.infoAt(2).?.connector_kind == abi.gfx_output_kind_displayport);
    const last = store.infoAt(32).?;
    try t.expect(std.meta.eql(first.limits, unknown_limits) and first.possible_heads == 0 and
        first.flags == abi.gfx_output_flag_connected | abi.gfx_output_flag_receiver_only);
    const revision = store.revision;
    records[31].modes[0].pixel_clock_hz = 0;
    try t.expectError(error.Invalid, store.replaceReceivers(owner, binding, 2, records));
    try t.expect(store.revision == revision and (try store.find(last.identity)).edid[1] == 79);
    records[31].modes[0] = testMode();
    try t.expectError(error.Stale, store.replaceReceivers(owner, binding, 1, records));
    var wrong = owner; wrong.generation += 1;
    try t.expectError(error.Stale, store.replaceReceivers(wrong, binding, 2, records));
    var state = abi.GfxAtomicState{ .count = 1, .topology_revision = store.revision };
    state.assignments[0] = testAssignment(first.identity, 0);
    try t.expectError(error.Unsupported, store.validate(&state, &.{testFact()}));
    // An unrelated hardware transaction is held. Invalidation neither waits
    // for it nor clears its retained resources, but every receiver ID expires.
    store.pending = .{ .id = 1 }; store.retained = 3;
    try store.replaceReceivers(owner, binding, 2, &.{});
    try t.expectError(error.Stale, store.find(first.identity));
    try t.expectError(error.Stale, store.find(last.identity));
    try t.expect(store.pending.id == 1 and store.retained == 3 and store.cursor().present == 1);
    try t.expect(std.meta.eql((try store.find(boot_id)).info.identity, boot_id));
    try store.replaceReceivers(owner, binding, 3, records[0..2]);
    const replacement = store.infoAt(1).?;
    try t.expect(replacement.identity.connection_generation > last.identity.connection_generation);
    store.revision = std.math.maxInt(u64) - 1;
    try t.expectError(error.Exhausted, store.replaceReceivers(owner, binding, 4, records[0..1]));
    try t.expect(try store.closeSource(owner, binding));
    try t.expectError(error.Stale, store.find(replacement.identity));
    try t.expectError(error.Stale, store.replaceReceivers(owner, binding, 4, records));
    try t.expect(store.cursor().present == 1 and store.infoAt(0).?.identity.adapter_id == 0 and store.infoAt(1) == null);

    // A source replaces only its own metadata port. Receiver refresh/close
    // cannot revoke that source's binding or invent a connected monitor.
    store.* = .{};
    const native_source = try store.registerSource(owner, 9);
    records[0] = .{ .connector_id = 1, .flags = abi.gfx_output_flag_connection_unknown };
    records[1] = .{ .connector_id = 2 };
    try store.replaceReceivers(owner, native_source, 1, records[0..2]);
    const metadata = store.infoAt(0).?.identity;
    var native_info = testInfo(1);
    native_info.identity.adapter_id = 9;
    native_info.flags = abi.gfx_output_flag_connection_unknown;
    native_info.limits.flags = 0;
    const geometry: abi.GfxOutputMode = .{ .mode_id = 1, .width = 640, .height = 480, .flags = abi.gfx_output_mode_geometry_only };
    try t.expectError(error.Busy, store.publish(99, native_info, &.{geometry}, &.{}));
    try t.expectError(error.Invalid, store.publish(3, native_info, &.{testMode()}, &.{}));
    const promoted = try store.publish(3, native_info, &.{geometry}, &.{});
    try t.expectError(error.Stale, store.find(metadata));
    try t.expect(store.cursor().present == 2 and (try store.find(promoted)).receiver_source == 0);
    try store.replaceReceivers(owner, native_source, 2, records[0..2]);
    try t.expect(std.meta.eql((try store.find(promoted)).info.identity, promoted) and store.cursor().present == 2);
    try t.expect((try store.find(promoted)).info.flags == abi.gfx_output_flag_connection_unknown);
    try t.expect(try store.closeSource(owner, native_source));
    try t.expect(store.cursor().present == 1 and (try store.find(promoted)).owner == 3);
    try store.withdraw(3, promoted);
    try t.expectError(error.Stale, store.find(promoted));
    try t.expect(store.infoAt(0).?.flags == 0 and store.infoAt(0).?.mode_count == 0);

    // Queue retirement calls stop() after the metadata source has closed.
    // The old native port is now disconnected and unowned, not a competing
    // driver. A rebuilt source must be admitted without reviving its ID or
    // leaving a duplicate port behind the fresh receiver publication.
    const retired_port = store.infoAt(0).?.identity;
    try t.expect(try store.stop(3));
    var rebuilt_owner = owner; rebuilt_owner.generation += 1;
    const rebuilt_source = try store.registerSource(rebuilt_owner, 9);
    try t.expectError(error.Stale, store.find(retired_port));
    try t.expectError(error.Stale, store.closeSource(owner, native_source));
    try t.expectEqual(@as(u32, 0), store.cursor().present);
    try store.replaceReceivers(rebuilt_owner, rebuilt_source, 1, records[0..1]);
    try t.expectEqual(@as(u32, 1), store.cursor().present);
    try t.expectEqual(rebuilt_source.generation, store.infoAt(0).?.identity.device_generation);
    try t.expectError(error.Busy, store.registerSource(owner, 9));
    try t.expectError(error.Busy, store.publish(99, native_info, &.{geometry}, &.{}));
    native_info.identity.device_generation += 1;
    const rebuilt_port = try store.publish(3, native_info, &.{geometry}, &.{});
    try t.expectEqual(@as(u32, 1), store.cursor().present);
    try t.expectEqual(@as(u32, 3), (try store.find(rebuilt_port)).owner);
    try t.expect(!(try store.closeSource(rebuilt_owner, rebuilt_source)));
    try t.expect(try store.stop(3));

    // Generic queue removal precedes proven mode retirement. Withdraw only
    // this driver's metadata while the exact pending/lost ticket stays held.
    // Busy is a live consumer, not catalog counter exhaustion.
    for (0..2) |lost_first| {
        store.* = .{};
        const source = try store.registerSource(owner, 4);
        records[0] = .{ .connector_id = 1 };
        try store.replaceReceivers(owner, source, 1, records[0..1]);
        const live_port = try store.publish(3, testInfo(1), &.{testMode()}, &.{});
        var foreign_info = testInfo(2); foreign_info.identity.adapter_id = 10;
        const foreign_port = try store.publish(99, foreign_info, &.{testMode()}, &.{});
        state = .{ .count = 1, .topology_revision = store.revision };
        state.assignments[0] = testAssignment(live_port, 0);
        const ticket = try store.begin(&state, &.{testFact()});
        if (lost_first != 0) _ = try store.finish(ticket, abi.gfx_output_outcome_lost, 0);
        const pending = store.pending; const retained = store.retained;
        try t.expect(try store.stop(3));
        try t.expect(std.meta.eql(store.pending, pending) and store.retained == retained and store.reset_retired.id == 0);
        try t.expectError(error.Stale, store.find(live_port));
        try t.expectError(error.Stale, store.closeSource(owner, source));
        try t.expect((try store.find(foreign_port)).owner == 99);
        if (lost_first == 0) _ = try store.finish(ticket, abi.gfx_output_outcome_lost, 0);
        try t.expect(store.retained == 3 and store.pending.id == 0);
        try t.expect(!(try store.stop(3)) and store.retained == 3);
        try t.expectError(error.Stale, store.retireAfterReset(.{ .id = ticket.id + 1 }));
        try t.expect(store.retained == 3);
        try store.retireAfterReset(ticket);
        try t.expect(store.retained == 0 and store.reset_retired.id == ticket.id);
        const fresh_source = try store.registerSource(rebuilt_owner, 4);
        try store.replaceReceivers(rebuilt_owner, fresh_source, 1, records[0..1]);
        const fresh_port = try store.publish(3, testInfo(1), &.{testMode()}, &.{});
        try t.expect(fresh_source.generation > source.generation and !std.meta.eql(fresh_port, live_port));
        try t.expect((try store.find(foreign_port)).owner == 99);
    }
    // Withdrawal must not consume the outstanding ticket's last increment.
    store.* = .{};
    const final_port = try store.publish(3, testInfo(1), &.{testMode()}, &.{});
    state = .{ .count = 1, .topology_revision = store.revision };
    state.assignments[0] = testAssignment(final_port, 0);
    const final_ticket = try store.begin(&state, &.{testFact()});
    store.revision = std.math.maxInt(u64) - 2;
    try t.expect(try store.stop(3));
    try t.expect(store.revision == std.math.maxInt(u64) - 1 and store.pending.id == final_ticket.id);
    _ = try store.finish(final_ticket, abi.gfx_output_outcome_lost, 0);
    try t.expect(store.revision == std.math.maxInt(u64) and store.retained == 3);
    try t.expectError(error.Exhausted, store.retireAfterReset(final_ticket));
    try t.expect(store.retained == 3);
}

test "receiver capacity and legacy table prefix preserve previous publications and caller canaries" {
    const t = std.testing;
    const store = try t.allocator.create(Store);
    defer t.allocator.destroy(store);
    store.* = .{};
    const owner: DriverOwner = .{ .kind = .driver, .id = 3, .generation = 9 };
    const first = try store.registerSource(owner, 9);
    const second = try store.registerSource(owner, 10);
    const third = try store.registerSource(owner, 11);
    const records = try t.allocator.alloc(abi.GfxReceiverInfo, 32);
    defer t.allocator.free(records);
    for (records, 0..) |*record, i| record.* = .{ .connector_id = @intCast(i + 1) };
    try store.replaceReceivers(owner, first, 1, records);
    try store.replaceReceivers(owner, second, 1, records);
    const old = store.infoAt(0).?;
    try t.expectError(error.Capacity, store.replaceReceivers(owner, third, 1, records[0..1]));
    try t.expect(store.cursor().present == capacity and store.revision == old.topology_revision);
    try t.expect(try store.closeSource(owner, first));
    try t.expect(store.infoAt(0).?.identity.adapter_id == 10 and store.infoAt(31).?.identity.connector_id == 32);
    const rebound = try store.registerSource(owner, 9);
    try t.expect(rebound.generation > first.generation);
    try t.expect(try store.stop(@intCast(owner.id)));
    try t.expectError(error.Stale, store.replaceReceivers(owner, rebound, 1, records));
    for ([_]u32{ 8, 23, 24, 31, 32, 40, 48, 55, 56, 63, 64, 71, 72, 79, 80, 87, 88, 95, 96, 103, 104, 111, 112, 119, 120, 127, 128, 135, 136, 143, 144, 151, 152, 159, 160, 167, 168, 175, 176 }) |bytes| {
        var storage: [176]u8 align(8) = @splat(0x79);
        const table: *abi.GfxDriverOutputApi = @ptrCast(&storage);
        table.version = 1; table.size = bytes;
        const before = storage;
        const code = driverTable(table, .{ .publish = 3, .withdraw = 4, .register_source = 5, .replace_receivers = 6, .close_source = 7, .color_publish = 9, .mode_read_color = 10, .refresh_publish = 11, .refresh_read = 12, .power_publish = 13, .power_read = 14, .brightness_publish = 15, .brightness_read = 16 });
        if (bytes < 24) {
            try t.expect(code == abi.gfx_output_error_invalid and std.mem.eql(u8, &storage, &before));
        } else {
            const returned = @min(bytes & ~@as(u32, 7), @sizeOf(abi.GfxDriverOutputApi));
            try t.expect(code == abi.gfx_output_ok and table.version == 1 and table.size == returned and table.publish == 3 and table.withdraw == 4);
            try t.expect(std.mem.allEqual(u8, storage[returned..], 0x79));
            if (returned >= 120) try t.expect(table.color_publish == 9);
            if (returned >= 128) try t.expect(table.mode_read_color == 10);
            if (returned >= 136) try t.expect(table.refresh_publish == 11);
            if (returned >= 144) try t.expect(table.refresh_read == 12);
            if (returned >= 152) try t.expect(table.power_publish == 13);
            if (returned >= 160) try t.expect(table.power_read == 14);
            if (returned >= 168) try t.expect(table.brightness_publish == 15);
            if (returned >= 176) try t.expect(table.brightness_read == 16);
        }
    }
}

test "receiver audio metadata rejects stale ELD and disappears on source close without video waits" {
    const t = std.testing;
    const store = try t.allocator.create(Store);
    defer t.allocator.destroy(store);
    // Both canonical PCI access methods retain their exact identity.
    for ([_]u32{1, 2}) |kind| {
        store.* = .{};
        const owner: DriverOwner = .{ .kind = .driver, .id = 3, .generation = 9 };
        const source = try store.registerSource(owner, 0x01000900);
        const record = try t.allocator.create(abi.GfxReceiverInfo);
        defer t.allocator.destroy(record);
        record.* = .{ .connector_id = 16, .flags = abi.gfx_output_flag_connected };
        try store.replaceReceivers(owner, source, 1, @as([*]const abi.GfxReceiverInfo, @ptrCast(record))[0..1]);
        var route: abi.GfxAudioRoute = .{ .source = source, .receiver_sequence = 1, .revision = 1,
            .connector_id = 16, .hda_location = (kind << 24) | 0x901, .hda_device = 0x228e10de,
            .head_id = 2, .device_entry = 0, .state = abi.gfx_audio_route_ready, .eld_bytes = 24,
            .port_id = .{16,0,0,0,0,0,0,0} };
        @memcpy(route.eld[8..16], &route.port_id);
        var invalid_bus = route;
        for ([_]u32{0, 3, 255}) |invalid| {
            invalid_bus.hda_location = (invalid << 24) | 0x901;
            try t.expectError(error.Invalid, store.publishAudio(owner, &invalid_bus));
        }
        try store.publishAudio(owner, &route);
        try store.publishAudio(owner, &route); // exact retry
        try t.expect(std.meta.eql(route, store.audio.query(route.hda_location, route.hda_device, 0).?));
        try t.expect(store.audio.query(route.hda_location ^ 1, route.hda_device, 0) == null);
        var other = route;
        other.connector_id = 32;
        try t.expectError(error.Routing, store.publishAudio(owner, &other));
        other = route; other.revision += 1; other.eld[8] ^= 1;
        try t.expectError(error.Invalid, store.publishAudio(owner, &other));
        try t.expectError(error.Stale, store.publishAudio(.{ .kind = .driver, .id = 3, .generation = 10 }, &route));
        // Receiver changes invalidate audio even during a retained video ticket.
        store.pending = .{ .id = 7 }; store.retained = 1;
        try store.replaceReceivers(owner, source, 2, &.{});
        const invalidated = store.audio.query(route.hda_location, route.hda_device, 0).?;
        try t.expect(invalidated.receiver_sequence == 2 and invalidated.state == abi.gfx_audio_route_pending and invalidated.eld_bytes == 0);
        try t.expect(std.mem.allEqual(u8, &invalidated.eld, 0));
        try t.expectError(error.Stale, store.publishAudio(owner, &route));
        route.receiver_sequence = 2;
        try t.expectError(error.Stale, store.publishAudio(owner, &route)); // same revision cannot restore invalid data
        route.revision += 1; route.state = abi.gfx_audio_route_absent; route.eld_bytes = 0; @memset(&route.eld, 0);
        try store.publishAudio(owner, &route);
        try t.expect(try store.closeSource(owner, source));
        try t.expect(store.audio.query(route.hda_location, route.hda_device, 0) == null);
        try t.expectError(error.Stale, store.publishAudio(owner, &route));
    }
}
