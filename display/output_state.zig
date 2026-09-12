// Pure bounded connector catalog and atomic validation. No device accesses,
// callback invocation, allocation or resource release is permitted here.
const std = @import("std");
pub const abi = @import("r4os_kernel_contract");
const bo = @import("../memory/gfx_buffer_layout.zig");
pub const Error = error{ Invalid, Stale, Busy, Unsupported, Capacity, Exhausted, Bandwidth, Routing, Dependency };
pub const capacity = abi.gfx_output_catalog_capacity;
pub const DriverOwner = @import("../memory/gfx_buffer_owner.zig").Owner;
const Source = struct {
    owner: DriverOwner = .{ .kind = .driver, .id = 0, .generation = 0 },
    binding: abi.GfxReceiverSource = .{},
    sequence: u64 = 0,
};
pub const Entry = struct {
    owner: u32 = 0,
    receiver_source: u64 = 0,
    info: abi.GfxOutputInfo = .{},
    modes: [abi.gfx_output_max_modes]abi.GfxOutputMode = .{abi.GfxOutputMode{}} ** abi.gfx_output_max_modes,
    edid: [abi.gfx_output_max_edid_bytes]u8 = .{0} ** abi.gfx_output_max_edid_bytes,
};
pub const Fact = struct { buffer: ?bo.Descriptor = null, dependency: enum { none, ready, pending, failed } = .none };
pub const Ticket = struct { id: u64 = 0 };
pub const Store = struct {
    entries: [capacity]Entry = .{Entry{}} ** capacity,
    sources: [16]Source = @splat(.{}),
    source_serial: u64 = 0,
    revision: u64 = 0,
    receiver_serial: u64 = 0,
    attempt: u64 = 0,
    commit_sequence: u64 = 0,
    pending: Ticket = .{},
    retained: u32 = 0,

    fn canChange(self: *const Store) Error!void {
        if (self.pending.id != 0 or self.retained != 0) return error.Busy;
        if (self.revision == std.math.maxInt(u64) or self.receiver_serial == std.math.maxInt(u64)) return error.Exhausted;
    }
    pub fn publish(self: *Store, owner: u32, info: abi.GfxOutputInfo, modes: []const abi.GfxOutputMode, edid: []const u8) Error!abi.GfxOutputId {
        try self.canChange();
        for (&self.sources) |*source| if (source.binding.generation != 0 and source.binding.adapter_id == info.identity.adapter_id) return error.Busy;
        if (!header(info) or !header(info.limits) or info.reserved0 != 0 or info.reserved1 != 0 or info.limits.reserved0 != 0 or
            info.identity.connector_id == 0 or info.identity.device_generation == 0 or
            info.identity.connection_generation != 0 or info.mode_count != modes.len or info.edid_bytes != edid.len or
            modes.len > abi.gfx_output_max_modes or edid.len > abi.gfx_output_max_edid_bytes or edid.len % 128 != 0 or
            info.flags & ~@as(u32, 31) != 0 or info.connector_kind > abi.gfx_output_kind_firmware) return error.Invalid;
        if ((owner == 0) != (info.identity.adapter_id == 0)) return error.Invalid;
        if (info.flags & abi.gfx_output_flag_connected == 0 and (modes.len != 0 or edid.len != 0 or info.preferred_mode_id != 0)) return error.Invalid;
        try validLimits(info.limits);
        if (info.possible_heads == 0 or info.possible_planes == 0 or info.possible_plls == 0 or
            info.possible_heads & ~info.limits.head_mask != 0 or info.possible_planes & ~info.limits.plane_mask != 0 or
            info.possible_plls & ~info.limits.pll_mask != 0) return error.Routing;
        var preferred_found = info.preferred_mode_id == 0;
        for (modes, 0..) |mode, i| {
            if (!validMode(mode)) return error.Invalid;
            for (modes[0..i]) |prior| if (prior.mode_id == mode.mode_id) return error.Invalid;
            if (info.preferred_mode_id == mode.mode_id) preferred_found = true;
        }
        if (!preferred_found) return error.Invalid;
        var selected: ?usize = null;
        var vacant: ?usize = null;
        for (&self.entries, 0..) |*entry, index| {
            if (entry.info.identity.connector_id == 0) { if (vacant == null) vacant = index; continue; }
            if (entry.info.identity.adapter_id != info.identity.adapter_id) continue;
            // Source limits are adapter-wide. A driver must withdraw/rebind a
            // complete device before changing that adapter capability contract.
            if (entry.info.identity.device_generation == info.identity.device_generation and
                !std.meta.eql(entry.info.limits, info.limits)) return error.Invalid;
            if (entry.info.identity.connector_id != info.identity.connector_id) continue;
            if (entry.owner != owner and entry.owner != 0) return error.Busy;
            selected = index;
        }
        const index = selected orelse vacant orelse return error.Capacity;
        var identity = info.identity;
        self.receiver_serial += 1;
        self.revision += 1;
        identity.connection_generation = self.receiver_serial;
        const entry = &self.entries[index];
        entry.* = .{ .owner = owner, .info = info };
        entry.info.identity = identity;
        entry.info.topology_revision = self.revision;
        @memcpy(entry.modes[0..modes.len], modes);
        @memcpy(entry.edid[0..edid.len], edid);
        return identity;
    }
    pub fn find(self: *const Store, identity: abi.GfxOutputId) Error!*const Entry {
        for (&self.entries) |*entry| if (entry.info.identity.connector_id != 0 and std.meta.eql(entry.info.identity, identity)) return entry;
        return error.Stale;
    }
    pub fn infoAt(self: *const Store, index: u32) ?abi.GfxOutputInfo {
        var ordinal: u32 = 0;
        for (&self.entries) |*entry| {
            if (entry.info.identity.connector_id == 0) continue;
            if (ordinal == index) {
                var result = entry.info;
                result.topology_revision = self.revision;
                return result;
            }
            ordinal += 1;
        }
        return null;
    }
    pub fn modeAt(self: *const Store, identity: abi.GfxOutputId, index: u32) Error!?abi.GfxOutputMode {
        const entry = try self.find(identity);
        return if (index < entry.info.mode_count) entry.modes[index] else null;
    }
    pub fn withdraw(self: *Store, owner: u32, identity: abi.GfxOutputId) Error!void {
        try self.canChange();
        const entry = try self.find(identity);
        if (owner == 0 or entry.owner != owner or entry.receiver_source != 0) return error.Stale;
        self.disconnect(@constCast(entry));
    }
    fn disconnect(self: *Store, entry: *Entry) void {
        self.receiver_serial += 1;
        self.revision += 1;
        entry.info.identity.connection_generation = self.receiver_serial;
        entry.info.topology_revision = self.revision;
        entry.info.flags &= ~(abi.gfx_output_flag_connected | abi.gfx_output_flag_active);
        entry.info.mode_count = 0;
        entry.info.preferred_mode_id = 0;
        entry.info.edid_bytes = 0;
        @memset(&entry.modes, .{});
        @memset(&entry.edid, 0);
    }
    pub fn stop(self: *Store, owner: u32) Error!bool {
        if (owner == 0) return error.Invalid;
        var receivers_changed = false;
        for (&self.sources) |*source| if (source.binding.generation != 0 and source.owner.id == owner) {
            receivers_changed = self.eraseReceivers(source.binding.generation) or receivers_changed;
            source.* = .{};
        };
        try self.canChange();
        var count: u64 = 0;
        for (&self.entries) |*entry| if (entry.owner == owner) { count += 1; };
        if (self.receiver_serial > std.math.maxInt(u64) - count or self.revision > std.math.maxInt(u64) - count) return error.Exhausted;
        for (&self.entries) |*entry| if (entry.owner == owner) {
            self.disconnect(entry);
            entry.owner = 0; // Rebinding reuses the stable port slot with new generations.
        };
        return count != 0 or receivers_changed;
    }
    pub fn registerSource(self: *Store, owner: DriverOwner, adapter: u32) Error!abi.GfxReceiverSource {
        if (owner.kind != .driver or !owner.valid() or owner.id > std.math.maxInt(u32) or adapter == 0) return error.Invalid;
        if (self.source_serial == std.math.maxInt(u64) or self.revision == std.math.maxInt(u64)) return error.Exhausted;
        for (&self.entries) |*entry| if (entry.info.identity.connector_id != 0 and entry.info.identity.adapter_id == adapter) return error.Busy;
        for (&self.sources) |*source| if (source.binding.generation != 0 and source.binding.adapter_id == adapter) return error.Busy;
        for (&self.sources) |*source| if (source.binding.generation == 0) {
            self.source_serial += 1;
            source.* = .{ .owner = owner, .binding = .{ .adapter_id = adapter, .generation = self.source_serial } };
            return source.binding;
        };
        return error.Capacity;
    }
    fn receiverSource(self: *Store, owner: DriverOwner, binding: abi.GfxReceiverSource) Error!*Source {
        if (binding.adapter_id == 0 or binding.generation == 0 or binding.reserved0 != 0) return error.Invalid;
        for (&self.sources) |*source| if (source.binding.generation == binding.generation and
            std.meta.eql(source.binding, binding) and source.owner.eql(owner)) return source;
        return error.Stale;
    }
    // Metadata receivers cannot participate in a hardware commit. They may
    // therefore be invalidated while unrelated BOs or commit tickets are held.
    // Reserve the pending ticket's finish increment even at counter exhaustion.
    fn receiverChange(self: *Store) void {
        const ceiling = std.math.maxInt(u64) - @as(u64, @intFromBool(self.pending.id != 0));
        if (self.revision < ceiling) self.revision += 1;
    }
    fn eraseReceivers(self: *Store, generation: u64) bool {
        var changed = false;
        for (&self.entries) |*entry| if (entry.receiver_source == generation) {
            entry.* = .{};
            changed = true;
        };
        if (changed) self.receiverChange();
        return changed;
    }
    pub fn closeSource(self: *Store, owner: DriverOwner, binding: abi.GfxReceiverSource) Error!bool {
        const source = try self.receiverSource(owner, binding);
        const changed = self.eraseReceivers(binding.generation);
        source.* = .{};
        return changed;
    }
    pub fn replaceReceivers(self: *Store, owner: DriverOwner, binding: abi.GfxReceiverSource,
        sequence: u64, records: []const abi.GfxReceiverInfo) Error!void
    {
        const source = try self.receiverSource(owner, binding);
        if (sequence == 0 or sequence <= source.sequence) return error.Stale;
        if (records.len > abi.gfx_receiver_max_outputs) return error.Invalid;
        const ceiling = std.math.maxInt(u64) - @as(u64, @intFromBool(self.pending.id != 0));
        if (self.revision >= ceiling or self.receiver_serial > std.math.maxInt(u64) - records.len) return error.Exhausted;
        var available: usize = 0;
        for (&self.entries) |*entry| if (entry.info.identity.connector_id == 0 or entry.receiver_source == binding.generation) { available += 1; };
        if (records.len > available) return error.Capacity;
        // Complete preflight before changing even one old receiver. The R4D
        // owns these resident records exclusively for the whole callback.
        for (records, 0..) |*record, i| {
            try validReceiver(record);
            for (records[0..i]) |*prior| if (record.connector_id == prior.connector_id) return error.Invalid;
        }
        for (&self.entries) |*entry| if (entry.receiver_source == binding.generation) { entry.* = .{}; };
        self.revision += 1;
        source.sequence = sequence;
        var slot: usize = 0;
        for (records) |*record| {
            while (self.entries[slot].info.identity.connector_id != 0) : (slot += 1) {}
            const entry = &self.entries[slot];
            slot += 1;
            self.receiver_serial += 1;
            entry.* = .{ .owner = @intCast(owner.id), .receiver_source = binding.generation, .info = .{
                .identity = .{ .adapter_id = binding.adapter_id, .device_generation = binding.generation,
                    .connector_id = record.connector_id, .connection_generation = self.receiver_serial },
                .topology_revision = self.revision, .flags = record.flags | abi.gfx_output_flag_receiver_only,
                .connector_kind = record.connector_kind, .mode_count = record.mode_count,
                .preferred_mode_id = record.preferred_mode_id, .edid_bytes = record.edid_bytes,
                .limits = unknown_limits } };
            @memcpy(entry.modes[0..record.mode_count], record.modes[0..record.mode_count]);
            @memcpy(entry.edid[0..record.edid_bytes], record.edid[0..record.edid_bytes]);
        }
    }
    pub fn cursor(self: *const Store) abi.GfxDisplayRevision {
        var present: u32 = 0;
        for (&self.entries) |*entry| if (entry.info.identity.connector_id != 0) { present += 1; };
        return .{ .revision = self.revision, .present = present };
    }
    pub fn validate(self: *const Store, state: *const abi.GfxAtomicState, facts: []const Fact) Error!abi.GfxAtomicResult {
        if (!header(state.*) or state.reserved0 != 0 or state.count == 0 or state.count > abi.gfx_output_max_assignments or facts.len != state.count) return error.Invalid;
        if (state.topology_revision != self.revision) return error.Stale;
        if (self.pending.id != 0 or self.retained != 0) return error.Busy;
        for (state.assignments[state.count..]) |entry| if (!std.meta.eql(entry, abi.GfxScanoutState{})) return error.Invalid;
        var heads: u32 = 0; var planes: u32 = 0; var plls: u32 = 0;
        var bandwidth: u128 = 0; var clocks: u128 = 0;
        const adapter = state.assignments[0].output;
        var limits: abi.GfxDisplayLimits = .{};
        for (state.assignments[0..state.count], facts, 0..) |assignment, fact, i| {
            if (!header(assignment) or assignment.reserved0 != 0 or assignment.buffer.reserved0 != 0 or
                assignment.output.adapter_id != adapter.adapter_id or assignment.output.device_generation != adapter.device_generation) return error.Invalid;
            const entry = try self.find(assignment.output);
            if (entry.info.flags & abi.gfx_output_flag_connected == 0) return error.Stale;
            if (entry.info.flags & abi.gfx_output_flag_receiver_only != 0) return error.Unsupported;
            if (i == 0) limits = entry.info.limits else if (!std.meta.eql(limits, entry.info.limits)) return error.Invalid;
            for (state.assignments[0..i]) |prior| if (prior.output.connector_id == assignment.output.connector_id) return error.Routing;
            const hb = try bit(assignment.head_id); const pb = try bit(assignment.plane_id); const cb = try bit(assignment.pll_id);
            if (hb & entry.info.possible_heads == 0 or pb & entry.info.possible_planes == 0 or cb & entry.info.possible_plls == 0 or
                heads & hb != 0 or planes & pb != 0 or plls & cb != 0) return error.Routing;
            heads |= hb; planes |= pb; plls |= cb;
            var selected: ?abi.GfxOutputMode = null;
            for (entry.modes[0..entry.info.mode_count]) |candidate| if (candidate.mode_id == assignment.mode_id) { selected = candidate; break; };
            const chosen = selected orelse return error.Stale;
            if (assignment.rotation > 3 or assignment.color > 3 or assignment.bits_per_color > 16 or assignment.bits_per_color < 6 or
                limits.rotations & (try bit(assignment.rotation)) == 0 or limits.colors & (try bit(assignment.color)) == 0 or
                limits.bpc_mask & (try bit(assignment.bits_per_color)) == 0) return error.Unsupported;
            if (chosen.flags & abi.gfx_output_mode_interlaced != 0 and limits.flags & abi.gfx_output_limit_interlace == 0) return error.Unsupported;
            if (chosen.flags & abi.gfx_output_mode_420_only != 0 and assignment.color != 3) return error.Unsupported;
            if (assignment.color == 3 and chosen.flags & (abi.gfx_output_mode_420_only | abi.gfx_output_mode_420_allowed) == 0) return error.Unsupported;
            if (!fits(chosen.width, assignment.destination_x, assignment.destination_width) or
                !fits(chosen.height, assignment.destination_y, assignment.destination_height)) return error.Invalid;
            const sw = if (assignment.rotation & 1 == 0) assignment.source_width else assignment.source_height;
            const sh = if (assignment.rotation & 1 == 0) assignment.source_height else assignment.source_width;
            if ((sw != assignment.destination_width or sh != assignment.destination_height) and limits.flags & abi.gfx_output_limit_scaling == 0) return error.Unsupported;
            if (assignment.source_width > limits.max_width or assignment.source_height > limits.max_height or chosen.width > limits.max_width or chosen.height > limits.max_height) return error.Unsupported;
            if (fact.dependency == .pending) return error.Busy;
            if (fact.dependency == .failed) return error.Dependency;
            if (std.meta.eql(assignment.ready_fence, abi.GfxFence{}) != (fact.dependency == .none)) return error.Invalid;
            const firmware = entry.info.flags & (abi.gfx_output_flag_fixed_geometry | abi.gfx_output_flag_firmware_snapshot) ==
                (abi.gfx_output_flag_fixed_geometry | abi.gfx_output_flag_firmware_snapshot);
            if (firmware) {
                if (state.count != 1 or !std.meta.eql(assignment.buffer, abi.GfxBufferHandle{}) or fact.buffer != null or fact.dependency != .none or
                    assignment.rotation != 0 or assignment.color != 0 or assignment.bits_per_color != 8 or
                    assignment.source_x != 0 or assignment.source_y != 0 or assignment.destination_x != 0 or assignment.destination_y != 0 or
                    assignment.source_width != chosen.width or assignment.source_height != chosen.height or
                    assignment.destination_width != chosen.width or assignment.destination_height != chosen.height) return error.Unsupported;
                continue;
            }
            if (limits.flags & abi.gfx_output_limit_modeset == 0 or chosen.flags & abi.gfx_output_mode_geometry_only != 0) return error.Unsupported;
            const descriptor = fact.buffer orelse return error.Invalid;
            if (assignment.buffer.id == 0 or assignment.buffer.generation == 0) return error.Invalid;
            _ = bo.validate(descriptor) catch return error.Invalid;
            if (!descriptor.binding.portable() and (descriptor.binding.adapter != adapter.adapter_id or descriptor.binding.device_generation != adapter.device_generation)) return error.Stale;
            if (descriptor.usage & bo.Usage.scanout == 0 or descriptor.modifier != 0 or limits.modifiers & 1 == 0) return error.Unsupported;
            // YUV memory plane/subsampling and non-linear modifiers require an
            // explicit validated scanout format path; never guess byte layout.
            const format: u32 = switch (descriptor.format) { .xrgb8888 => 1, .argb8888 => 2, else => return error.Unsupported };
            if (limits.formats & format == 0 or descriptor.plane_count != 1 or descriptor.planes[0].pitch % limits.pitch_alignment != 0) return error.Unsupported;
            if (!fits(descriptor.width, assignment.source_x, assignment.source_width) or !fits(descriptor.height, assignment.source_y, assignment.source_height)) return error.Invalid;
            clocks += chosen.pixel_clock_hz;
            bandwidth += (@as(u128, descriptor.planes[0].pitch) * descriptor.height * chosen.refresh_millihz + 999) / 1000;
            if (chosen.pixel_clock_hz > limits.max_pixel_clock_hz or clocks > limits.total_pixel_clock_hz or bandwidth > limits.bandwidth_bytes_per_second) return error.Bandwidth;
        }
        return .{ .topology_revision = self.revision, .commit_sequence = self.commit_sequence };
    }
    pub fn begin(self: *Store, state: *const abi.GfxAtomicState, facts: []const Fact) Error!Ticket {
        _ = try self.validate(state, facts);
        try self.canChange();
        if (self.attempt == std.math.maxInt(u64) or self.commit_sequence == std.math.maxInt(u64)) return error.Exhausted;
        self.attempt += 1;
        self.pending = .{ .id = self.attempt };
        return self.pending;
    }
    // The caller owns real old/new BO leases. Outcome alone cannot authorize
    // release: applied needs old quiescence; rollback needs new quiescence.
    pub fn finish(self: *Store, ticket: Ticket, outcome: u32, quiesced: u32) Error!abi.GfxAtomicResult {
        if (ticket.id == 0 or ticket.id != self.pending.id) return error.Stale;
        if (quiesced & ~@as(u32, 3) != 0) return error.Invalid;
        const retain: u32 = switch (outcome) {
            abi.gfx_output_outcome_applied => if (quiesced & 1 != 0) 2 else return error.Busy,
            abi.gfx_output_outcome_old_preserved => if (quiesced & 2 != 0) 1 else return error.Busy,
            abi.gfx_output_outcome_lost => 3 & ~quiesced,
            else => return error.Invalid,
        };
        self.pending = .{};
        self.revision += 1;
        if (outcome == abi.gfx_output_outcome_applied) self.commit_sequence += 1;
        if (outcome == abi.gfx_output_outcome_lost) self.retained = retain;
        return .{ .topology_revision = self.revision, .commit_sequence = self.commit_sequence, .outcome = outcome, .retained = retain };
    }
};
fn header(value: anytype) bool { return value.version == 1 and value.size >= @sizeOf(@TypeOf(value)); }
pub const unknown_limits = blk: {
    var value = std.mem.zeroes(abi.GfxDisplayLimits);
    value.version = 1;
    value.size = @sizeOf(abi.GfxDisplayLimits);
    break :blk value;
};
pub fn driverTable(output: *abi.GfxDriverOutputApi, functions: abi.GfxDriverOutputApi) i32 {
    if (@intFromPtr(output) == 0 or output.version != 1 or output.size < 24) return abi.gfx_output_error_invalid;
    var value = functions;
    value.size = @min(output.size & ~@as(u32, 7), @sizeOf(abi.GfxDriverOutputApi));
    @memcpy(@as([*]u8, @ptrCast(output))[0..value.size], std.mem.asBytes(&value)[0..value.size]);
    return abi.gfx_output_ok;
}
fn validReceiver(value: *const abi.GfxReceiverInfo) Error!void {
    const flags = abi.gfx_output_flag_connected | abi.gfx_output_flag_connection_unknown |
        abi.gfx_output_flag_edid_missing | abi.gfx_output_flag_edid_invalid |
        abi.gfx_output_flag_receiver_incomplete | abi.gfx_output_flag_query_failed;
    if (value.connector_id == 0 or value.reserved0 != 0 or value.flags & ~flags != 0 or
        value.connector_kind > abi.gfx_output_kind_virtual or value.mode_count > abi.gfx_output_max_modes or
        value.edid_bytes > abi.gfx_output_max_edid_bytes or value.edid_bytes % 128 != 0) return error.Invalid;
    const connected = value.flags & abi.gfx_output_flag_connected != 0;
    if (connected and value.flags & abi.gfx_output_flag_connection_unknown != 0) return error.Invalid;
    if (!connected and (value.mode_count != 0 or value.edid_bytes != 0 or value.preferred_mode_id != 0 or
        value.flags & (abi.gfx_output_flag_edid_missing | abi.gfx_output_flag_edid_invalid) != 0)) return error.Invalid;
    if (value.flags & abi.gfx_output_flag_edid_missing != 0 and value.edid_bytes != 0) return error.Invalid;
    if (value.flags & abi.gfx_output_flag_edid_invalid != 0 and value.mode_count != 0) return error.Invalid;
    var preferred = value.preferred_mode_id == 0;
    for (value.modes[0..value.mode_count], 0..) |mode, i| {
        if (!validMode(mode) or mode.flags & abi.gfx_output_mode_geometry_only != 0) return error.Invalid;
        for (value.modes[0..i]) |prior| if (prior.mode_id == mode.mode_id) return error.Invalid;
        if (mode.mode_id == value.preferred_mode_id) preferred = true;
    }
    if (!preferred) return error.Invalid;
}
fn bit(index: u32) Error!u32 { if (index >= 32) return error.Routing; return @as(u32, 1) << @as(u5, @intCast(index)); }
fn fits(total: u32, offset: u32, size: u32) bool { return size != 0 and offset < total and size <= total - offset; }
fn validLimits(value: abi.GfxDisplayLimits) Error!void {
    if (value.head_mask == 0 or value.plane_mask == 0 or value.pll_mask == 0 or value.rotations == 0 or value.rotations & ~@as(u32, 15) != 0 or
        value.colors == 0 or value.colors & ~@as(u32, 15) != 0 or value.bpc_mask & 0x1_5555 == 0 or value.bpc_mask & ~@as(u32, 0x1_5540) != 0 or
        value.flags & ~@as(u32, 7) != 0 or !std.math.isPowerOfTwo(value.pitch_alignment) or value.pitch_alignment > 65536 or
        value.max_width == 0 or value.max_height == 0 or value.max_width > 65536 or value.max_height > 65536 or value.formats == 0 or
        value.formats & ~@as(u32, 15) != 0 or value.modifiers != 1) return error.Invalid;
    if (value.flags & abi.gfx_output_limit_modeset != 0 and (value.bandwidth_bytes_per_second == 0 or value.max_pixel_clock_hz == 0 or value.total_pixel_clock_hz == 0)) return error.Invalid;
}
pub fn validMode(mode: abi.GfxOutputMode) bool {
    if (!header(mode) or mode.reserved0 != 0 or mode.mode_id == 0 or mode.width == 0 or mode.height == 0 or mode.width > 65536 or mode.height > 65536 or mode.flags & ~@as(u32, 127) != 0) return false;
    if (mode.flags & abi.gfx_output_mode_geometry_only != 0) return mode.pixel_clock_hz == 0 and mode.refresh_millihz == 0 and mode.h_total == 0 and mode.v_total == 0 and
        mode.h_sync_start == 0 and mode.h_sync_end == 0 and mode.v_sync_start == 0 and mode.v_sync_end == 0;
    if (mode.pixel_clock_hz == 0 or mode.pixel_clock_hz > 20_000_000_000 or mode.h_total > 131072 or mode.v_total > 131072 or
        mode.width > mode.h_sync_start or mode.h_sync_start >= mode.h_sync_end or mode.h_sync_end > mode.h_total or
        mode.height > mode.v_sync_start or mode.v_sync_start >= mode.v_sync_end or mode.v_sync_end > mode.v_total) return false;
    const expected = mode.pixel_clock_hz * 1000 * (if (mode.flags & 1 != 0) @as(u64, 2) else 1) / (@as(u64, mode.h_total) * mode.v_total);
    return expected > 0 and expected == mode.refresh_millihz;
}

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
    const revision = store.revision;
    try store.withdraw(14, first);
    try t.expect(store.revision > revision);
    try t.expectError(error.Stale, store.find(first));
    const empty = store.infoAt(0).?;
    try t.expectEqual(@as(u32, 0), empty.edid_bytes);
    try t.expectEqual(@as(u32, 0), empty.mode_count);
    try t.expectEqual(@as(u32, 0), empty.preferred_mode_id);
    const second = try store.publish(14, info, &.{testMode()}, &data);
    try t.expect(first.connector_id == second.connector_id and second.connection_generation > first.connection_generation);
    try t.expectError(error.Stale, store.modeAt(first, 0));
    try t.expect((try store.modeAt(second, 0)).?.width == 640);
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
    try t.expectError(error.Busy, store.withdraw(14, id));
    try t.expectError(error.Busy, store.finish(ticket, abi.gfx_output_outcome_old_preserved, 0));
    const rollback = try store.finish(ticket, abi.gfx_output_outcome_old_preserved, abi.gfx_output_retain_new);
    try t.expectEqual(@as(u32, 1), rollback.retained);
    try t.expectEqual(@as(u64, 0), rollback.commit_sequence);
    try t.expectError(error.Stale, store.begin(&state, &.{testFact()}));
    state.topology_revision = store.revision;
    const next = try store.begin(&state, &.{testFact()});
    try t.expectError(error.Stale, store.finish(ticket, abi.gfx_output_outcome_applied, 3));
    const lost = try store.finish(next, abi.gfx_output_outcome_lost, 0);
    try t.expectEqual(@as(u32, 3), lost.retained);
    state.topology_revision = store.revision;
    try t.expectError(error.Busy, store.begin(&state, &.{testFact()}));
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
    try store.replaceReceivers(owner, binding, 1, records);
    try t.expectEqual(@as(u32, 33), store.cursor().present);
    const first = store.infoAt(1).?;
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
    for ([_]u32{ 8, 23, 24, 31, 32, 40, 48, 56 }) |bytes| {
        var storage: [64]u8 align(8) = @splat(0x79);
        const table: *abi.GfxDriverOutputApi = @ptrCast(&storage);
        table.version = 1; table.size = bytes;
        const before = storage;
        const code = driverTable(table, .{ .publish = 3, .withdraw = 4, .register_source = 5, .replace_receivers = 6, .close_source = 7 });
        if (bytes < 24) {
            try t.expect(code == abi.gfx_output_error_invalid and std.mem.eql(u8, &storage, &before));
        } else {
            const returned = @min(bytes & ~@as(u32, 7), 48);
            try t.expect(code == abi.gfx_output_ok and table.version == 1 and table.size == returned and table.publish == 3 and table.withdraw == 4);
            try t.expect(std.mem.allEqual(u8, storage[returned..], 0x79));
        }
    }
}
