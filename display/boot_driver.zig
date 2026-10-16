// Temporary private projections to the single native early-display bridge.
const abi = @import("r4os_kernel_contract");
const display = @import("display.zig");
const framebuffer = @import("framebuffer.zig");
const native = @import("native_driver.zig");
const buffers = @import("../memory/gfx_buffers.zig");
const owner = @import("queue_resources.zig").display_owner;
const Identity = extern struct { kind: u32, id: u64, generation: u64 };
const Handle = extern struct { id: u32 = 0, generation: u64 = 0 };
const Snapshot = extern struct { frame: framebuffer.Framebuffer, mapping_bytes: u64, width: u32, height: u32, pitch: u32 };
const Description = extern struct { bytes: u64, usage: u32, is_bytes: bool, is_system: bool, portable: bool };
const Mapping = extern struct { lease: Handle, address: u64 };
const HoldResult = extern struct { generation: u64, captured: bool, retained: bool };
fn identity(value: buffers.Owner) Identity {
    return .{ .kind = @intFromEnum(value.kind), .id = value.id, .generation = value.generation };
}
fn actual(value: *const Identity) buffers.Owner {
    return .{ .kind = @enumFromInt(value.kind), .id = value.id, .generation = value.generation };
}
fn handle(value: *const Handle) buffers.Handle {
    return .{ .id = value.id, .generation = value.generation };
}
fn wire(value: buffers.Handle) Handle {
    return .{ .id = value.id, .generation = value.generation };
}
fn snapshot(raw: *const anyopaque) *const display.BootSnapshot {
    return @ptrCast(@alignCast(raw));
}
extern fn r4os_rust_boot_driver_hold(*const Identity, *const abi.GfxBootHoldRequest, *abi.GfxNativeState) i32;
extern fn r4os_rust_boot_driver_finish(*const Identity, u64, u32, *abi.GfxNativeState) i32;
extern fn r4os_rust_boot_driver_prepare(*const Identity, *const abi.GfxNativeRegistration, u64, u64, bool, *abi.GfxNativeState) i32;
extern fn r4os_rust_boot_driver_copy(*const Identity, u64, *const anyopaque, *const framebuffer.Framebuffer, bool) bool;
extern fn r4os_rust_boot_driver_capture(*const anyopaque) bool;
extern fn r4os_rust_boot_driver_restore(u64, *const anyopaque, bool) bool;
extern fn r4os_rust_boot_driver_release(bool) bool;
pub fn hold(value: buffers.Owner, input: *const abi.GfxBootHoldRequest, output: *abi.GfxNativeState) i32 {
    const id = identity(value);
    return r4os_rust_boot_driver_hold(&id, input, output);
}
pub fn finish(value: buffers.Owner, generation: u64, operation: u32, output: *abi.GfxNativeState) i32 {
    const id = identity(value);
    return r4os_rust_boot_driver_finish(&id, generation, operation, output);
}
pub fn prepareHeld(value: buffers.Owner, input: *const abi.GfxNativeRegistration, generation: u64, output: *abi.GfxNativeState) i32 {
    const id = identity(value);
    return r4os_rust_boot_driver_prepare(&id, input, generation, 0, false, output);
}
pub fn prepareReset(value: buffers.Owner, input: *const abi.GfxNativeRegistration, generation: u64, reset: u64, output: *abi.GfxNativeState) i32 {
    const id = identity(value);
    return r4os_rust_boot_driver_prepare(&id, input, generation, reset, true, output);
}
pub fn copySnapshot(value: buffers.Owner, generation: u64, saved: *const display.BootSnapshot, target: *const framebuffer.Framebuffer, include_padding: bool) bool {
    const id = identity(value);
    return r4os_rust_boot_driver_copy(&id, generation, saved, target, include_padding);
}
// Ordinary Zig callback signatures stay at the display owner. C callbacks are
// invoked only through these typed thunks, never by casting function ABIs.
fn capture(_: usize, saved: *const display.BootSnapshot) bool {
    return r4os_rust_boot_driver_capture(saved);
}
fn restore(_: usize, generation: u64, saved: *const display.BootSnapshot) bool {
    return r4os_rust_boot_driver_restore(generation, saved, false);
}
fn restoreAdopted(_: usize, generation: u64, saved: *const display.BootSnapshot) bool {
    return r4os_rust_boot_driver_restore(generation, saved, true);
}
fn release(_: usize) bool {
    return r4os_rust_boot_driver_release(false);
}
fn releaseAdopted(_: usize) bool {
    return r4os_rust_boot_driver_release(true);
}
export fn r4os_zig_boot_driver_adapter(id: u32) callconv(.c) bool {
    return native.validAdapter(id);
}
export fn r4os_zig_boot_driver_state() callconv(.c) u32 {
    return @intFromEnum(display.backendState().state);
}
export fn r4os_zig_boot_driver_hold(id: *const Identity, request: *const abi.GfxBootHoldRequest, out: *HoldResult) callconv(.c) i32 {
    const result = display.holdBoot(.{ .owner = @intCast(id.id), .adapter_id = request.adapter_id, .expected_generation = request.generation, .context = 0, .capture = capture, .restore = restore, .restore_adopted = restoreAdopted, .release = release, .release_adopted = releaseAdopted }) catch |err| return native.code(err);
    out.* = .{ .generation = result.generation, .captured = result.captured, .retained = result.retained };
    return 0;
}
export fn r4os_zig_boot_driver_finish(id: u64, generation: u64, operation: u32, out: *u8) callconv(.c) i32 {
    const released = display.finishBoot(@intCast(id), generation, operation) catch |err| {
        if (err != error.RestoreFailed) return native.code(err);
        out.* = 2;
        return 0;
    };
    out.* = @intFromBool(released);
    return 0;
}
export fn r4os_zig_boot_driver_prepare(id: *const Identity, input: *const abi.GfxNativeRegistration, generation: u64, reset: u64, is_reset: bool, output: *abi.GfxNativeState) callconv(.c) i32 {
    return if (is_reset) native.prepareReset(actual(id), input, generation, reset, output) else native.prepareHeld(actual(id), input, generation, output);
}
export fn r4os_zig_boot_driver_snapshot(raw: *const anyopaque, out: *Snapshot) callconv(.c) void {
    const saved = snapshot(raw);
    out.* = .{ .frame = saved.framebuffer, .mapping_bytes = saved.mapping.byte_len, .width = saved.mode.width, .height = saved.mode.height, .pitch = saved.mode.pitch };
}
export fn r4os_zig_boot_driver_description(raw: *const anyopaque, generation: u64, out: *abi.GfxNativeBootInfo) callconv(.c) void {
    out.* = native.bootDescription(generation, snapshot(raw));
}
export fn r4os_zig_boot_buffer_lock(prepared: bool) callconv(.c) bool {
    if (prepared) buffers.lockPrepared(.{ .references = 1, .leases = 1 }) catch return false else buffers.lock();
    return true;
}
export fn r4os_zig_boot_buffer_unlock() callconv(.c) void {
    buffers.unlock();
}
export fn r4os_zig_boot_buffer_describe(value: *const Handle, id: *const Identity, out: *Description) callconv(.c) bool {
    const d = buffers.store.describe(handle(value), actual(id)) catch return false;
    out.* = .{ .bytes = d.bytes, .usage = d.usage, .is_bytes = d.format == .bytes, .is_system = d.location == .system, .portable = d.binding.portable() };
    return true;
}
export fn r4os_zig_boot_buffer_share(value: *const Handle, id: *const Identity, out: *Handle) callconv(.c) bool {
    _ = id;
    out.* = wire(buffers.store.share(handle(value), owner) catch return false);
    return true;
}
export fn r4os_zig_boot_buffer_map(value: *const Handle, write: bool, bytes: u64, out: *Mapping) callconv(.c) bool {
    const mapped = buffers.mapLocked(handle(value), owner, if (write) .cpu_write else .cpu_read, 0, bytes) catch return false;
    out.* = .{ .lease = wire(mapped.lease), .address = mapped.backing.cpu_address };
    return true;
}
export fn r4os_zig_boot_buffer_unmap(value: *const Handle) callconv(.c) bool {
    buffers.unmapCpuLocked(handle(value), owner) catch return false;
    return true;
}
export fn r4os_zig_boot_buffer_drop(value: *const Handle) callconv(.c) bool {
    buffers.drop(handle(value), owner) catch return false;
    return true;
}

