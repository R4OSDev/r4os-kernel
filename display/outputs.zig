// Privileged output publication boundary. Parsing/policy lives in R4GFX/R4D.
// All state is copied under the existing program metadata owner; no second
// event system, framebuffer writer or driver callback is introduced here.
const std = @import("std");
pub const model = @import("output_state.zig");
pub const abi = model.abi;
const ownership = @import("ownership.zig");
const events = @import("../kernel/desktop_events.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const queue = @import("queue.zig");
const queue_api = @import("../program/gfx_queue_api.zig");
const buffer_api = @import("../program/gfx_buffer_api.zig");
const display = @import("display.zig");
const irq = @import("../kernel/irq_router.zig");
pub const Error = model.Error || queue.Error;
var catalog: model.Store = .{};
// Reset/unload advances this even before the first connector publication.
// A queue binding checked before that transition cannot publish afterwards.
var owner_epoch: u64 = 1;
var epoch_exhausted = false;
var native_owner: u32 = 0;
var native_port: abi.GfxOutputId = .{};
fn samePort(left: abi.GfxOutputId, right: abi.GfxOutputId) bool {
    return left.adapter_id == right.adapter_id and left.connector_id == right.connector_id and left.device_generation == right.device_generation;
}

pub fn initBoot(frame: *const @import("framebuffer.zig").Framebuffer) void {
    if (frame.width == 0 or frame.height == 0 or frame.width > 65536 or frame.height > 65536) return;
    var data: [abi.gfx_output_max_edid_bytes]u8 = .{0} ** abi.gfx_output_max_edid_bytes;
    var bytes: usize = 0;
    if (frame.edid) |pointer| if (frame.edid_size <= data.len and frame.edid_size % 128 == 0) {
        bytes = @intCast(frame.edid_size);
        const source: [*]const u8 = @ptrCast(pointer);
        @memcpy(data[0..bytes], source[0..bytes]);
    };
    const width: u32 = @intCast(frame.width); const height: u32 = @intCast(frame.height);
    const info = abi.GfxOutputInfo{ .identity = .{ .connector_id = 1, .device_generation = 1 },
        .flags = abi.gfx_output_flag_connected | abi.gfx_output_flag_firmware_snapshot | abi.gfx_output_flag_active | abi.gfx_output_flag_fixed_geometry | abi.gfx_output_flag_connection_unknown,
        .connector_kind = abi.gfx_output_kind_firmware, .mode_count = 1, .preferred_mode_id = 1, .edid_bytes = @intCast(bytes),
        .possible_heads = 1, .possible_planes = 1, .possible_plls = 1,
        .limits = .{ .head_mask = 1, .plane_mask = 1, .pll_mask = 1, .max_width = width, .max_height = height } };
    const mode = abi.GfxOutputMode{ .mode_id = 1, .flags = abi.gfx_output_mode_geometry_only | abi.gfx_output_mode_preferred, .width = width, .height = height };
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (catalog.revision != 0) return;
    _ = catalog.publish(0, info, &.{mode}, data[0..bytes]) catch return;
    // Boot runs before the scheduler activity wait exists. First consumers
    // simply observe the initial revision; no wake is necessary here.
}
pub fn revision() abi.GfxDisplayRevision {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return catalog.cursor();
}
pub fn infoAt(index: u32) ?abi.GfxOutputInfo {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return catalog.infoAt(index);
}
pub fn modeAt(identity: abi.GfxOutputId, index: u32) Error!?abi.GfxOutputMode {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return catalog.modeAt(identity, index);
}
pub fn edidAt(identity: abi.GfxOutputId, index: u32) Error!?abi.GfxEdidBlock {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    const entry = try catalog.find(identity);
    if (index >= entry.info.edid_bytes / 128) return null;
    var result = abi.GfxEdidBlock{ .identity = identity, .block_index = index };
    @memcpy(&result.data, entry.edid[index * 128 ..][0..128]);
    return result;
}
pub fn publish(owner: u32, input: *const abi.GfxOutputPublication) Error!abi.GfxOutputId {
    if (owner == 0 or irq.inDispatch() or input.version != 1 or input.size < @sizeOf(abi.GfxOutputPublication) or
        input.info.mode_count > abi.gfx_output_max_modes or input.info.edid_bytes > abi.gfx_output_max_edid_bytes) return error.Invalid;
    if (input.info.flags & (abi.gfx_output_flag_firmware_snapshot | abi.gfx_output_flag_active | abi.gfx_output_flag_fixed_geometry) != 0 or
        input.info.identity.adapter_id != input.backend.adapter_id or input.info.identity.device_generation != input.backend.device_generation) return error.Invalid;
    const modeset = input.info.limits.flags & abi.gfx_output_limit_modeset != 0;
    if (modeset and !@import("native_driver.zig").modesEnabled(owner, input.backend)) return error.Unsupported;
    for (input.modes[input.info.mode_count..]) |value| if (!std.meta.eql(value, abi.GfxOutputMode{})) return error.Invalid;
    for (input.edid[input.info.edid_bytes..]) |value| if (value != 0) return error.Invalid;
    const epoch = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (epoch_exhausted) return error.Exhausted;
        break :blk owner_epoch;
    };
    try queue.validateOutputBinding(owner, input.backend);
    const identity = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (epoch_exhausted or owner_epoch != epoch) return error.Stale;
        var info = input.info;
        if (owner == native_owner and samePort(info.identity, native_port) and nativePresence(info.flags, true))
            info.flags |= abi.gfx_output_flag_active | (if (modeset) @as(u32, 0) else abi.gfx_output_flag_fixed_geometry);
        break :blk if (modeset) try catalog.publishModeset(owner, info, input.modes[0..info.mode_count], input.edid[0..info.edid_bytes]) else
            try catalog.publish(owner, info, input.modes[0..info.mode_count], input.edid[0..info.edid_bytes]);
    };
    events.signal(); // Complete topology is visible before sequence + wake.
    return identity;
}
pub fn withdraw(owner: u32, identity: abi.GfxOutputId) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        const entry = try catalog.find(identity);
        if (owner == 0 or entry.owner != owner or entry.receiver_source != 0) return error.Stale;
    }
    // Request rollback before removing the identity used by its receipt.
    // The driver retries withdrawal after draining its real GPU operation.
    if (@import("mode_work.zig").withdrawingOutput(owner, identity)) return error.Busy;
    {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        try catalog.withdraw(owner, identity);
    }
    events.signal();
}
pub fn pause(owner: u32, identity: abi.GfxOutputId, paused: bool) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (native_owner != owner or !samePort(native_port, identity)) return error.Stale;
        const entry = try catalog.find(identity);
        if (!paused and !nativePresence(entry.info.flags, true)) return error.Stale;
        break :blk try catalog.pause(owner, identity, paused, true);
    };
    if (paused) _ = @import("mode_work.zig").withdrawingOutput(owner, identity);
    if (changed) events.signal();
}
pub fn nativePaused(owner: u32, identity: abi.GfxOutputId) bool {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    for (&catalog.entries) |*entry| if (entry.owner == owner and entry.receiver_source == 0 and samePort(entry.info.identity, identity))
        return entry.paused;
    return false;
}
pub fn receiverEpoch() u64 {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return owner_epoch;
}
pub fn registerReceiverSource(owner: buffers.Owner, adapter: u32, epoch: u64) Error!abi.GfxReceiverSource {
    if (irq.inDispatch()) return error.Invalid;
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (epoch_exhausted) return error.Exhausted;
    if (epoch != owner_epoch) return error.Stale;
    return catalog.registerSource(owner, adapter);
}
pub fn replaceReceivers(owner: buffers.Owner, input: *const abi.GfxReceiverUpdate) Error!void {
    if (irq.inDispatch() or input.version != 1 or input.size < @sizeOf(abi.GfxReceiverUpdate) or
        input.reserved0 != 0 or input.count > abi.gfx_receiver_max_outputs or
        (input.count == 0) != (input.receivers == 0) or input.receivers % @alignOf(abi.GfxReceiverInfo) != 0 or
        input.receivers > std.math.maxInt(u64) - @as(u64, input.count) * @sizeOf(abi.GfxReceiverInfo)) return error.Invalid;
    // Unlike pageable R4X payloads this bridge accepts exclusively borrowed
    // resident R4D memory. Only bounded counted prefixes are copied; never a
    // 32-record snapshot on the worker/kernel stack.
    const records: []const abi.GfxReceiverInfo = if (input.count == 0) &.{} else
        @as([*]const abi.GfxReceiverInfo, @ptrFromInt(input.receivers))[0..input.count];
    {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (epoch_exhausted) return error.Exhausted;
        try catalog.replaceReceivers(owner, input.source, input.sequence, records);
    }
    events.signal();
}
pub fn closeReceiverSource(owner: buffers.Owner, binding: abi.GfxReceiverSource) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        break :blk try catalog.closeSource(owner, binding);
    };
    if (changed) events.signal();
}
pub fn publishAudio(owner: buffers.Owner, input: *const abi.GfxAudioRoute) Error!void {
    if (irq.inDispatch()) return error.Invalid;
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (epoch_exhausted) return error.Exhausted;
    try catalog.publishAudio(owner, input);
}
pub fn queryAudio(location: u32, device: u32, index: u32) ?abi.GfxAudioRoute {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return catalog.audio.query(location, device, index);
}
pub fn stoppedDriver(owner: u32) void {
    if (owner == 0) return;
    @import("mode_work.zig").stoppedDriver(owner);
    @import("cursor_work.zig").stoppedDriver(owner);
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (owner_epoch == std.math.maxInt(u64)) epoch_exhausted = true else owner_epoch += 1;
        if (native_owner == owner) { native_owner = 0; native_port = .{}; }
        break :blk catalog.stop(owner) catch {
            // Exhaustion is fail-closed: no stale receiver data survives.
            for (&catalog.entries) |*entry| if (entry.owner == owner) { entry.* = .{}; };
            epoch_exhausted = true;
            break :blk true;
        };
    };
    if (changed) events.signal();
}
pub fn bootActive(active: bool) void {
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        const entry = &catalog.entries[0];
        if (entry.info.identity.connector_id == 0 or entry.info.identity.adapter_id != 0) break :blk false;
        const was = entry.info.flags & abi.gfx_output_flag_active != 0;
        if (was == active) break :blk false;
        if (catalog.revision == std.math.maxInt(u64)) { epoch_exhausted = true; break :blk false; }
        if (active) entry.info.flags |= abi.gfx_output_flag_active else entry.info.flags &= ~abi.gfx_output_flag_active;
        catalog.revision += 1;
        break :blk true;
    };
    if (changed) events.signal();
}
pub fn validateNative(owner: u32, backend: abi.GfxBackendBinding, identity: abi.GfxOutputId, width: u32, height: u32, held: bool) Error!void {
    try queue.validateOutputBinding(owner, backend);
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (epoch_exhausted) return error.Exhausted;
    const entry = try catalog.find(identity);
    if (owner == 0 or entry.owner != owner or entry.receiver_source != 0 or identity.adapter_id != backend.adapter_id or
        identity.device_generation != backend.device_generation or !nativePresence(entry.info.flags, held)) return error.Stale;
    for (entry.modes[0..entry.info.mode_count]) |mode| if (mode.width == width and mode.height == height) return;
    return error.Unsupported;
}
pub fn cursorHead(owner: u32, identity: abi.GfxOutputId) Error!u32 {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (epoch_exhausted or owner == 0 or native_owner != owner or !samePort(identity, native_port)) return error.Stale;
    for (&catalog.entries) |*entry| if (entry.owner == owner and samePort(identity, entry.info.identity) and
        entry.info.flags & abi.gfx_output_flag_active != 0) {
        if (@popCount(entry.info.possible_heads) != 1 or entry.info.possible_heads & ~@as(u32, 255) != 0) return error.Unsupported;
        return @ctz(entry.info.possible_heads);
    };
    return error.Unsupported;
}
fn nativePresence(flags: u32, held: bool) bool {
    // An authenticated held boot route may keep driving an unresponsive sink.
    // This preserves the distinction between active scanout and connection.
    return flags & abi.gfx_output_flag_connected != 0 or
        (held and flags & abi.gfx_output_flag_connection_unknown != 0);
}
pub fn nativeActive(owner: u32, identity: abi.GfxOutputId, active: bool) void {
    const changed = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        if (owner == 0 or epoch_exhausted) break :blk false;
        if (active) {
            var found: ?abi.GfxOutputId = null;
            for (&catalog.entries) |*entry| if (entry.owner == owner and samePort(entry.info.identity, identity)) {
                found = entry.info.identity;
                break;
            };
            const selected = found orelse break :blk false;
            native_owner = owner; native_port = selected;
        } else {
            if (native_owner != owner or !samePort(native_port, identity)) break :blk false;
            native_owner = 0; native_port = .{};
        }
        // Recovery uses the original registration; receiver refresh may have
        // advanced its connection generation while the physical route stayed.
        for (&catalog.entries) |*entry| {
            if (entry.owner != owner or !samePort(entry.info.identity, identity)) continue;
            const was = entry.info.flags & abi.gfx_output_flag_active != 0;
            const enabled = active and !entry.paused and nativePresence(entry.info.flags, true);
            if (was == enabled) break :blk false;
            if (catalog.revision == std.math.maxInt(u64)) { epoch_exhausted = true; break :blk false; }
            if (enabled) entry.info.flags |= abi.gfx_output_flag_active |
                (if (entry.info.limits.flags & abi.gfx_output_limit_modeset != 0) @as(u32, 0) else abi.gfx_output_flag_fixed_geometry) else
                entry.info.flags &= ~(abi.gfx_output_flag_active | abi.gfx_output_flag_fixed_geometry);
            catalog.revision += 1;
            break :blk true;
        }
        break :blk false;
    };
    if (changed) events.signal();
}
fn gather(owner: buffers.Owner, state: *const abi.GfxAtomicState, result: *[abi.gfx_output_max_assignments]model.Fact) Error!void {
    if (state.count == 0 or state.count > result.len) return error.Invalid;
    for (state.assignments[0..state.count], result[0..state.count]) |assignment, *fact| {
        fact.* = .{};
        if (!std.meta.eql(assignment.buffer, abi.GfxBufferHandle{})) {
            const reference = try buffer_api.handle(assignment.buffer);
            buffers.lock();
            const descriptor = buffers.store.describe(reference, owner) catch |err| { buffers.unlock(); return err; };
            buffers.unlock();
            fact.buffer = descriptor;
        }
        if (!std.meta.eql(assignment.ready_fence, abi.GfxFence{})) {
            const status = try queue.query(queue_api.fence(assignment.ready_fence));
            fact.dependency = if (status.phase != .terminal or status.device_active or status.resources_held) .pending else
                if (status.result == .complete) .ready else .failed;
        }
    }
}
pub fn atomic(owner: buffers.Owner, state: *const abi.GfxAtomicState, commit: bool) Error!abi.GfxAtomicResult {
    if (irq.inDispatch()) return error.Invalid;
    var facts: [abi.gfx_output_max_assignments]model.Fact = .{model.Fact{}} ** abi.gfx_output_max_assignments;
    try gather(owner, state, &facts);
    if (!commit) {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        return catalog.validate(state, facts[0..state.count]);
    }
    // Shared admission with all normal presenters and the existing takeover
    // path. A fixed-geometry retention performs no firmware/device write.
    if (!display.beginOutputCommit()) return error.Busy;
    defer display.endOutputCommit();
    if (display.backendState().state != .bootfb or state.count != 1 or state.assignments[0].output.adapter_id != 0) return error.Unsupported;
    const result = blk: {
        const token = ownership.enterState(); defer ownership.leaveState(token);
        const ticket = try catalog.begin(state, facts[0..state.count]);
        var result = try catalog.finish(ticket, abi.gfx_output_outcome_applied, 3);
        result.retained = 0; // No new/old BO lease exists for firmware retention.
        break :blk result;
    };
    events.signal();
    return result;
}

