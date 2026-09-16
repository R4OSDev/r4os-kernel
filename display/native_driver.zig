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
pub const Error = outputs.Error || display.TransitionError || @import("native_additional_mode.zig").Error;
const Callback = *const fn (u64, u64, *const abi.GfxNativeBootInfo) callconv(.c) i32;
const Bridge = struct {
    driver_owner: buffers.Owner = .{ .kind = .driver, .id = 0, .generation = 0 },
    registration: abi.GfxNativeRegistration = .{},
    reference: buffers.Handle = .{},
    cpu_lease: buffers.Handle = .{},
    timeline: u64 = 0,
    pending: ?queue.model.Fence = null,
    image_pending: ?queue.model.Fence = null,
    image_direct: bool = false,
    last_present: ?queue.model.Fence = null,
    bytes: u64 = 0,
    frame: framebuffer.Framebuffer = undefined,
    output_format: u32 = abi.gfx_buffer_format_xrgb8888,
    encoded_output: bool = false,
    held_generation: u64 = 0,
    generation: u64 = 0,
    ready: bool = false,
    cancelled: bool = false,
    hardware_restored: bool = false,
    pixels_restored: bool = false,
    queue_stopped: bool = false,
    modes_enabled: bool = false,
    driver_reference: buffers.Handle = .{},
};
var bridge: Bridge = .{};
const Surface = struct {
    reference: buffers.Handle = .{},
    driver_reference: buffers.Handle = .{},
    read_lease: buffers.Handle = .{},
    bytes: u64 = 0,
    frame: framebuffer.Framebuffer = undefined,
    output_format: u32 = abi.gfx_buffer_format_xrgb8888,
    encoded_output: bool = false,
};
const Replacement = struct { old: Surface, new: Surface, using_new: bool = false };
var replacement: ?Replacement = null;
// Different R4D owners can call this global bridge concurrently. Callbacks
// also enter from normal display presentation, outside a driver transition.
// Reentrance is required for a transition's own commit/CPU/restore callbacks;
// competing tasks get Busy. This guard permits waits and never holds an I/O
// owner across the queue completion or the actual driver callback.
var execution = @import("../sched/sync.zig").UnwindGuard.init("native-display-driver");
var retained_owner: u32 = 0;
const ResetRecord = struct {
    driver: buffers.Owner,
    backend: abi.GfxBackendBinding,
    original_generation: u64,
    generation: u64,
    retired: bool = false,
};
var reset_record: ?ResetRecord = null;

