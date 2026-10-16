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