pub const NativeAdmission = struct { ticket: model.Ticket, mode: abi.GfxOutputMode, revision: u64 };
pub fn nativeMode(owner: buffers.Owner, state: *const abi.GfxAtomicState) Error!abi.GfxOutputMode {
    var facts: [abi.gfx_output_max_assignments]model.Fact = @splat(.{});
    try gather(owner, state, &facts);
    if (state.count != 1 or state.assignments[0].output.adapter_id == 0) return error.Unsupported;
    const token = ownership.enterState(); defer ownership.leaveState(token);
    _ = try catalog.validate(state, facts[0..1]);
    const entry = try catalog.find(state.assignments[0].output);
    for (entry.modes[0..entry.info.mode_count]) |mode| if (mode.mode_id == state.assignments[0].mode_id) return mode;
    return error.Stale;
}
pub fn beginNative(owner: buffers.Owner, state: *const abi.GfxAtomicState) Error!NativeAdmission {
    var facts: [abi.gfx_output_max_assignments]model.Fact = @splat(.{});
    try gather(owner, state, &facts);
    if (state.count != 1 or state.assignments[0].output.adapter_id == 0) return error.Unsupported;
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (epoch_exhausted) return error.Exhausted;
    if (catalog.revision > std.math.maxInt(u64) - 3 or catalog.commit_sequence > std.math.maxInt(u64) - 2) return error.Exhausted;
    const entry = try catalog.find(state.assignments[0].output);
    for (entry.modes[0..entry.info.mode_count]) |mode| if (mode.mode_id == state.assignments[0].mode_id) {
        return .{ .ticket = try catalog.begin(state, facts[0..1]), .mode = mode, .revision = catalog.revision };
    };
    return error.Stale;
}
pub fn finishNative(ticket: model.Ticket, operation: u32, outcome: u32, quiesced: u32) Error!abi.GfxAtomicResult {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (operation == abi.gfx_mode_operation_apply) return catalog.finish(ticket, outcome, quiesced);
    if (catalog.pending.id != 0 or catalog.retained != 0) return error.Busy;
    if (catalog.revision == std.math.maxInt(u64) or catalog.commit_sequence == std.math.maxInt(u64)) return error.Exhausted;
    if (outcome == abi.gfx_output_outcome_lost) catalog.retained = 3;
    if (operation == abi.gfx_mode_operation_rollback and outcome == abi.gfx_output_outcome_old_preserved) catalog.commit_sequence += 1;
    catalog.revision += 1;
    return .{ .topology_revision = catalog.revision, .commit_sequence = catalog.commit_sequence, .outcome = outcome,
        .retained = if (outcome == abi.gfx_output_outcome_lost) 3 else if (outcome == abi.gfx_output_outcome_applied) 2 else 1 };
}
pub fn canFinishNative(ticket: model.Ticket, operation: u32) Error!void {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    if (operation == abi.gfx_mode_operation_apply) {
        if (ticket.id == 0 or catalog.pending.id != ticket.id) return error.Stale;
    } else if (catalog.pending.id != 0 or catalog.retained != 0) return error.Busy;
    if (catalog.revision == std.math.maxInt(u64) or catalog.commit_sequence == std.math.maxInt(u64)) return error.Exhausted;
}