fn binding(value: abi.GfxBackendBinding) queue.model.Binding {
    return .{ .adapter = value.adapter_id, .device_generation = value.device_generation, .reset_generation = value.reset_generation };
}
pub fn retained(id: u32) bool { return id != 0 and @atomicLoad(u32, &retained_owner, .acquire) == id; }
// Read only while DisplayExecution is held; replacement admission, geometry
// mutation and retirement all use that same execution owner.
pub fn modePending() bool { return replacement != null; }
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
    return prepareRequest(identity, input, 0, 0, output);
}
pub fn prepareHeld(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, generation: u64, output: *abi.GfxNativeState) i32 {
    if (generation == 0) return abi.gfx_output_error_stale;
    return prepareRequest(identity, input, generation, 0, output);
}
pub fn prepareReset(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, held_generation: u64, generation: u64, output: *abi.GfxNativeState) i32 {
    if (generation == 0) return abi.gfx_output_error_stale;
    return prepareRequest(identity, input, held_generation, generation, output);
}
fn prepareRequest(identity: buffers.Owner, input: *const abi.GfxNativeRegistration, held_generation: u64, reset_generation: u64, output: *abi.GfxNativeState) i32 {
    if (@intFromPtr(input) == 0 or irq.inDispatch() or input.version != 1 or input.size < @sizeOf(abi.GfxNativeRegistration) or
        !buffer_api.validOutput(abi.GfxNativeState, output)) return abi.gfx_output_error_invalid;
    if (!execution.enter(0)) return abi.gfx_output_error_busy;
    defer _ = execution.leave();
    if (reset_generation != 0) {
        const reset = reset_record orelse return abi.gfx_output_error_stale;
        if (!reset.driver.eql(identity) or reset.generation != reset_generation or
            input.backend.adapter_id != reset.backend.adapter_id or input.backend.device_generation <= reset.backend.device_generation)
            return abi.gfx_output_error_stale;
        if (!reset.retired) return abi.gfx_output_error_busy;
    } else if (reset_record != null) return abi.gfx_output_error_busy;
    const result = prepareImpl(identity, input.*, held_generation, reset_generation) catch |err| {
        if (bridge.driver_owner.eql(identity) and !bridge.ready)
            output.* = stateResult(@intCast(identity.id), abi.gfx_output_outcome_lost);
        return code(err);
    };
    if (reset_generation != 0) reset_record = null;
    output.* = result;
    return abi.gfx_output_ok;
}
fn prepareImpl(identity: buffers.Owner, request: abi.GfxNativeRegistration, held_generation: u64, reset_generation: u64) Error!abi.GfxNativeState {
    if (identity.kind != .driver or !identity.valid() or bridge.driver_owner.id != 0) return error.Busy;
    if (request.version != 1 or request.size < @sizeOf(abi.GfxNativeRegistration) or request.reference.reserved0 != 0 or
        request.commit_callback < 0xffff800000000000 or request.restore_callback < 0xffff800000000000 or
        !validAdapter(request.backend.adapter_id) or request.backend.milestone != @intFromEnum(queue.model.Milestone.device_execution)) return error.Invalid;
    const end = std.mem.indexOfScalar(u8, &request.name, 0) orelse return error.Invalid;
    if (end == 0) return error.Invalid;
    for (request.name[end..]) |byte| if (byte != 0) return error.Invalid;
    const saved = display.bootSnapshot() orelse return error.Unavailable;
    if (!framebuffer.isNativeXrgb32(&saved.framebuffer)) return error.Unsupported;
    try outputs.validateNative(@intCast(identity.id), request.backend, request.output, saved.mode.width, saved.mode.height, held_generation != 0);
    const caller_reference = try buffer_api.handle(request.reference);
    const prepared = blk: {
        try buffers.lockPrepared(.{ .references = 1, .leases = 1 }); defer buffers.unlock();
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
    bridge.generation = if (reset_generation != 0) try display.prepareRecoveredNative(candidate, held_generation, reset_generation)
        else if (held_generation != 0) try display.prepareHeldNative(candidate, held_generation) else try display.prepareNative(candidate);
    bridge.timeline = try queue.open(owner, .{ .binding = binding(request.backend), .milestone = .device_execution, .capacity = 1 });
    try queue.bindDisplayQueue(@intCast(identity.id), binding(request.backend), bridge.timeline);
    try display.bindPresentationStats(identity.id, identity.generation, request.backend, bridge.generation);
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
    if (replacement != null) return abi.gfx_output_error_busy;
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
            if (reset_record) |reset| if (!reset.retired) return abi.gfx_output_error_busy;
            display.restoreBootBackend(id, generation) catch |err| {
                if (err != error.RestoreFailed) return code(err);
                break :blk abi.gfx_output_outcome_lost;
            };
            reset_record = null;
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
    outputs.validateNative(@intCast(bridge.driver_owner.id), bridge.registration.backend, bridge.registration.output, saved.mode.width, saved.mode.height, bridge.held_generation != 0) catch return .old_preserved;
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
    if (replacement != null) return false;
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
    if (bridge.encoded_output or bridge.cpu_lease.id != 0 or bridge.reference.id == 0 or bridge.pending != null or !imageIdle()) return false;
    buffers.lockPrepared(.{ .leases = 1 }) catch return false;
    defer buffers.unlock();
    const use = buffers.mapLocked(bridge.reference, owner, .cpu_write, 0, bridge.bytes) catch return false;
    if (use.backing.cpu_address != @intFromPtr(bridge.frame.address)) {
        buffers.unmapCpuLocked(use.lease, owner) catch {};
        return false;
    }
    bridge.cpu_lease = use.lease;
    return true;
}
// Called only under the bridge's unwind guard. The receipt belongs to the
// producer queue; observing retirement never drops that producer reference.
fn imageIdle() bool {
    const fence = bridge.image_pending orelse return true;
    const status = queue.query(fence) catch |err| {
        // Fence storage is collected only after device activity and retained
        // resources are gone. Stale here releases admission, not a success
        // receipt and not a claim that this image was ever displayed.
        if (err != error.Stale) return false;
        bridge.image_pending = null;
        return true;
    };
    if (status.phase != .terminal or (!bridge.image_direct and (status.device_active or status.resources_held))) return false;
    if (status.result == .complete) bridge.last_present = fence;
    bridge.image_pending = null;
    return true;
}
pub fn outputTarget(adapter: u32, head: u32) Error!abi.GfxOutputTarget {
    if (try @import("output_runtime.zig").targetAt(adapter, head)) |target| return target;
    if (!display.beginOutputCommit()) return error.Busy;
    defer display.endOutputCommit();
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    const state = display.backendState();
    const target = if (adapter == 0)
        try outputs.activeTarget(0, .{ .connector_id = 1, .device_generation = 1 }, state.generation)
    else blk: {
        if (!bridge.ready or bridge.cancelled or bridge.hardware_restored or state.state != .software_native) return error.Unsupported;
        try queue.validateOutputBinding(@intCast(bridge.driver_owner.id), bridge.registration.backend);
        break :blk try outputs.activeTarget(@intCast(bridge.driver_owner.id), bridge.registration.output, bridge.generation);
    };
    if (target.adapter_id != adapter or target.head_id != head) return error.Unsupported;
    return target;
}
pub fn submitImage(caller: buffers.Owner, timeline: u64, submission: queue.model.Submission,
    request: @import("queue_resources.zig").Request) queue.Error!queue.model.Status
{
    const additional = @import("output_runtime.zig");
    if (additional.contains(request.display_target)) return additional.submitImage(caller, timeline, submission, request);
    if (!display.beginOutputCommit()) return error.Busy;
    defer display.endOutputCommit();
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    if (!bridge.ready or bridge.cancelled or bridge.hardware_restored or
        display.backendState().state != .software_native) return error.Unsupported;
    if (replacement != null or bridge.cpu_lease.id != 0 or bridge.pending != null or !imageIdle()) return error.Busy;
    if (outputs.nativePaused(@intCast(bridge.driver_owner.id), bridge.registration.output)) return error.Unavailable;
    if (request.display_target.connector_id != 0) {
        const current = try outputs.activeTarget(@intCast(bridge.driver_owner.id), bridge.registration.output, bridge.generation);
        if (!@import("output_target.zig").same(current, request.display_target)) return error.Stale;
    }
    try queue.validateOutputBinding(@intCast(bridge.driver_owner.id), bridge.registration.backend);
    const descriptor = blk: {
        buffers.lock(); defer buffers.unlock();
        break :blk try buffers.store.describe(request.source, caller);
    };
    if ((request.operation != .present and request.operation != .direct_present) or descriptor.width != bridge.frame.width or descriptor.height != bridge.frame.height or
        @intFromEnum(descriptor.format) != bridge.output_format or (descriptor.location != .device_local and descriptor.location != .system) or
        descriptor.plane_count != 1 or descriptor.planes[0].offset != 0)
        return error.Unsupported;
    const accepted = try queue.submitDisplayImage(caller, timeline, submission, request, binding(bridge.registration.backend));
    bridge.image_pending = accepted.fence;
    bridge.image_direct = request.operation == .direct_present;
    return accepted;
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
    if (outputs.nativePaused(@intCast(bridge.driver_owner.id), bridge.registration.output)) return true;
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
    const succeeded = complete.result == .complete and !complete.device_active and !complete.resources_held;
    if (succeeded) bridge.last_present = accepted.fence;
    // A driver may drain a queued upload when its connector disappears.
    // CPU painting is still valid; no visible/Present fence is invented and
    // the driver must refresh the retained image before reconnecting it.
    return succeeded or (complete.result == .cancelled and !complete.device_active and !complete.resources_held);
}
fn discard() bool {
    if (replacement != null or !imageIdle()) return false;
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
    if (bridge.driver_reference.id != 0) {
        buffers.drop(bridge.driver_reference, bridge.driver_owner) catch return false;
        bridge.driver_reference = .{};
    }
    bridge = .{};
    @atomicStore(u32, &retained_owner, 0, .release);
    return true;
}

pub const ModeBinding = struct { driver: buffers.Owner, backend: abi.GfxBackendBinding, generation: u64,
    additional: ?@import("native_additional_mode.zig").Binding = null };
pub const CursorBinding = struct { driver: buffers.Owner, backend: abi.GfxBackendBinding, generation: u64,
    head: u32, timeline: u64 = 0, point: u64 = 0 };
// Caller holds DisplayExecution. Taking this snapshot never waits for a GPU
// or reuses the CPU-copy result as a visibility receipt.
pub fn cursorBinding() Error!CursorBinding {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    if (!bridge.ready or bridge.cancelled or bridge.hardware_restored or
        display.backendState().state != .software_native) return error.Unsupported;
    if (replacement != null or bridge.cpu_lease.id != 0 or bridge.pending != null or !imageIdle()) return error.Busy;
    try queue.validateOutputBinding(@intCast(bridge.driver_owner.id), bridge.registration.backend);
    return .{ .driver = bridge.driver_owner, .backend = bridge.registration.backend, .generation = bridge.generation,
        .head = try outputs.cursorHead(@intCast(bridge.driver_owner.id), bridge.registration.output),
        .timeline = if (bridge.last_present) |fence| fence.timeline else 0,
        .point = if (bridge.last_present) |fence| fence.point else 0 };
}
pub fn enableModes(identity: buffers.Owner, backend: abi.GfxBackendBinding) Error!void {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    if (!bridge.driver_owner.eql(identity) or !bridge.ready or !std.meta.eql(bridge.registration.backend, backend) or
        display.backendState().state != .software_native or bridge.cancelled or bridge.hardware_restored) return error.Stale;
    try queue.validateOutputBinding(@intCast(identity.id), backend);
    bridge.modes_enabled = true;
}
pub fn modesEnabled(id: u32, backend: abi.GfxBackendBinding) bool {
    if (!execution.enter(0)) return false;
    defer _ = execution.leave();
    return bridge.ready and bridge.modes_enabled and bridge.driver_owner.id == id and
        std.meta.eql(bridge.registration.backend, backend) and !bridge.cancelled and !bridge.hardware_restored;
}
pub fn modeBinding(assignment: abi.GfxScanoutState) Error!ModeBinding {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    if (!bridge.ready or !bridge.modes_enabled or bridge.cancelled or bridge.hardware_restored or
        display.backendState().state != .software_native) return error.Unsupported;
    if (assignment.output.adapter_id != bridge.registration.backend.adapter_id or
        assignment.output.device_generation != bridge.registration.backend.device_generation) return error.Stale;
    try queue.validateOutputBinding(@intCast(bridge.driver_owner.id), bridge.registration.backend);
    if (assignment.output.connector_id != bridge.registration.output.connector_id) {
        const additional = try @import("output_runtime.zig").modeBinding(assignment);
        if (!additional.driver.eql(bridge.driver_owner) or !std.meta.eql(additional.backend, bridge.registration.backend)) return error.Stale;
        return .{ .driver = additional.driver, .backend = additional.backend,
            .generation = additional.target.display_generation, .additional = additional };
    }
    return .{ .driver = bridge.driver_owner, .backend = bridge.registration.backend, .generation = bridge.generation };
}
// DisplayExecution is held by the submitting task. Full references are
// imported before caller exit can close the producer; queued mapping-only
// loans cannot support subsequent normal desktop CPU writes.
pub fn prepareMode(caller: buffers.Owner, assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode) Error!abi.GfxBufferReference {
    return prepareModeEncoding(caller, assignment, mode, null);
}
pub fn prepareModeEncoding(caller: buffers.Owner, assignment: abi.GfxScanoutState, mode: abi.GfxOutputMode, encoded_format: ?u32) Error!abi.GfxBufferReference {
    if (encoded_format) |format| if (format != abi.gfx_buffer_format_xrgb8888 and format != abi.gfx_buffer_format_xrgb2101010) return error.Unsupported;
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    const selected = try modeBinding(assignment);
    if (selected.additional) |additional| return @import("native_additional_mode.zig").prepare(caller, assignment, mode, additional);
    if (replacement != null or bridge.cpu_lease.id != 0 or bridge.pending != null or !imageIdle()) return error.Busy;
    if (assignment.source_x != 0 or assignment.source_y != 0 or assignment.destination_x != 0 or assignment.destination_y != 0 or
        assignment.source_width != mode.width or assignment.source_height != mode.height or
        assignment.destination_width != mode.width or assignment.destination_height != mode.height or
        assignment.rotation != 0 or assignment.color != 0 or assignment.bits_per_color != 8) return error.Unsupported;
    const source = try buffer_api.handle(assignment.buffer);
    try buffers.lockPrepared(.{ .references = 2, .leases = 1 }); defer buffers.unlock();
    const descriptor = try buffers.store.describe(source, caller);
    const usage = buffers.layout.Usage.cpu_write | buffers.layout.Usage.transfer_source | buffers.layout.Usage.scanout;
    if (descriptor.format != .xrgb8888 or descriptor.location != .system or !descriptor.binding.portable() or
        descriptor.modifier != 0 or descriptor.plane_count != 1 or descriptor.planes[0].offset != 0 or descriptor.usage & usage != usage or
        descriptor.width != mode.width or descriptor.height != mode.height or descriptor.planes[0].pitch > std.math.maxInt(u32) or
        descriptor.bytes != descriptor.planes[0].pitch * descriptor.height) return error.Unsupported;
    if ((try buffers.store.bufferFor(source, caller)).eql(try buffers.store.bufferFor(bridge.reference, owner))) return error.Invalid;
    const reference = try buffers.store.share(source, owner);
    errdefer buffers.store.drop(reference, owner) catch unreachable;
    const driver_reference = try buffers.store.share(source, bridge.driver_owner);
    errdefer buffers.store.drop(driver_reference, bridge.driver_owner) catch unreachable;
    const mapped = try buffers.mapLocked(reference, owner, .cpu_write, 0, descriptor.bytes);
    try buffers.unmapCpuLocked(mapped.lease, owner);
    const read = try buffers.store.use(reference, owner, .device_read, 0, descriptor.bytes);
    errdefer buffers.store.endUse(read.lease, owner, true) catch unreachable;
    const wire = try buffer_api.referenceLocked(driver_reference, bridge.driver_owner);
    var frame = bridge.frame;
    frame.address = @ptrFromInt(mapped.backing.cpu_address);
    frame.width = descriptor.width; frame.height = descriptor.height; frame.pitch = descriptor.planes[0].pitch;
    frame.edid = null; frame.edid_size = 0;
    replacement = .{ .old = .{ .reference = bridge.reference, .driver_reference = bridge.driver_reference, .bytes = bridge.bytes, .frame = bridge.frame,
            .output_format = bridge.output_format, .encoded_output = bridge.encoded_output },
        .new = .{ .reference = reference, .driver_reference = driver_reference, .read_lease = read.lease, .bytes = descriptor.bytes, .frame = frame,
            .output_format = encoded_format orelse abi.gfx_buffer_format_xrgb8888, .encoded_output = encoded_format != null } };
    return wire;
}
// The mode worker holds DisplayExecution before exposing a job to the R4D.
pub fn startMode(operation: u32, expected: ModeBinding) Error!void {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    if (expected.additional) |additional| return @import("native_additional_mode.zig").start(operation, additional);
    if (!bridge.driver_owner.eql(expected.driver) or bridge.generation != expected.generation or
        !std.meta.eql(bridge.registration.backend, expected.backend) or bridge.cancelled or bridge.hardware_restored) return error.Stale;
    const change = if (replacement) |*value| value else return error.Stale;
    if (bridge.cpu_lease.id != 0 or bridge.pending != null or !imageIdle()) return error.Busy;
    try queue.validateOutputBinding(@intCast(expected.driver.id), expected.backend);
    const surface = if (operation == abi.gfx_mode_operation_apply) &change.old else if (operation == abi.gfx_mode_operation_rollback) &change.new else return;
    if (surface.read_lease.id == 0) {
        try buffers.lockPrepared(.{ .leases = 1 }); defer buffers.unlock();
        surface.read_lease = (try buffers.store.use(surface.reference, owner, .device_read, 0, surface.bytes)).lease;
    }
}
fn releaseRead(surface: *Surface) Error!void {
    if (surface.read_lease.id == 0) return;
    buffers.lock(); defer buffers.unlock();
    try buffers.store.endUse(surface.read_lease, owner, true);
    surface.read_lease = .{};
}
fn releaseSurface(surface: *Surface) Error!void {
    try releaseRead(surface);
    if (surface.driver_reference.id != 0) {
        try buffers.drop(surface.driver_reference, bridge.driver_owner);
        surface.driver_reference = .{};
    }
    if (surface.reference.id != 0) {
        try buffers.drop(surface.reference, owner);
        surface.reference = .{};
    }
}
fn selectSurface(surface: *const Surface) Error!void {
    const before = bridge.frame;
    bridge.frame = surface.frame;
    display.replaceNativeFrameLocked(bridge.driver_owner.id, bridge.generation, &bridge.frame, surface.bytes) catch |err| {
        bridge.frame = before;
        return err;
    };
    bridge.reference = surface.reference; bridge.driver_reference = surface.driver_reference; bridge.bytes = surface.bytes;
    bridge.output_format = surface.output_format; bridge.encoded_output = surface.encoded_output;
    outputs.nativeActive(@intCast(bridge.driver_owner.id), bridge.registration.output, true);
}
pub fn settleMode(operation: u32, outcome: u32) Error!void {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    const change = if (replacement) |*value| value else return error.Stale;
    if (outcome == abi.gfx_output_outcome_lost) {
        try display.loseNativeModeLocked(bridge.driver_owner.id, bridge.generation);
        outputs.nativeActive(@intCast(bridge.driver_owner.id), bridge.registration.output, false);
        return;
    }
    if (outcome == abi.gfx_output_outcome_applied) {
        if (operation == abi.gfx_mode_operation_apply) {
            try releaseRead(&change.new);
            try selectSurface(&change.new);
            change.using_new = true;
            return;
        }
        if (operation != abi.gfx_mode_operation_confirm or !change.using_new) return error.Invalid;
        // A late receipt may follow an operation timeout that hid the
        // primary. Republish the proven image before retiring old backing.
        try selectSurface(&change.new);
        try releaseSurface(&change.old);
    } else if (outcome == abi.gfx_output_outcome_old_preserved) {
        try releaseRead(&change.old);
        // Even a rejected apply may arrive after the primary was hidden.
        // using_new alone therefore cannot decide whether recovery is needed.
        try selectSurface(&change.old);
        try releaseSurface(&change.new);
    } else return error.Invalid;
    replacement = null;
}
pub fn settleModeFor(operation: u32, outcome: u32, expected: ModeBinding) Error!void {
    if (expected.additional != null) return @import("native_additional_mode.zig").settle(operation, outcome);
    return settleMode(operation, outcome);
}
pub fn abortPreparedMode(expected: ModeBinding) void {
    settleModeFor(abi.gfx_mode_operation_apply, abi.gfx_output_outcome_old_preserved, expected) catch {};
}

// DisplayExecution and common mode admission exclude writers and receipts.
// Only the reset bridge may call this after proven quiescence of backend.
pub fn retireModeAfterReset(identity: buffers.Owner, backend: abi.GfxBackendBinding) Error!void {
    if (!execution.enter(0)) return error.Busy;
    defer _ = execution.leave();
    if (bridge.driver_owner.id != 0) {
        if (!bridge.driver_owner.eql(identity) or !std.meta.eql(bridge.registration.backend, backend)) return error.Stale;
        if (bridge.cpu_lease.id != 0) {
            buffers.lock(); defer buffers.unlock();
            try buffers.unmapCpuLocked(bridge.cpu_lease, owner);
            bridge.cpu_lease = .{};
        }
        if (bridge.pending) |fence| {
            try requireQuiescedFence(fence);
            queue.drop(owner, fence) catch |err| { if (err != error.Stale) return err; };
            bridge.pending = null;
        }
        if (bridge.image_pending) |fence| {
            try requireQuiescedFence(fence);
            bridge.image_pending = null;
        }
        bridge.modes_enabled = false;
        bridge.cancelled = true;
        if (replacement) |*change| {
            // The bridge aliases the selected surface; the replacement owns
            // both references until retirement has completed, including retry.
            bridge.reference = .{};
            bridge.driver_reference = .{};
            try releaseSurface(&change.old);
            try releaseSurface(&change.new);
            replacement = null;
        }
    }
    try @import("native_additional_mode.zig").retireAfterReset(identity, backend);
}

fn requireQuiescedFence(fence: queue.model.Fence) Error!void {
    const status = queue.query(fence) catch |err| {
        // Collection can retire only a terminal, physically released fence.
        if (err == error.Stale) return;
        return err;
    };
    if (status.phase != .terminal or status.device_active or status.resources_held) return error.Busy;
}

pub fn deviceReset(identity: buffers.Owner, input: *const abi.GfxBackendBinding, generation: u64, quiesced: u32, output: *abi.GfxNativeState) i32 {
    if (@intFromPtr(input) == 0 or irq.inDispatch() or !buffer_api.validOutput(abi.GfxNativeState, output) or quiesced > 1 or
        input.version != 1 or input.size < @sizeOf(abi.GfxBackendBinding) or generation == 0 or
        !identity.valid() or identity.kind != .driver or !validAdapter(input.adapter_id) or
        input.milestone != abi.gfx_queue_milestone_device_execution or (input.device_generation == 0) != (input.reset_generation == 0))
        return abi.gfx_output_error_invalid;
    if (!execution.enter(0)) return abi.gfx_output_error_busy;
    defer _ = execution.leave();
    if (reset_record == null) {
        if (quiesced != 0) return abi.gfx_output_error_stale;
        const current = display.backendState();
        if (current.state == .preparing) {
            if (current.pending_owner != identity.id or current.pending_generation != generation or current.pending_adapter_id != input.adapter_id)
                return abi.gfx_output_error_stale;
        } else if (current.owner != identity.id or current.generation != generation or current.adapter_id != input.adapter_id)
            return abi.gfx_output_error_stale;
        if (bridge.driver_owner.id != 0 and (!bridge.driver_owner.eql(identity) or
            !std.meta.eql(bridge.registration.backend, input.*) or (bridge.generation != generation and
                !(current.state == .unavailable and current.reason == .restore_failed and bridge.generation < generation)))) return abi.gfx_output_error_stale;
        @import("mode_work.zig").stoppedDriver(@intCast(identity.id));
        @import("cursor_work.zig").stoppedDriver(@intCast(identity.id));
        @import("output_runtime.zig").stoppedDriver(@intCast(identity.id));
        const next = display.beginDeviceReset(identity.id, generation, input.adapter_id) catch |err| return code(err);
        bridge.cancelled = true;
        reset_record = .{ .driver = identity, .backend = input.*, .original_generation = generation, .generation = next };
    }
    const reset = &reset_record.?;
    if (!reset.driver.eql(identity) or !std.meta.eql(reset.backend, input.*) or
        generation != (if (quiesced == 0) reset.original_generation else reset.generation)) return abi.gfx_output_error_stale;
    if (quiesced == 1 and !reset.retired) {
        if (input.device_generation != 0) queue.unregisterNative(@intCast(identity.id), binding(input.*), true) catch |err| {
            if (err != error.Stale) return code(err);
        };
        @import("mode_work.zig").retireAfterReset(identity, input.*) catch |err| return code(err);
        @import("cursor_work.zig").retireAfterReset(identity, input.*) catch |err| return @import("cursor_work.zig").code(err);
        @import("output_runtime.zig").retireAfterReset(identity, input.*) catch |err| return code(err);
        display.retireDeviceReset(identity.id, reset.generation) catch |err| return code(err);
        if (!discard()) return abi.gfx_output_error_busy;
        reset.retired = true;
    }
    output.* = .{ .generation = reset.generation, .state = @intFromEnum(display.backendState().state),
        .outcome = abi.gfx_output_outcome_lost, .retained = 1 };
    return abi.gfx_output_ok;
}