comptime {
    if (@sizeOf(Identity) != 24 or @alignOf(Identity) != 8) @compileError("Native boot-driver Identity C layout");
    if (@offsetOf(Identity, "kind") != 0) @compileError("Native boot-driver Identity.kind C offset");
    if (@offsetOf(Identity, "id") != 8) @compileError("Native boot-driver Identity.id C offset");
    if (@offsetOf(Identity, "generation") != 16) @compileError("Native boot-driver Identity.generation C offset");
    if (@sizeOf(Handle) != 16 or @alignOf(Handle) != 8) @compileError("Native boot-driver Handle C layout");
    if (@offsetOf(Handle, "id") != 0) @compileError("Native boot-driver Handle.id C offset");
    if (@offsetOf(Handle, "generation") != 8) @compileError("Native boot-driver Handle.generation C offset");
    if (@sizeOf(Snapshot) != 88 or @alignOf(Snapshot) != 8) @compileError("Native boot-driver Snapshot C layout");
    if (@offsetOf(Snapshot, "frame") != 0) @compileError("Native boot-driver Snapshot.frame C offset");
    if (@offsetOf(Snapshot, "mapping_bytes") != 64) @compileError("Native boot-driver Snapshot.mapping_bytes C offset");
    if (@offsetOf(Snapshot, "width") != 72) @compileError("Native boot-driver Snapshot.width C offset");
    if (@offsetOf(Snapshot, "height") != 76) @compileError("Native boot-driver Snapshot.height C offset");
    if (@offsetOf(Snapshot, "pitch") != 80) @compileError("Native boot-driver Snapshot.pitch C offset");
    if (@sizeOf(Description) != 16 or @alignOf(Description) != 8) @compileError("Native boot-driver Description C layout");
    if (@offsetOf(Description, "bytes") != 0) @compileError("Native boot-driver Description.bytes C offset");
    if (@offsetOf(Description, "usage") != 8) @compileError("Native boot-driver Description.usage C offset");
    if (@offsetOf(Description, "is_bytes") != 12) @compileError("Native boot-driver Description.is_bytes C offset");
    if (@offsetOf(Description, "is_system") != 13) @compileError("Native boot-driver Description.is_system C offset");
    if (@offsetOf(Description, "portable") != 14) @compileError("Native boot-driver Description.portable C offset");
    if (@sizeOf(Mapping) != 24 or @alignOf(Mapping) != 8) @compileError("Native boot-driver Mapping C layout");
    if (@offsetOf(Mapping, "lease") != 0) @compileError("Native boot-driver Mapping.lease C offset");
    if (@offsetOf(Mapping, "address") != 16) @compileError("Native boot-driver Mapping.address C offset");
    if (@sizeOf(HoldResult) != 16 or @alignOf(HoldResult) != 8) @compileError("Native boot-driver HoldResult C layout");
    if (@offsetOf(HoldResult, "generation") != 0) @compileError("Native boot-driver HoldResult.generation C offset");
    if (@offsetOf(HoldResult, "captured") != 8) @compileError("Native boot-driver HoldResult.captured C offset");
    if (@offsetOf(HoldResult, "retained") != 9) @compileError("Native boot-driver HoldResult.retained C offset");
}
