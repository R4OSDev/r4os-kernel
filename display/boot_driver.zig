// Early R4D boot-display hold. GPU state/VRAM/register programming remains in
// the driver; the kernel owns writer exclusion and an immutable resident copy.
const abi = @import("r4os_kernel_contract");
const display = @import("display.zig");
const native = @import("native_driver.zig");
const ownership = @import("ownership.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const buffer_api = @import("../program/gfx_buffer_api.zig");
const driver = @import("../kernel/driver_api.zig");
const irq = @import("../kernel/irq_router.zig");
const timer = @import("../kernel/timer.zig");
const owner = @import("queue_resources.zig").display_owner;
const Callback = *const fn (u64, u64, *const abi.GfxNativeBootInfo) callconv(.c) i32;
var execution = ownership.Execution.init("boot-display-driver");
const Bridge = struct {
    identity: buffers.Owner,
    request: abi.GfxBootHoldRequest,
    reference: buffers.Handle = .{},
    lease: buffers.Handle = .{},
    address: u64 = 0,
    bytes: u64 = 0,
    captured: bool = false,
};
var bridge: ?Bridge = null;

pub fn hold(identity: buffers.Owner, input: *const abi.GfxBootHoldRequest, output: *abi.GfxNativeState) i32 {
    if (irq.inDispatch() or @intFromPtr(input) == 0 or input.version != 1 or input.size < @sizeOf(abi.GfxBootHoldRequest) or
        !buffer_api.validOutput(abi.GfxNativeState, output)) return abi.gfx_output_error_invalid;
    if (!execution.tryEnter()) return abi.gfx_output_error_busy;
    defer execution.leave();
    if (bridge != null) return abi.gfx_output_error_busy;
    const request = input.*;
    if (identity.kind != .driver or !identity.valid() or
        request.reserved0 != 0 or request.reference.reserved0 != 0 or request.generation == 0 or
        !native.validAdapter(request.adapter_id) or request.restore_callback < 0xffff800000000000) return abi.gfx_output_error_invalid;
    bridge = .{ .identity = identity, .request = request };
    const result = display.holdBoot(.{ .owner = identity.id, .adapter_id = request.adapter_id,
        .expected_generation = request.generation, .context = 0, .capture = capture, .restore = restore, .release = release }) catch |err| {
        // No capture has run on these admission errors.
        bridge = null;
        return native.code(err);
    };
    if (!result.retained) bridge = null;
    output.* = .{ .generation = result.generation, .state = @intFromEnum(display.backendState().state),
        .outcome = if (result.captured) abi.gfx_output_outcome_validated else if (result.retained) abi.gfx_output_outcome_lost else abi.gfx_output_outcome_old_preserved,
        .retained = @intFromBool(result.retained) };
    return abi.gfx_output_ok;
}

pub fn finish(identity: buffers.Owner, generation: u64, operation: u32, output: *abi.GfxNativeState) i32 {
    if (irq.inDispatch() or !buffer_api.validOutput(abi.GfxNativeState, output)) return abi.gfx_output_error_invalid;
    if (!execution.tryEnter()) return abi.gfx_output_error_busy;
    defer execution.leave();
    const current = &(bridge orelse return abi.gfx_output_error_stale);
    if (!current.identity.eql(identity)) return abi.gfx_output_error_stale;
    if (operation == 1 and !current.captured) return abi.gfx_output_error_invalid;
    const released = display.finishBoot(identity.id, generation, operation) catch |err| {
        if (err != error.RestoreFailed) return native.code(err);
        output.* = .{ .generation = generation, .state = @intFromEnum(display.backendState().state),
            .outcome = abi.gfx_output_outcome_lost, .retained = 1 };
        return abi.gfx_output_ok;
    };
    if (released) bridge = null;
    output.* = .{ .generation = generation, .state = @intFromEnum(display.backendState().state),
        .outcome = if (released) abi.gfx_output_outcome_old_preserved else abi.gfx_output_outcome_validated,
        .retained = @intFromBool(!released) };
    return abi.gfx_output_ok;
}

fn capture(_: usize, saved: *const display.BootSnapshot) bool {
    const current = if (bridge) |*value| value else return false;
    const bytes = @as(u64, saved.mode.pitch) * saved.mode.height;
    if (bytes == 0 or bytes > 256 * 1024 * 1024 or bytes > saved.mapping.byte_len or
        (bytes & 3) != 0 or (@intFromPtr(saved.framebuffer.address) & 3) != 0) return false;
    const caller = buffer_api.handle(current.request.reference) catch return false;
    buffers.lock();
    const descriptor = buffers.store.describe(caller, current.identity) catch { buffers.unlock(); return false; };
    if (descriptor.format != .bytes or descriptor.location != .system or !descriptor.binding.portable() or
        descriptor.bytes != bytes or descriptor.usage & (buffers.layout.Usage.cpu_read | buffers.layout.Usage.cpu_write) !=
            (buffers.layout.Usage.cpu_read | buffers.layout.Usage.cpu_write)) { buffers.unlock(); return false; }
    current.reference = buffers.store.share(caller, owner) catch { buffers.unlock(); return false; };
    const mapped = buffers.mapLocked(current.reference, owner, .cpu_write, 0, bytes) catch { buffers.unlock(); return false; };
    current.lease = mapped.lease;
    current.bytes = bytes;
    current.address = mapped.backing.cpu_address;
    buffers.unlock();
    // Full rows include pitch padding. All firmware/normal CPU writers are
    // excluded by DisplayManager; no hardware callback has yet been admitted.
    const source: [*]volatile const u32 = @ptrCast(@alignCast(saved.framebuffer.address));
    const target: [*]u32 = @ptrFromInt(current.address);
    for (0..bytes / 4) |index| target[index] = source[index];
    buffers.lock();
    defer buffers.unlock();
    buffers.unmapCpuLocked(current.lease, owner) catch return false;
    current.lease = .{};
    // Retaining a read lease excludes every BO writer/device write, including
    // writes through an imported reference, throughout the hardware hold.
    const read = buffers.mapLocked(current.reference, owner, .cpu_read, 0, bytes) catch return false;
    current.lease = read.lease;
    if (read.backing.cpu_address != current.address) return false;
    current.captured = true;
    return true;
}

fn restore(_: usize, generation: u64, saved: *const display.BootSnapshot) bool {
    const current = &(bridge orelse return false);
    if (!current.captured or current.lease.id == 0) return false;
    if (!driver.enterOwnerBounded(@intCast(current.identity.id), @max(timer.frequency(), 1))) return false;
    const callback: Callback = @ptrFromInt(current.request.restore_callback);
    const boot = native.bootDescription(generation, saved);
    const restored = callback(current.request.context, generation, &boot) == 1;
    _ = driver.leaveOwner();
    if (!restored) return false;
    // Only the real driver can establish that this original mapping again
    // targets the original scanout and that no GPU DMA can race its contents.
    const source: [*]const u32 = @ptrFromInt(current.address);
    const target: [*]volatile u32 = @ptrCast(@alignCast(saved.framebuffer.address));
    for (0..current.bytes / 4) |index| target[index] = source[index];
    return true;
}

fn release(_: usize) bool {
    const current = if (bridge) |*value| value else return true;
    buffers.lock();
    if (current.lease.id != 0) {
        buffers.unmapCpuLocked(current.lease, owner) catch { buffers.unlock(); return false; };
        current.lease = .{};
    }
    buffers.unlock();
    if (current.reference.id != 0) {
        buffers.drop(current.reference, owner) catch return false;
        current.reference = .{};
    }
    return true;
}
