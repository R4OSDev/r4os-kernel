// Privileged R4D/display lifetime bridge. No PCI register programming or GPU
// command encoding belongs here. Display execution excludes concurrent CPU
// writers; the common queue retains source backing during device execution.
const std = @import("std");
const abi = @import("r4os_kernel_contract");
const display = @import("display.zig");
const boot_driver = @import("boot_driver.zig");
const framebuffer = @import("framebuffer.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const buffer_api = @import("../program/gfx_buffer_api.zig");
const driver = @import("../kernel/driver_api.zig");
const pci = @import("../platform/pci_inventory.zig");
const paging = @import("../memory/paging.zig");
const queue = @import("queue.zig");
const outputs = @import("outputs.zig");
const owner = @import("queue_resources.zig").display_owner;
const timer = @import("../kernel/timer.zig");
const monotonic = @import("../platform/monotonic.zig");
const irq = @import("../kernel/irq_router.zig");
const Error = outputs.Error || display.TransitionError;
const Callback = *const fn (u64, u64, *const abi.GfxNativeBootInfo) callconv(.c) i32;
const Bridge = struct {
    driver_owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    registration: abi.GfxNativeRegistration = .{},
    reference: buffers.Handle = .{},
    cpu_lease: buffers.Handle = .{},
    timeline: u64 = 0,
    pending: ?queue.model.Fence = null,
    bytes: u64 = 0,
    frame: framebuffer.Framebuffer = undefined,
    held_generation: u64 = 0,
    generation: u64 = 0,
    ready: bool = false,
    cancelled: bool = false,
    hardware_restored: bool = false,
    pixels_restored: bool = false,
    queue_stopped: bool = false,
};
var bridge: Bridge = .{};
// Different R4D owners can call this global bridge concurrently. Callbacks
// also enter from normal display presentation, outside a driver transition.
// Reentrance is required for a transition's own commit/CPU/restore callbacks;
// competing tasks get Busy. This guard permits waits and never holds an I/O
// owner across the queue completion or the actual driver callback.
var execution = @import("../sched/sync.zig").UnwindGuard.init("native-display-driver");
var retained_owner: u32 = 0;

fn binding(value: abi.GfxBackendBinding) queue.model.Binding {
    return .{ .adapter = value.adapter_id, .device_generation = value.device_generation, .reset_generation = value.reset_generation };
}
pub fn retained(id: u32) bool { return id != 0 and @atomicLoad(u32, &retained_owner, .acquire) == id; }
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
pub fn bootDescription(generation: u64, saved: *const display.BootSnapshot) abi.GfxNativeBootInfo {
    const state = display.backendState();
    const base = paging.physicalAddress(saved.mapping.virt_base) orelse 0;
    var physical = base;
    // One-time bounded verification of the saved contiguous physical extent.
    // It is not reconstructed from an HHDM offset or an arbitrary PCI BAR.
    if (saved.mapping.byte_len == 0 or saved.mapping.byte_len > 256 * 1024 * 1024) physical = 0;
    var offset: u64 = 0;
    while (physical != 0 and offset < saved.mapping.byte_len) : (offset += 4096) {
        if (paging.physicalAddress(saved.mapping.virt_base + offset) != base + offset) physical = 0;
    }
    return .{ .generation = generation, .physical_address = physical, .byte_length = saved.mapping.byte_len,
        .width = saved.mode.width, .height = saved.mode.height, .pitch = saved.mode.pitch,
        .format = if (framebuffer.isNativeXrgb32(&saved.framebuffer)) abi.gfx_buffer_format_xrgb8888 else 0,
        .policy = @intFromEnum(state.policy), .state = @intFromEnum(state.state) };
}
pub fn bootInfo(output: *abi.GfxNativeBootInfo) i32 {
    if (irq.inDispatch() or !buffer_api.validOutput(abi.GfxNativeBootInfo, output)) return abi.gfx_output_error_invalid;
    const saved = display.bootSnapshot() orelse return abi.gfx_output_error_unavailable;
    output.* = bootDescription(display.backendState().generation, &saved);
    return abi.gfx_output_ok;
}
fn stateResult(id: u32, outcome: u32) abi.GfxNativeState {
    const state = display.backendState();
    return .{ .generation = if (retained(id) and !bridge.ready) bridge.generation else if (state.pending_owner == id) state.pending_generation else state.generation,
        .state = @intFromEnum(state.state), .outcome = outcome,
        .retained = @intFromBool(display.retainsDriverOwner(id) or retained(id)) };
}
pub fn validAdapter(adapter: u32) bool {
    for (0..pci.count()) |index| {
        const item = pci.deviceAt(index) orelse return false;
        if (item.class_code == 3 and adapter == (0x0100_0000 | (@as(u32, item.bus) << 8) | (@as(u32, item.device) << 3) | item.function)) return true;
    }
    return false;
}
pub fn prepare(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, output: *abi.GfxNativeState) i32 {
    return prepareRequest(identity, input, 0, output);
}
pub fn prepareHeld(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, generation: u64, output: *abi.GfxNativeState) i32 {
    if (generation == 0) return abi.gfx_output_error_stale;
    return prepareRequest(identity, input, generation, output);
}
fn prepareRequest(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, held_generation: u64, output: *abi.GfxNativeState) i32 {
    if (@intFromPtr(input) == 0 or irq.inDispatch() or input.version != 1 or input.size < @sizeOf(abi.GfxNativeRegistration) or
        !buffer_api.validOutput(abi.GfxNativeState, output)) return abi.gfx_output_error_invalid;
    if (!execution.enter(0)) return abi.gfx_output_error_busy;
    defer _ = execution.leave();
    const result = prepareImpl(identity, input.*, held_generation) catch |err| {
        if (bridge.driver_owner.eql(identity) and !bridge.ready)
            output.* = stateResult(@intCast(identity.id), abi.gfx_output_outcome_lost);
        return code(err);
    };
    output.* = result;
    return abi.gfx_output_ok;
}
fn prepareImpl(identity: buffers.Owner, request: abi.GfxNativeRegistration, held_generation: u64) Error!abi.GfxNativeState {
    if (identity.kind != .driver or !identity.valid() or bridge.driver_owner.id != 0) return error.Busy;
    if (request.version != 1 or request.size < @sizeOf(abi.GfxNativeRegistration) or request.reference.reserved0 != 0 or
        request.commit_callback < 0xffff800000000000 or request.restore_callback < 0xffff800000000000 or
        !validAdapter(request.backend.adapter_id) or request.backend.milestone != @intFromEnum(queue.model.Milestone.device_execution)) return error.Invalid;
    const end = std.mem.indexOfScalar(u8, &request.name, 0) orelse return error.Invalid;
    if (end == 0) return error.Invalid;
    for (request.name[end..]) |byte| if (byte != 0) return error.Invalid;
    const saved = display.bootSnapshot() orelse return error.Unavailable;
    if (!framebuffer.isNativeXrgb32(&saved.framebuffer)) return error.Unsupported;
    try outputs.validateNative(@intCast(identity.id), request.backend, request.output, saved.mode.width, saved.mode.height);
    const caller_reference = try buffer_api.handle(request.reference);
    const prepared = blk: {
        buffers.lock(); defer buffers.unlock();
        const descriptor = try buffers.store.describe(caller_reference, identity);
        if (descriptor.format != .xrgb8888 or descriptor.location != .system or !descriptor.binding.portable() or
            descriptor.modifier != 0 or descriptor.plane_count != 1 or descriptor.planes[0].offset != 0 or
            descriptor.width != saved.mode.width or descriptor.height != saved.mode.height or
            descriptor.planes[0].pitch > std.math.maxInt(u32) or descriptor.planes[0].pitch != @as(u64, descriptor.width) * 4 or
            descriptor.bytes != descriptor.planes[0].pitch * descriptor.height or
            descriptor.usage & (buffers.layout.Usage.cpu_write | buffers.layout.Usage.transfer_source) !=
                (buffers.layout.Usage.cpu_write | buffers.layout.Usage.transfer_source)) return error.Unsupported;
        const reference = try buffers.store.share(caller_reference, owner);
        errdefer buffers.store.drop(reference, owner) catch unreachable;
        const mapped = try buffers.mapLocked(reference, owner, .cpu_write, 0, descriptor.bytes);
        try buffers.unmapCpuLocked(mapped.lease, owner);
        break :blk .{ .reference = reference, .address = mapped.backing.cpu_address, .descriptor = descriptor };
    };
    bridge = .{ .driver_owner = identity, .registration = request, .reference = prepared.reference,
        .bytes = prepared.descriptor.bytes, .frame = saved.framebuffer, .held_generation = held_generation };
    @atomicStore(u32, &retained_owner, @intCast(identity.id), .release);
    errdefer cancelPreparation();
    bridge.frame.address = @ptrFromInt(prepared.address);
    bridge.frame.pitch = prepared.descriptor.planes[0].pitch;
    bridge.frame.edid = null; bridge.frame.edid_size = 0;
    var mode = saved.mode;
    mode.pitch = @intCast(bridge.frame.pitch);
    const candidate = display.NativeBackend{ .owner = identity.id, .adapter_id = request.backend.adapter_id,
        .target = .{ .name = bridge.registration.name[0..end], .kind = .native, .flags = 255, .mode = mode,
            .mapping = .{ .kind = .native_scanout, .virt_base = prepared.address, .byte_len = bridge.bytes }, .framebuffer = &bridge.frame },
        .context = @intFromPtr(&bridge), .commit = commit, .restore = restore,
        .begin_cpu = beginCpu, .end_cpu = endCpu };
    bridge.generation = if (held_generation != 0) try display.prepareHeldNative(candidate, held_generation) else try display.prepareNative(candidate);
    bridge.timeline = try queue.open(owner, .{ .binding = binding(request.backend), .milestone = .device_execution, .capacity = 1 });
    try queue.bindDisplayQueue(@intCast(identity.id), binding(request.backend), bridge.timeline);
    bridge.ready = true;
    return stateResult(@intCast(identity.id), abi.gfx_output_outcome_validated);
}
fn cancelPreparation() void {
    if (bridge.generation != 0) display.abortNative(bridge.driver_owner.id, bridge.generation) catch return;
    bridge.cancelled = true;
    _ = discard();
}
pub fn transition(id: u32, generation: u64, operation: u32, output: *abi.GfxNativeState) i32 {
    if (id == 0 or irq.inDispatch() or !buffer_api.validOutput(abi.GfxNativeState, output) or operation > 2) return abi.gfx_output_error_invalid;
    if (!execution.enter(0)) return abi.gfx_output_error_busy;
    defer _ = execution.leave();
    // Native cleanup may already have released this bridge while a retained
    // boot snapshot still needs release. DisplayManager owns that final retry.
    if (bridge.driver_owner.id != id and !(operation == 2 and bridge.driver_owner.id == 0 and display.retainsDriverOwner(id))) return abi.gfx_output_error_stale;
    const outcome: u32 = switch (operation) {
        0 => blk: {
            if (!bridge.ready or bridge.cancelled) return abi.gfx_output_error_busy;
            const result = display.commitNative(id, generation) catch |err| return code(err);
            if (result == .old_preserved) {
                bridge.cancelled = true;
                bridge.ready = false;
                if (!discard()) break :blk abi.gfx_output_outcome_lost;
            }
            break :blk switch (result) { .confirmed => abi.gfx_output_outcome_applied, .old_preserved => abi.gfx_output_outcome_old_preserved, .output_lost => abi.gfx_output_outcome_lost };
        },
        1 => blk: {
            if (generation != bridge.generation) return abi.gfx_output_error_stale;
            if (!bridge.cancelled) display.abortNative(id, generation) catch |err| return code(err);
            bridge.cancelled = true;
            bridge.ready = false;
            break :blk if (discard()) abi.gfx_output_outcome_old_preserved else abi.gfx_output_outcome_lost;
        },
        2 => blk: {
            display.restoreBootBackend(id, generation) catch |err| {
                if (err != error.RestoreFailed) return code(err);
                break :blk abi.gfx_output_outcome_lost;
            };
            break :blk abi.gfx_output_outcome_applied;
        },
        else => unreachable,
    };
    output.* = stateResult(id, outcome);
    return abi.gfx_output_ok;
}
fn commit(_: usize, generation: u64, saved: *const display.BootSnapshot) display.CommitResult {
    if (!execution.enter(0)) return .old_preserved;
    defer _ = execution.leave();
    outputs.validateNative(@intCast(bridge.driver_owner.id), bridge.registration.backend, bridge.registration.output, saved.mode.width, saved.mode.height) catch return .old_preserved;
    if (!beginCpu(0)) return .old_preserved;
    // A firmware-started GPU may have repurposed the original VRAM mapping.
    // Only its pre-effects RAM capture is a valid source for a held handoff.
    const copied = if (bridge.held_generation != 0)
        boot_driver.copySnapshot(bridge.driver_owner, bridge.held_generation, saved, &bridge.frame, false)
    else blk: {
        for (0..saved.mode.height) |y| for (0..saved.mode.width) |x| {
            const source: *volatile u32 = @ptrCast(@alignCast(saved.framebuffer.address + y * saved.framebuffer.pitch + x * 4));
            const target: *u32 = @ptrFromInt(@intFromPtr(bridge.frame.address) + y * bridge.frame.pitch + x * 4);
            target.* = source.*;
        };
        break :blk true;
    };
    const released = endCpu(0, false, null);
    if (!copied or !released) return .old_preserved;
    if (!driver.enterOwnerBounded(@intCast(bridge.driver_owner.id), @max(timer.frequency(), 1))) return .old_preserved;
    defer _ = driver.leaveOwner();
    const callback: Callback = @ptrFromInt(bridge.registration.commit_callback);
    const boot = bootDescription(generation, saved);
    return switch (callback(bridge.registration.context, generation, &boot)) {
        1 => blk: {
            outputs.nativeActive(@intCast(bridge.driver_owner.id), bridge.registration.output, true);
            break :blk .confirmed;
        },
        2 => .old_preserved,
        else => .output_lost,
    };
}
fn restore(_: usize, generation: u64, saved: *const display.BootSnapshot) bool {
    if (!execution.enter(0)) return false;
    defer _ = execution.leave();
    if (!bridge.hardware_restored) {
        if (!driver.enterOwnerBounded(@intCast(bridge.driver_owner.id), @max(timer.frequency(), 1))) return false;
        const callback: Callback = @ptrFromInt(bridge.registration.restore_callback);
        const boot = bootDescription(generation, saved);
        const restored = callback(bridge.registration.context, generation, &boot) == 1;
        _ = driver.leaveOwner();
        if (!restored) return false;
        bridge.hardware_restored = true;
    }
    if (bridge.held_generation != 0 and !bridge.pixels_restored) {
        if (!boot_driver.copySnapshot(bridge.driver_owner, bridge.held_generation, saved, &saved.framebuffer, true)) return false;
        bridge.pixels_restored = true;
    }
    // The device callback has acknowledged whole-device quiescence. Marking
    // common queue jobs stopped is now backed by physical evidence.
    if (!bridge.queue_stopped) {
        outputs.nativeActive(@intCast(bridge.driver_owner.id), bridge.registration.output, false);
        queue.unregisterNative(@intCast(bridge.driver_owner.id), binding(bridge.registration.backend), true) catch |err| {
            // Busy follows publication of quiescence; the worker keeps its
            // own live record. Stale means this exact binding already retired.
            if (err != error.Busy and err != error.Stale) return false;
        };
        bridge.queue_stopped = true;
    }
    return discard();
}
fn beginCpu(_: usize) bool {
    if (!execution.enter(0)) return false;
    defer _ = execution.leave();
    if (bridge.cpu_lease.id != 0 or bridge.reference.id == 0 or bridge.pending != null) return false;
    buffers.lock(); defer buffers.unlock();
    const use = buffers.mapLocked(bridge.reference, owner, .cpu_write, 0, bridge.bytes) catch return false;
    if (use.backing.cpu_address != @intFromPtr(bridge.frame.address)) {
        buffers.unmapCpuLocked(use.lease, owner) catch {};
        return false;
    }
    bridge.cpu_lease = use.lease;
    return true;
}
fn endCpu(_: usize, changed: bool, damage: ?display.Rect) bool {
    if (!execution.enter(0)) return false;
    defer _ = execution.leave();
    if (bridge.cpu_lease.id == 0) return false;
    buffers.lock();
    buffers.unmapCpuLocked(bridge.cpu_lease, owner) catch { buffers.unlock(); return false; };
    bridge.cpu_lease = .{};
    buffers.unlock();
    if (!changed) return true;
    const rect = damage orelse display.Rect{ .w = @intCast(bridge.frame.width), .h = @intCast(bridge.frame.height) };
    if (rect.w == 0 or rect.h == 0 or rect.x >= bridge.frame.width or rect.y >= bridge.frame.height or
        rect.w > bridge.frame.width - rect.x or rect.h > bridge.frame.height - rect.y) return false;
    const offset = @as(u64, rect.y) * bridge.frame.pitch + @as(u64, rect.x) * 4;
    const span = @as(u64, rect.h - 1) * bridge.frame.pitch + @as(u64, rect.w) * 4;
    const now = monotonic.nowNanoseconds() orelse return false;
    const accepted = queue.submit(owner, bridge.timeline, .{ .deadline_ns = now +| 3_000_000_000 },
        .{ .operation = .upload, .source = bridge.reference, .source_offset = offset, .bytes = span }) catch return false;
    bridge.pending = accepted.fence;
    const complete = queue.wait(accepted.fence, @as(u64, @max(timer.frequency(), 1)) * 4, .resources_released) catch return false;
    queue.drop(owner, accepted.fence) catch return false;
    bridge.pending = null;
    return complete.result == .complete and !complete.device_active and !complete.resources_held;
}
fn discard() bool {
    if (bridge.cpu_lease.id != 0) {
        buffers.lock();
        buffers.unmapCpuLocked(bridge.cpu_lease, owner) catch { buffers.unlock(); return false; };
        bridge.cpu_lease = .{};
        buffers.unlock();
    }
    if (bridge.pending) |fence| {
        queue.drop(owner, fence) catch return false;
        bridge.pending = null;
    }
    if (bridge.timeline != 0) {
        queue.unbindDisplayQueue(@intCast(bridge.driver_owner.id), binding(bridge.registration.backend), bridge.timeline);
        queue.close(owner, bridge.timeline) catch |err| {
            // Device loss may already retire an empty timeline before this
            // bridge releases its CPU reference. Timeline IDs never wrap or
            // repeat: Stale for our retained ID means that exact queue is gone.
            // Every other failure, pending fence and CPU lease still vetoes.
            if (err != error.Stale) return false;
        };
        bridge.timeline = 0;
    }
    if (bridge.reference.id != 0) {
        buffers.drop(bridge.reference, owner) catch return false;
        bridge.reference = .{};
    }
    bridge = .{};
    @atomicStore(u32, &retained_owner, 0, .release);
    return true;
}
