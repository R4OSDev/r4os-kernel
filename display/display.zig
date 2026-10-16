//! Fixed projection of the sole native DisplayManager. Public source values
//! retain the existing Kernel caller interface; no display state lives here.
const std = @import("std");
const fb = @import("framebuffer.zig");
const blit_backend = @import("blit_backend.zig");
const firmware_access = @import("firmware_access.zig");
pub const backend_state = @import("backend_state.zig");
pub const MAX_PRESENT_REGIONS: usize = 8;
pub const PRESENT_FORMAT_XRGB32: u32 = 1;
pub const PresentRegion = blit_backend.Region;
pub const DeviceKind = enum(u8) {
    none = 0,
    bootfb = 1,
    native = 2,
};

pub const DeviceFlags = struct {
    pub const visible: u32 = 1 << 0;
    pub const fixed_mode: u32 = 1 << 1;
    pub const cpu_present: u32 = 1 << 2;
    pub const rgb32: u32 = 1 << 3;
    pub const fill: u32 = 1 << 4;
    pub const rect: u32 = 1 << 5;
    pub const packed32: u32 = 1 << 6;
    pub const xrgb32: u32 = 1 << 7;
};

pub const Mode = extern struct {
    width: u32 = 0,
    height: u32 = 0,
    pitch: u32 = 0,
    bpp: u16 = 0,
    memory_model: u8 = 0,
    red_mask_size: u8 = 0,
    red_mask_shift: u8 = 0,
    green_mask_size: u8 = 0,
    green_mask_shift: u8 = 0,
    blue_mask_size: u8 = 0,
    blue_mask_shift: u8 = 0,
};

pub const Rect = extern struct {
    x: u32 = 0,
    y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
};

pub const PresentOutcome = extern struct {
    success: bool = false,
    accelerated: bool = false,
    fallback: bool = false,
    source_generation: u64 = 0,
    present_generation: u64 = 0,
    fence: u64 = 0,
    completed_fence: u64 = 0,
    region_count: u32 = 0,
    pixel_count: u32 = 0,
    fallback_regions: u32 = 0,
    backend_error: i32 = 0,
    present_tick: u64 = 0,
    elapsed_ticks: u64 = 0,
    backend_name: [blit_backend.NAME_BYTES]u8 = .{0} ** blit_backend.NAME_BYTES,
};

pub const PresentCapabilities = extern struct {
    flags: u32 = 0,
    formats: u32 = PRESENT_FORMAT_XRGB32,
    max_regions: u32 = MAX_PRESENT_REGIONS,
    backend_kind: u32 = 1,
    backend_name: [blit_backend.NAME_BYTES]u8 = .{0} ** blit_backend.NAME_BYTES,
    fallback_name: [blit_backend.NAME_BYTES]u8 = .{0} ** blit_backend.NAME_BYTES,
};

pub const MappingKind = enum(u8) {
    none = 0,
    bootloader_framebuffer = 1,
    native_scanout = 2,
};

pub const CachePolicy = enum(u8) {
    unknown = 0,
    bootloader_default = 1,
    pat_write_combining = 2,
    write_combining_unsupported = 3,
    write_combining_failed = 4,
};

pub const Mapping = extern struct {
    kind: MappingKind = .none,
    cache_policy: CachePolicy = .unknown,
    virt_base: u64 = 0,
    byte_len: u64 = 0,
    volatile_cpu_writes: bool = false,
};

pub const PresentReason = enum(u8) {
    none = 0,
    fill = 1,
    rect = 2,
    packed32_present = 3,
    xrgb32_present = 4,
};

pub const DisplayTarget = struct {
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    // Low-level backend handle only. Normal display consumers must use the
    // DisplayManager boundary: mode, mapping, stats and present operations.
    framebuffer: ?*fb.Framebuffer = null,
};

pub const Stats = struct {
    registered: bool = false,
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    present_count: u64 = 0,
    present_pixels_total: u64 = 0,
    present_bytes_total: u64 = 0,
    last_present_pixels: u64 = 0,
    last_present_bytes: u64 = 0,
    last_present_rect: Rect = .{},
    last_present_reason: PresentReason = .none,
    last_present_converted: bool = false,
    full_present_count: u64 = 0,
    partial_present_count: u64 = 0,
    fill_present_count: u64 = 0,
    rect_present_count: u64 = 0,
    packed32_present_count: u64 = 0,
    xrgb32_present_count: u64 = 0,
    conversion_present_count: u64 = 0,
    present_total_ticks: u64 = 0,
    present_max_ticks: u64 = 0,
    present_last_ticks: u64 = 0,
    present_slow_count: u64 = 0,
};

pub const BackendView = struct {
    state: backend_state.Snapshot,
    device: Stats,
    boot_mode: Mode,
    boot_mapping: Mapping,
    firmware_writable: bool,
    name: [blit_backend.NAME_BYTES]u8,
};

pub const BootSnapshot = extern struct {
    mode: Mode,
    mapping: Mapping,
    framebuffer: fb.Framebuffer,
};

pub const CommitResult = enum { confirmed, old_preserved, output_lost };

pub const CpuWriteResult = enum { complete, retry, failed };

pub const NativeBackend = struct {
    // adapter_id is the existing PCI inventory identity, never a second index.
    owner: usize,
    adapter_id: u32,
    target: DisplayTarget,
    context: usize = 0,
    commit: *const fn (usize, u64, *const BootSnapshot) CommitResult,
    // True confirms hardware quiescence AND restoration of the saved scanout.
    restore: *const fn (usize, u64, *const BootSnapshot) bool,
    // Optional resident CPU shadow ownership. End completes its upload before
    // normal presentation succeeds; only failed requires proven recovery.
    begin_cpu: ?*const fn (usize) bool = null,
    end_cpu: ?*const fn (usize, bool, ?Rect) CpuWriteResult = null,
};

pub const BootHolder = struct {
    owner: usize,
    adapter_id: u32,
    expected_generation: u64,
    context: usize,
    capture: *const fn (usize, *const BootSnapshot) bool,
    // True proves hardware quiescence and the original CPU scanout mapping,
    // and restores its saved pixels. A timeout/INIT_DONE is not this proof.
    restore: *const fn (usize, u64, *const BootSnapshot) bool,
    restore_adopted: ?*const fn (usize, u64, *const BootSnapshot) bool = null,
    release: *const fn (usize) bool,
    // Native recovery enters from the display owner, outside the original
    // hold/finish caller. Its bridge may need separate lifetime admission.
    release_adopted: ?*const fn (usize) bool = null,
};

pub const BootHoldResult = extern struct { generation: u64, captured: bool, retained: bool };

pub const TransitionError = backend_state.Error || error{ Unavailable, RestoreFailed };
const PresentationError = @import("presentation_stats.zig").Error;
const Text = extern struct { pointer: [*]const u8, len: usize,
    fn from(source: []const u8) Text { return .{ .pointer = source.ptr, .len = source.len }; }
    fn value(self: Text) []const u8 { return self.pointer[0..self.len]; }
};
const NativeStats = extern struct {
    registered: bool = false,
    name: Text = Text.from("none"),
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    present_count: u64 = 0,
    present_pixels_total: u64 = 0,
    present_bytes_total: u64 = 0,
    last_present_pixels: u64 = 0,
    last_present_bytes: u64 = 0,
    last_present_rect: Rect = .{},
    last_present_reason: PresentReason = .none,
    last_present_converted: bool = false,
    full_present_count: u64 = 0,
    partial_present_count: u64 = 0,
    fill_present_count: u64 = 0,
    rect_present_count: u64 = 0,
    packed32_present_count: u64 = 0,
    xrgb32_present_count: u64 = 0,
    conversion_present_count: u64 = 0,
    present_total_ticks: u64 = 0,
    present_max_ticks: u64 = 0,
    present_last_ticks: u64 = 0,
    present_slow_count: u64 = 0,
};
const NativeTarget = extern struct {
    name: Text = Text.from("none"),
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    // Low-level backend handle only. Normal display consumers must use the
    // DisplayManager boundary: mode, mapping, stats and present operations.
    framebuffer: ?*fb.Framebuffer = null,
};
fn publicStats(value: NativeStats) Stats {
    var out: Stats = undefined;
    inline for (std.meta.fields(Stats)) |field| {
        @field(out, field.name) = if (comptime std.mem.eql(u8, field.name, "name")) value.name.value() else @field(value, field.name);
    }
    return out;
}
fn nativeTarget(value: DisplayTarget) NativeTarget { return .{ .name = Text.from(value.name), .kind = value.kind, .flags = value.flags, .mode = value.mode,
    .mapping = value.mapping, .framebuffer = value.framebuffer }; }
const NativeView = extern struct { state: backend_state.Snapshot, device: NativeStats, boot_mode: Mode, boot_mapping: Mapping, firmware_writable: bool, name: [blit_backend.NAME_BYTES]u8 };
const LegacyBackend = extern struct { owner: usize, adapter_id: u32, target: NativeTarget, context: usize, commit: usize, restore: usize, begin_cpu: usize, end_cpu: usize };
fn legacyBackend(value: NativeBackend) LegacyBackend { return .{ .owner = value.owner, .adapter_id = value.adapter_id, .target = nativeTarget(value.target), .context = value.context,
    .commit = @intFromPtr(value.commit), .restore = @intFromPtr(value.restore), .begin_cpu = if (value.begin_cpu) |f| @intFromPtr(f) else 0, .end_cpu = if (value.end_cpu) |f| @intFromPtr(f) else 0 }; }
const LegacyHolder = extern struct { owner: usize, adapter_id: u32, expected_generation: u64, context: usize, capture: usize, restore: usize, restore_adopted: usize, release: usize, release_adopted: usize };
fn legacyHolder(value: BootHolder) LegacyHolder { return .{ .owner = value.owner, .adapter_id = value.adapter_id, .expected_generation = value.expected_generation, .context = value.context,
    .capture = @intFromPtr(value.capture), .restore = @intFromPtr(value.restore), .restore_adopted = if (value.restore_adopted) |f| @intFromPtr(f) else 0,
    .release = @intFromPtr(value.release), .release_adopted = if (value.release_adopted) |f| @intFromPtr(f) else 0 }; }
const Args = extern struct {
    owner: usize = 0, generation: u64 = 0, other_generation: u64 = 0, adapter: u32 = 0, operation: u32 = 0,
    value: u64 = 0, x: i64 = 0, y: i64 = 0, w: u64 = 0, h: u64 = 0, color: u32 = 0, background: u32 = 0,
    count: u32 = 0, stride: u64 = 0, source_generation: u64 = 0, input_tick: u64 = 0, flag: bool = false, has_font: bool = false,
    input: ?*const anyopaque = null, frame: ?*fb.Framebuffer = null, regions: ?[*]const PresentRegion = null, regions_len: usize = 0,
};
extern fn r4os_rust_display_core(u32, *const Args, ?*anyopaque) i32;
fn nativeValue(comptime T: type, op: u32, args: Args) T { var out: T = undefined; if (r4os_rust_display_core(op, &args, &out) != 0) unreachable; return out; }
fn call(op: u32, args: Args) void { if (r4os_rust_display_core(op, &args, null) != 0) unreachable; }
fn transitionChecked(code: i32) TransitionError!void { return switch (code) { 0 => {}, 1 => error.Invalid, 4 => error.Exhausted, 7 => error.Stale,
    8 => error.Busy, 12 => error.Unavailable, 15 => error.Disabled, 21 => error.RestoreFailed, else => unreachable }; }
fn transitionCall(op: u32, args: Args) TransitionError!void { try transitionChecked(r4os_rust_display_core(op, &args, null)); }
fn transitionResult(comptime T: type, op: u32, args: Args) TransitionError!T { var out: T = undefined; try transitionChecked(r4os_rust_display_core(op, &args, &out)); return out; }
fn presentationChecked(code: i32) PresentationError!void { return switch (code) { 0 => {}, 1 => error.Invalid, 3 => error.Unsupported, 6 => error.Capacity, 7 => error.Stale, else => unreachable }; }
fn presentationCall(op: u32, args: Args) PresentationError!void { try presentationChecked(r4os_rust_display_core(op, &args, null)); }
fn presentationResult(comptime T: type, op: u32, args: Args) PresentationError!T { var out: T = undefined; try presentationChecked(r4os_rust_display_core(op, &args, &out)); return out; }

pub fn beginOutputCommit() bool { return nativeValue(bool, 0, .{}); }
pub fn endOutputCommit() void { call(1, .{}); }
pub fn beginSystemTransition(timeout_ticks: u64) bool { return nativeValue(bool, 2, .{ .value = timeout_ticks }); }
pub fn systemTransitionQuiesced() bool { return nativeValue(bool, 3, .{}); }
pub fn backendView() BackendView { const value = nativeValue(NativeView, 4, .{}); return .{ .state = value.state, .device = publicStats(value.device), .boot_mode = value.boot_mode,
    .boot_mapping = value.boot_mapping, .firmware_writable = value.firmware_writable, .name = value.name }; }
pub fn holdBoot(holder: BootHolder) TransitionError!BootHoldResult { const value = legacyHolder(holder); return transitionResult(BootHoldResult, 5, .{ .input = &value }); }
pub fn finishBoot(owner: usize, generation: u64, operation: u32) TransitionError!bool { return transitionResult(bool, 6, .{ .owner = owner, .generation = generation, .operation = operation }); }
const OptionalBoot = extern struct { has: bool, value: BootSnapshot };
pub fn bootSnapshot() ?BootSnapshot { const value = nativeValue(OptionalBoot, 7, .{}); return if (value.has) value.value else null; }
pub fn backendState() backend_state.Snapshot { return nativeValue(backend_state.Snapshot, 8, .{}); }
pub fn retainsDriverOwner(owner: usize) bool { return nativeValue(bool, 9, .{ .owner = owner }); }
pub fn setBackendPolicy(policy: backend_state.Policy) bool { return nativeValue(bool, 10, .{ .operation = @intFromEnum(policy) }); }
pub fn rejectNativeBackend() void { call(11, .{}); }
pub fn prepareNative(backend: NativeBackend) TransitionError!u64 { const value = legacyBackend(backend); return transitionResult(u64, 12, .{ .input = &value }); }
pub fn prepareHeldNative(backend: NativeBackend, held_generation: u64) TransitionError!u64 { const value = legacyBackend(backend); return transitionResult(u64, 13, .{ .input = &value, .generation = held_generation }); }
pub fn prepareRecoveredNative(backend: NativeBackend, held_generation: u64, reset_generation: u64) TransitionError!u64 { const value = legacyBackend(backend); return transitionResult(u64, 14, .{ .input = &value, .generation = held_generation, .other_generation = reset_generation }); }
pub fn abortNative(owner: usize, generation: u64) TransitionError!void { try transitionCall(15, .{ .owner = owner, .generation = generation }); }
pub fn commitNative(owner: usize, generation: u64) TransitionError!CommitResult { return @enumFromInt(try transitionResult(u32, 16, .{ .owner = owner, .generation = generation })); }
pub fn restoreBootBackend(owner: usize, generation: u64) TransitionError!void { try transitionCall(17, .{ .owner = owner, .generation = generation }); }
pub fn beginDeviceReset(owner: usize, generation: u64, adapter: u32) TransitionError!u64 { return transitionResult(u64, 18, .{ .owner = owner, .generation = generation, .adapter = adapter }); }
pub fn restoredBootOwner(owner: usize, generation: u64, adapter: u32) TransitionError!bool { return transitionResult(bool, 19, .{ .owner = owner, .generation = generation, .adapter = adapter }); }
pub fn beginResetRetirement(owner: usize, generation: u64, adapter: u32) TransitionError!void { try transitionCall(20, .{ .owner = owner, .generation = generation, .adapter = adapter }); }
pub fn endResetRetirement() void { call(21, .{}); }
pub fn retireDeviceReset(owner: usize, generation: u64) TransitionError!void { try transitionCall(22, .{ .owner = owner, .generation = generation }); }
pub fn releaseTerminal(owner: usize, generation: u64, adapter: u32) TransitionError!void { try transitionCall(23, .{ .owner = owner, .generation = generation, .adapter = adapter }); }
pub fn replaceNativeFrameLocked(owner: usize, generation: u64, frame: *fb.Framebuffer, bytes: u64) TransitionError!void { try transitionCall(24, .{ .owner = owner, .generation = generation, .frame = frame, .value = bytes }); }
pub fn loseNativeModeLocked(owner: usize, generation: u64) TransitionError!void { try transitionCall(25, .{ .owner = owner, .generation = generation }); }
pub fn registerBootBackend(value: DisplayTarget) void { const wire = nativeTarget(value); call(26, .{ .input = &wire }); }
pub fn activeBackendRegistered() bool { return stats().registered; }
pub fn activeBackendName() []const u8 { return stats().name; }
pub fn activeBackendKind() DeviceKind { return stats().kind; }
pub fn activeMode() ?Mode { const value = stats(); return if (value.registered) value.mode else null; }
pub fn activeMapping() ?Mapping { const value = stats(); return if (value.registered) value.mapping else null; }
pub fn enableFramebufferWriteCombining() bool { return nativeValue(bool, 27, .{}); }
pub fn stats() Stats { return publicStats(nativeValue(NativeStats, 28, .{})); }
pub fn bindPresentationStats(owner: usize, owner_generation: u64, binding: @import("r4os_kernel_contract").GfxBackendBinding, generation: u64) PresentationError!void { try presentationCall(29, .{ .owner = owner, .value = owner_generation, .input = &binding, .generation = generation }); }
pub fn publishPresentationStats(owner: usize, owner_generation: u64, value: @import("r4os_kernel_contract").DisplayPresentationStats) PresentationError!void { try presentationCall(30, .{ .owner = owner, .value = owner_generation, .input = &value }); }
pub fn presentationStats(head: u32) PresentationError!@import("r4os_kernel_contract").DisplayPresentationStats { return presentationResult(@import("r4os_kernel_contract").DisplayPresentationStats, 31, .{ .operation = head }); }
pub fn publishPresentationInfo(owner: usize, owner_generation: u64, value: @import("r4os_kernel_contract").DisplayPresentationInfo) PresentationError!void { try presentationCall(32, .{ .owner = owner, .value = owner_generation, .input = &value }); }
pub fn presentationInfo(head: u32) PresentationError!@import("r4os_kernel_contract").DisplayPresentationInfo { return presentationResult(@import("r4os_kernel_contract").DisplayPresentationInfo, 33, .{ .operation = head }); }
pub fn presentationFeedback(head: u32, source: @import("r4os_kernel_contract").GfxFence) PresentationError!@import("r4os_kernel_contract").DisplayPresentationStats { return presentationResult(@import("r4os_kernel_contract").DisplayPresentationStats, 34, .{ .operation = head, .input = &source }); }
pub fn fill(rgb: u32) bool { return nativeValue(bool, 35, .{ .color = rgb }); }
pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) bool { return nativeValue(bool, 36, .{ .x = x, .y = y, .w = w, .h = h, .color = rgb }); }
pub fn textZ(font_id: ?u32, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) bool { return nativeValue(bool, 37, .{ .has_font = font_id != null, .operation = font_id orelse 0, .x = x, .y = y, .input = @ptrCast(value), .color = fg, .background = bg }); }
pub fn putPacked32(x: u64, y: u64, color32: u32) bool { return nativeValue(bool, 38, .{ .w = x, .h = y, .color = color32 }); }
pub fn putXrgb32(x: u64, y: u64, rgb: u32) bool { return nativeValue(bool, 39, .{ .w = x, .h = y, .color = rgb }); }
pub fn presentPacked32Rect(x: u64, y: u64, w: u64, h: u64, src: []const u8, stride: u64) bool { return nativeValue(bool, 40, .{ .x = @bitCast(x), .y = @bitCast(y), .w = w, .h = h, .input = @ptrCast(src.ptr), .value = src.len, .stride = stride }); }
pub fn presentXrgb32Rect(x: u64, y: u64, w: u64, h: u64, src: []const u8, stride: u64) bool { return nativeValue(bool, 41, .{ .x = @bitCast(x), .y = @bitCast(y), .w = w, .h = h, .input = @ptrCast(src.ptr), .value = src.len, .stride = stride }); }
pub fn presentXrgb32Regions(source: [*]const u32, source_pixel_count: u32, source_stride_pixels: u32, regions: []const PresentRegion, source_generation: u64, input_tick: u64, input_tick_valid: bool) PresentOutcome {
    return nativeValue(PresentOutcome, 42, .{ .input = @ptrCast(source), .count = source_pixel_count, .stride = source_stride_pixels,
        .regions = regions.ptr, .regions_len = regions.len, .source_generation = source_generation, .input_tick = input_tick, .flag = input_tick_valid });
}
pub fn presentCapabilities() PresentCapabilities { return nativeValue(PresentCapabilities, 43, .{}); }
pub fn presentFenceCompleted(fence: u64) bool { return nativeValue(bool, 44, .{ .value = fence }); }
pub fn highestCompletedFence() u64 { return nativeValue(u64, 45, .{}); }
pub fn probeRejectedTakeover() bool { return nativeValue(bool, 46, .{}); }
pub fn operationNames(flags: u32) []const u8 { return nativeValue(Text, 47, .{ .operation = flags }).value(); }
pub fn presentReasonName(reason: PresentReason) []const u8 { return nativeValue(Text, 48, .{ .operation = @intFromEnum(reason) }).value(); }
pub fn mappingKindName(kind: MappingKind) []const u8 { return nativeValue(Text, 49, .{ .operation = @intFromEnum(kind) }).value(); }
pub fn cachePolicyName(policy: CachePolicy) []const u8 { return nativeValue(Text, 50, .{ .operation = @intFromEnum(policy) }).value(); }

// Legacy code addresses are invoked only by a thunk with their precise Zig
// signature. Native C callbacks never pass through a Zig function-ABI cast.
export fn r4os_zig_display_callback(op: u32, code: usize, context: usize, generation: u64, saved: ?*const BootSnapshot, changed: bool, damage: ?*const Rect) callconv(.c) u32 {
    switch (op) {
        0 => { const f: @FieldType(NativeBackend, "commit") = @ptrFromInt(code); return @intFromEnum(f(context, generation, saved.?)); },
        1 => { const f: @FieldType(NativeBackend, "restore") = @ptrFromInt(code); return @intFromBool(f(context, generation, saved.?)); },
        2 => { const f: *const fn (usize) bool = @ptrFromInt(code); return @intFromBool(f(context)); },
        3 => { const f: *const fn (usize, bool, ?Rect) CpuWriteResult = @ptrFromInt(code); return @intFromEnum(f(context, changed, if (damage) |v| v.* else null)); },
        4 => { const f: @FieldType(BootHolder, "capture") = @ptrFromInt(code); return @intFromBool(f(context, saved.?)); },
        5 => { const f: @FieldType(BootHolder, "release") = @ptrFromInt(code); return @intFromBool(f(context)); },
        else => unreachable,
    }
}
pub const DisplayManager = struct {};
comptime { if (@sizeOf(Mode) != 24 or @sizeOf(Mapping) != 32 or @sizeOf(BootSnapshot) != 120) @compileError("Native DisplayManager value C layout"); }

// Only original fixtures access these projections; production state remains
// solely in Rust. No private test write admits hardware or fabricates a receipt.
extern fn r4os_rust_display_fixture(u32, u64, ?*const anyopaque, ?*anyopaque) void;
fn fixtureSet(op: u32, value: u64) void { std.debug.assert(@import("builtin").is_test); r4os_rust_display_fixture(op, value, null, null); }
fn publishStats() void { fixtureSet(24, 0); }
fn fixtureManager() backend_state.Manager { var out: backend_state.Manager = undefined; r4os_rust_display_fixture(0, 0, null, &out); return out; }
fn fixtureSetManager(value: backend_state.Manager) void { r4os_rust_display_fixture(1, 0, &value, null); }
const HeldView = extern struct { has: bool, generation: u64, captured: bool, effects: bool };
const HeldProjection = struct { generation: u64, captured: bool, effects: bool };
fn fixtureHeld() ?HeldProjection { var out: HeldView = undefined; r4os_rust_display_fixture(17, 0, null, &out); return if (out.has) .{ .generation = out.generation, .captured = out.captured, .effects = out.effects } else null; }
fn fixtureHasNative() bool { var out: bool = undefined; r4os_rust_display_fixture(18, 0, null, &out); return out; }
fn fixtureNumber(op: u32) u64 { var out: u64 = undefined; r4os_rust_display_fixture(op, 0, null, &out); return out; }

test "display takeover excludes firmware writers and retains uncertain hardware owners" {
    const t = @import("std").testing;
    try @import("presentation_stats_test.zig").check();
    try @import("cursor_test.zig").check();
    const Probe = struct {
        result: CommitResult = .old_preserved,
        restores: bool = false,
        commits: u32 = 0,
        cpu_active: bool = false,
        cpu_ok: bool = true,
        cpu_retry: bool = false,
        cpu_frames: u32 = 0,
        fn beginCpu(raw: usize) bool {
            const self: *@This() = @ptrFromInt(raw);
            t.expect(!self.cpu_active) catch unreachable;
            self.cpu_active = true;
            return true;
        }
        fn endCpu(raw: usize, changed: bool, damage: ?Rect) CpuWriteResult {
            const self: *@This() = @ptrFromInt(raw);
            t.expect(self.cpu_active) catch unreachable;
            self.cpu_active = false;
            if (changed) self.cpu_frames += 1;
            if (changed and self.cpu_frames == 1) t.expectEqualDeep(Rect{ .x = 2, .y = 2, .w = 2, .h = 2 }, damage.?) catch unreachable;
            return if (self.cpu_retry) .retry else if (self.cpu_ok) .complete else .failed;
        }
        fn commit(raw: usize, _: u64, snapshot: *const BootSnapshot) CommitResult {
            const self: *@This() = @ptrFromInt(raw);
            self.commits += 1;
            t.expect(firmware_access.gate.isRevoked()) catch unreachable;
            t.expectEqual(@as(u32, 8), snapshot.mode.width) catch unreachable;
            // Reentrant presentation must not enter the pending generation.
            t.expect(!fill(0xDDDDDD)) catch unreachable;
            return self.result;
        }
        fn restore(raw: usize, _: u64, _: *const BootSnapshot) bool {
            const self: *@This() = @ptrFromInt(raw);
            return self.restores;
        }
    };
    var probe: Probe = .{};
    var boot_pixels: [64]u32 align(32) = .{0x00112233} ** 64;
    var native_pixels: [64]u32 align(32) = .{0x00445566} ** 64;
    var boot_frame = testFrame(boot_pixels[0..].ptr);
    var native_frame = testFrame(native_pixels[0..].ptr);
    registerBootBackend(testTarget("test-bootfb", .bootfb, &boot_frame));
    defer {
        fixtureSet(4, 0);
        fixtureSet(3, 0);
        fixtureSet(5, 0);
        fixtureSet(6, 0);
        fixtureSet(7, 0);
        fixtureSet(8, 0);
        fixtureSet(9, 0);
        fixtureSet(10, 0);
        fixtureSet(11, 0);
        fixtureSetManager(.{});
        fixtureSet(12, 0);
        fixtureSet(13, 0);
        fixtureSet(14, 0);
        fixtureSet(15, 0);
        fixtureSet(16, 0);
        firmware_access.gate.* = .{};
    }
    const backend = NativeBackend{
        .owner = 92,
        .adapter_id = 4,
        .target = testTarget("native-test", .native, &native_frame),
        .context = @intFromPtr(&probe),
        .commit = Probe.commit,
        .restore = Probe.restore,
        .begin_cpu = Probe.beginCpu,
        .end_cpu = Probe.endCpu,
    };
    var malformed = backend;
    malformed.target.mode.red_mask_shift = 8;
    try t.expectError(error.Invalid, prepareNative(malformed));
    malformed = backend;
    malformed.owner = @as(usize, 1) << 32;
    try t.expectError(error.Invalid, prepareNative(malformed));
    malformed = backend;
    malformed.target.mode.width = 4;
    native_frame.width = 4;
    try t.expectError(error.Invalid, prepareNative(malformed));
    native_frame.width = 8;
    try t.expect(setBackendPolicy(.software));
    try t.expectError(error.Disabled, prepareNative(backend));
    try t.expect(fill(0xABCDEF));
    try t.expect(setBackendPolicy(.automatic));
    rejectNativeBackend();
    try t.expectEqual(backend_state.Reason.backend_rejected, backendState().reason);
    const first = try prepareNative(backend);
    try t.expect(firmware_access.gate.tryAcquire());
    try t.expectError(error.Busy, commitNative(92, first));
    try t.expectEqual(@as(u32, 0), probe.commits);
    firmware_access.gate.release();
    try t.expectEqual(CommitResult.old_preserved, try commitNative(92, first));
    try t.expectEqual(backend_state.State.bootfb, backendState().state);
    try t.expect(!retainsDriverOwner(92));
    try t.expect(!firmware_access.gate.isRevoked());
    const before = boot_pixels;
    probe.result = .confirmed;
    const second = try prepareNative(backend);
    try t.expectError(error.Stale, commitNative(92, first));
    try t.expectEqual(CommitResult.confirmed, try commitNative(92, second));
    try t.expect(retainsDriverOwner(92));
    try t.expectEqual(backend_state.State.software_native, backendState().state);
    try t.expect(!firmware_access.gate.tryAcquire());
    var source: [64]u32 = .{0x765432} ** 64;
    const region = PresentRegion{ .dst_x = 2, .dst_y = 2, .src_x = 0, .src_y = 0, .w = 2, .h = 2 };
    const presented = presentXrgb32Regions(&source, source.len, 8, (&region)[0..1], 71, 0, false);
    try t.expect(presented.success);
    try t.expectEqualSlices(u32, &before, &boot_pixels);
    try t.expectEqual(@as(u32, 0x765432), native_pixels[18]);
    try t.expectEqual(@as(u32, 0x00445566), native_pixels[17]);
    try t.expectEqualStrings("native-cpu", nameSlice(&presented.backend_name));
    try t.expect(!probe.cpu_active and probe.cpu_frames == 1);
    // A completed variable geometry change republishes the shared primary
    // while keeping the boot timing/mapping and original pixels immutable.
    var resized = native_frame;
    resized.width = 4; resized.height = 4; resized.pitch = 16;
    try t.expect(beginOutputCommit());
    try replaceNativeFrameLocked(92, second, &resized, 64);
    try t.expect(stats().mode.width == 4 and stats().mode.height == 4 and bootSnapshot().?.mode.width == 8);
    try t.expect(@import("surface_pipeline.zig").width() == 4 and @import("surface_pipeline.zig").height() == 4);
    try t.expect(@import("surface_pipeline.zig").pixelBounds().width == 4 and @import("surface_pipeline.zig").pixelBounds().height == 4);
    try loseNativeModeLocked(92, second);
    try t.expect(!stats().registered and retainsDriverOwner(92));
    try t.expect(@import("surface_pipeline.zig").width() == 0 and @import("surface_pipeline.zig").height() == 0);
    try t.expect(@import("surface_pipeline.zig").pixelBounds().width == 0);
    try replaceNativeFrameLocked(92, second, &native_frame, 256);
    endOutputCommit();
    try t.expect(stats().mode.width == 8 and @import("std").mem.eql(u32, &before, &boot_pixels));
    try t.expect(@import("surface_pipeline.zig").width() == 8 and @import("surface_pipeline.zig").height() == 8);
    try t.expect(@import("surface_pipeline.zig").pixelBounds().width == 8);
    const live_generation = backendState().generation;
    const live_reset = backendState().reset_generation;
    probe.cpu_retry = true;
    try t.expect(!fill(0x998877));
    try t.expect(!probe.cpu_active and backendState().state == .software_native);
    try t.expect(backendState().generation == live_generation and backendState().reset_generation == live_reset);
    try t.expect(retainsDriverOwner(92) and firmware_access.gate.isRevoked());
    probe.cpu_retry = false;
    try t.expect(fill(0x998877));
    probe.cpu_ok = false;
    try t.expect(!fill(0x112233)); // Failed upload triggers the same recovery.
    try t.expect(!probe.cpu_active and probe.cpu_frames == 4);
    try t.expectEqual(@as(u64, 2), backendState().reset_generation);
    try t.expect(!fill(0xAAAAAA));
    try t.expectEqual(@as(u32, 0), presentCapabilities().flags);
    try t.expect(retainsDriverOwner(92));
    try t.expect(firmware_access.gate.isRevoked());
    try t.expectError(error.Stale, restoreBootBackend(92, second));
    probe.restores = true;
    try restoreBootBackend(92, backendState().generation);
    try t.expectEqual(@as(u64, 0), backendState().reset_generation);
    try t.expect(backendState().generation > second);
    try t.expect(!retainsDriverOwner(92));
    try t.expect(!firmware_access.gate.isRevoked());
    try t.expectEqualSlices(u32, &before, &boot_pixels);
    try t.expect(fill(0x123456));
    try t.expectEqual(@as(u32, 0x123456), boot_pixels[0]);
    try exerciseBootHold(backend);
    try exerciseHeldNative(backend);
    try exerciseSystemTransition(backend);
}

fn exerciseHeldNative(template: NativeBackend) !void {
    const t = @import("std").testing;
    const Probe = struct {
        pixels: [64]u32 = undefined,
        captures: u32 = 0,
        old_restores: u32 = 0,
        native_restores: u32 = 0,
        releases: u32 = 0,
        restore_ok: bool = false,
        release_ok: bool = false,
        result: CommitResult = .old_preserved,
        fn capture(raw: usize, saved: *const BootSnapshot) bool {
            const self: *@This() = @ptrFromInt(raw);
            self.captures += 1;
            const source: [*]volatile const u32 = @ptrCast(@alignCast(saved.framebuffer.address));
            for (&self.pixels, 0..) |*pixel, index| pixel.* = source[index];
            return true;
        }
        fn oldRestore(raw: usize, _: u64, _: *const BootSnapshot) bool {
            @as(*@This(), @ptrFromInt(raw)).old_restores += 1;
            return false; // An adopted hold must use its native recovery owner.
        }
        fn release(_: usize) bool { return false; }
        fn releaseAdopted(raw: usize) bool {
            const self: *@This() = @ptrFromInt(raw);
            self.releases += 1;
            t.expect(firmware_access.gate.isRevoked() and !fill(0xABCDEF)) catch unreachable;
            return self.release_ok;
        }
        fn commit(raw: usize, generation: u64, _: *const BootSnapshot) CommitResult {
            t.expect(firmware_access.gate.isRevoked() and fixtureHeld().?.captured and fixtureHeld().?.effects) catch unreachable;
            t.expectEqual(generation, backendState().pending_generation) catch unreachable;
            return @as(*@This(), @ptrFromInt(raw)).result;
        }
        fn restore(raw: usize, _: u64, saved: *const BootSnapshot) bool {
            const self: *@This() = @ptrFromInt(raw);
            self.native_restores += 1;
            if (!self.restore_ok) return false;
            const target: [*]volatile u32 = @ptrCast(@alignCast(saved.framebuffer.address));
            for (self.pixels, 0..) |pixel, index| target[index] = pixel;
            return true;
        }
    };
    var probe: Probe = .{};
    var candidate = template;
    candidate.owner = 91;
    candidate.context = @intFromPtr(&probe);
    candidate.commit = Probe.commit;
    candidate.restore = Probe.restore;
    candidate.begin_cpu = null;
    candidate.end_cpu = null;
    var holder = BootHolder{ .owner = 91, .adapter_id = candidate.adapter_id, .expected_generation = backendState().generation,
        .context = @intFromPtr(&probe), .capture = Probe.capture, .restore = Probe.oldRestore,
        .release = Probe.release, .release_adopted = Probe.releaseAdopted, .restore_adopted = Probe.restore };
    try t.expectError(error.Stale, prepareHeldNative(candidate, 0));
    try t.expectError(error.Stale, prepareHeldNative(candidate, holder.expected_generation));
    const held = try holdBoot(holder);
    try t.expectError(error.Invalid, prepareHeldNative(candidate, held.generation));
    try t.expect(!try finishBoot(91, held.generation, 1));
    try t.expectError(error.Stale, prepareHeldNative(template, held.generation));
    var wrong_adapter = candidate;
    wrong_adapter.adapter_id += 1;
    try t.expectError(error.Stale, prepareHeldNative(wrong_adapter, held.generation));
    try t.expectError(error.Stale, prepareHeldNative(candidate, held.generation + 1));
    try t.expectEqual(held.generation, try prepareHeldNative(candidate, held.generation));
    try t.expectError(error.Busy, finishBoot(91, held.generation, 2));
    try t.expectError(error.Busy, prepareHeldNative(candidate, held.generation));
    try t.expectError(error.Stale, abortNative(91, held.generation + 1));
    try abortNative(91, held.generation);
    try t.expectEqual(backend_state.State.preparing, backendState().state);
    try t.expectEqual(held.generation, backendState().pending_generation);
    try t.expectEqual(holder.expected_generation, backendState().generation);
    try t.expect(!fill(0x123456) and firmware_access.gate.isRevoked());
    _ = try prepareHeldNative(candidate, held.generation);
    try t.expectEqual(CommitResult.old_preserved, try commitNative(91, held.generation));
    try t.expect(!fixtureHasNative() and fixtureHeld() != null and retainsDriverOwner(91));
    try t.expectEqual(held.generation, backendState().pending_generation);
    try t.expect(firmware_access.gate.isRevoked() and !fill(0x123456));
    try t.expectEqual(@as(u32, 0), probe.old_restores + probe.releases);

    _ = try prepareHeldNative(candidate, held.generation);
    probe.result = .confirmed;
    try t.expectEqual(CommitResult.confirmed, try commitNative(91, held.generation));
    try t.expectEqual(backend_state.State.software_native, backendState().state);
    try t.expectEqual(held.generation, backendState().generation);
    try t.expect(fixtureHeld() != null and firmware_access.gate.isRevoked());
    try t.expectError(error.Busy, finishBoot(91, held.generation, 2));
    try t.expectError(error.RestoreFailed, restoreBootBackend(91, held.generation));
    try t.expectEqual(@as(u32, 1), probe.native_restores);
    try t.expect(fixtureHeld() != null and retainsDriverOwner(91));
    try t.expectError(error.Stale, restoreBootBackend(91, held.generation));
    // A deferred driver restore can escalate to the physical reset worker.
    // It must use the failed restoration's current generation, while the
    // immutable boot capture and old native registration stay identifiable.
    try t.expectError(error.Stale, beginDeviceReset(91, held.generation, candidate.adapter_id));
    const deferred_generation = backendState().generation;
    const deferred_reset = try beginDeviceReset(91, deferred_generation, candidate.adapter_id);
    try t.expect(deferred_reset > deferred_generation and fixtureHeld().?.generation == held.generation and
        backendState().state == .recovering and firmware_access.gate.isRevoked());
    probe.restore_ok = true;
    try t.expectError(error.RestoreFailed, restoreBootBackend(91, backendState().generation));
    try t.expectEqual(@as(u32, 2), probe.native_restores);
    try t.expectEqual(@as(u32, 1), probe.releases);
    try t.expect(fixtureHeld() != null and firmware_access.gate.isRevoked());
    probe.release_ok = true;
    try restoreBootBackend(91, backendState().generation);
    try t.expectEqual(@as(u32, 2), probe.native_restores);
    try t.expectEqual(@as(u32, 2), probe.releases);
    try t.expectEqual(@as(u32, 0), probe.old_restores);
    try t.expectEqual(@as(u32, 1), probe.captures);
    try t.expect(fixtureHeld() == null and !retainsDriverOwner(91) and !firmware_access.gate.isRevoked());

    // Whole-device reset during firmware startup keeps the original hold,
    // but each reconstructed native candidate has a distinct generation.
    holder.expected_generation = backendState().generation;
    const reset_hold = try holdBoot(holder);
    try t.expect(!try finishBoot(91, reset_hold.generation, 1));
    try t.expectError(error.Stale, beginDeviceReset(92, reset_hold.generation, candidate.adapter_id));
    const reset = try beginDeviceReset(91, reset_hold.generation, candidate.adapter_id);
    try t.expect(reset > reset_hold.generation and !fill(0x123456) and firmware_access.gate.isRevoked());
    try t.expectError(error.Busy, beginDeviceReset(91, reset, candidate.adapter_id));
    try t.expectError(error.Stale, retireDeviceReset(91, reset_hold.generation));
    try retireDeviceReset(91, reset);
    // No scanout preparation occurs during headless restart. An additional
    // physical stop requires a fresh common generation and fresh retirement.
    try t.expectError(error.Stale, beginDeviceReset(92, reset, candidate.adapter_id));
    const headless_reset = try beginDeviceReset(91, reset, candidate.adapter_id);
    try t.expect(headless_reset > reset and fixtureHeld().?.generation == reset_hold.generation and
        backendState().state == .recovering and !fill(0x123456) and fixtureNumber(19) == 0);
    try t.expectError(error.Busy, beginDeviceReset(91, headless_reset, candidate.adapter_id));
    try t.expectError(error.Stale, retireDeviceReset(91, reset));
    try retireDeviceReset(91, headless_reset);
    try t.expectError(error.Stale, prepareRecoveredNative(candidate, reset_hold.generation + 1, reset));
    const prepared = try prepareRecoveredNative(candidate, reset_hold.generation, headless_reset);
    try t.expect(prepared > reset and fixtureNumber(20) == reset_hold.generation);
    try abortNative(91, prepared);
    try t.expect(backendState().state == .unavailable and fixtureHeld() != null and !fill(0xFFFFFF));
    const retry = try beginDeviceReset(91, headless_reset, candidate.adapter_id);
    try retireDeviceReset(91, retry);
    const fresh = try prepareRecoveredNative(candidate, reset_hold.generation, retry);
    try t.expectError(error.Stale, commitNative(91, prepared));
    probe.result = .confirmed;
    try t.expectEqual(CommitResult.confirmed, try commitNative(91, fresh));
    try t.expect(backendState().generation == fresh and fixtureHeld().?.generation == reset_hold.generation);
    try t.expectError(error.Stale, beginDeviceReset(91, reset_hold.generation, candidate.adapter_id));
    try @import("native_driver.zig").checkConsoleRestoreForTest(91, fresh, candidate.adapter_id);
    try t.expect(fixtureHeld() == null and !firmware_access.gate.isRevoked());

    // A physically reconstructed original console keeps its GPU alive after
    // common bootfb ownership returns. Shutdown needs a fresh logical reset,
    // followed by a separate physical stop; the old receipt is insufficient.
    const restored_manager = fixtureManager();
    const restored_generation = backendState().generation;
    const before_console_restores = probe.native_restores + probe.old_restores;
    try t.expectError(error.Busy, beginDeviceReset(91, restored_generation, candidate.adapter_id));
    try t.expect(beginSystemTransition(0));
    try t.expectError(error.Stale, beginDeviceReset(92, restored_generation, candidate.adapter_id));
    try t.expectError(error.Stale, beginDeviceReset(91, restored_generation - 1, candidate.adapter_id));
    try t.expectError(error.Stale, beginDeviceReset(91, restored_generation, candidate.adapter_id + 1));
    const console_reset = try beginDeviceReset(91, restored_generation, candidate.adapter_id);
    try t.expect(console_reset > restored_generation and backendState().state == .recovering and
        fixtureHeld() == null and !fill(0xabcdef) and firmware_access.gate.isRevoked());
    try t.expectError(error.Busy, releaseTerminal(91, console_reset, candidate.adapter_id));
    try retireDeviceReset(91, console_reset);
    try releaseTerminal(91, console_reset, candidate.adapter_id);
    try t.expect(backendState().state == .unavailable and systemTransitionQuiesced() and
        before_console_restores == probe.native_restores + probe.old_restores);
    // Continue the independent original scenarios from their saved CPU model.
    fixtureSetManager(restored_manager);
    fixtureSet(3, 0);
    fixtureSet(4, 1);
    firmware_access.gate.restore();
    publishStats();

    for ([_]CommitResult{ .output_lost, .confirmed }) |result| {
        holder.expected_generation = backendState().generation;
        const next = try holdBoot(holder);
        try t.expect(!try finishBoot(91, next.generation, 1));
        _ = try prepareHeldNative(candidate, next.generation);
        probe.result = result;
        try t.expectEqual(result, try commitNative(91, next.generation));
        try t.expect(fixtureHeld() != null and retainsDriverOwner(91));
        if (result == .output_lost) try t.expectEqual(backend_state.State.unavailable, backendState().state);
        if (result == .confirmed) {
            try t.expectError(error.Busy, beginResetRetirement(91, next.generation, candidate.adapter_id));
            try t.expect(beginSystemTransition(0));
            try t.expect(!systemTransitionQuiesced());
            // A live R4D must still invalidate and retire its exact native
            // generation after terminal admission has stopped presentation.
            // These are teardown receipts, never new-output admissions.
            try t.expectError(error.Stale, beginDeviceReset(92, next.generation, candidate.adapter_id));
            try t.expectError(error.Stale, beginDeviceReset(91, next.generation - 1, candidate.adapter_id));
            try t.expectError(error.Stale, beginDeviceReset(91, next.generation, candidate.adapter_id + 1));
            const terminal_reset = try beginDeviceReset(91, next.generation, candidate.adapter_id);
            try t.expect(terminal_reset > next.generation and fixtureHeld().?.generation == next.generation);
            try t.expect(!systemTransitionQuiesced() and !beginOutputCommit() and !fill(0xFFFFFF));
            try t.expectError(error.Stale, beginResetRetirement(92, terminal_reset, candidate.adapter_id));
            try t.expectError(error.Stale, beginResetRetirement(91, next.generation, candidate.adapter_id));
            try t.expectError(error.Stale, beginResetRetirement(91, terminal_reset, candidate.adapter_id + 1));
            try t.expectError(error.Stale, beginResetRetirement(91, terminal_reset, 0));
            try beginResetRetirement(91, terminal_reset, candidate.adapter_id);
            try t.expect(fixtureHasNative() and fixtureHeld() != null and retainsDriverOwner(91));
            try t.expect(!systemTransitionQuiesced() and firmware_access.gate.isRevoked() and !beginOutputCommit());
            endResetRetirement();
            try t.expectError(error.Stale, retireDeviceReset(92, terminal_reset));
            try t.expectError(error.Stale, retireDeviceReset(91, next.generation));
            try retireDeviceReset(91, terminal_reset);
            try t.expect(!fixtureHasNative() and fixtureHeld() != null and retainsDriverOwner(91));
            try t.expect(!systemTransitionQuiesced() and firmware_access.gate.isRevoked());
            try t.expectError(error.Busy, prepareRecoveredNative(candidate, next.generation, terminal_reset));
            probe.restore_ok = false;
            try t.expectError(error.RestoreFailed, restoreBootBackend(91, terminal_reset));
            try t.expect(!systemTransitionQuiesced() and retainsDriverOwner(91) and !fill(0xFFFFFF));
            probe.restore_ok = true;
        }
        try restoreBootBackend(91, backendState().generation);
        try t.expect(fixtureHeld() == null and !retainsDriverOwner(91));
    }
    try t.expect(systemTransitionQuiesced() and firmware_access.gate.isRevoked() and !fill(0xFFFFFF));
    try t.expectEqual(@as(u32, 0), probe.old_restores);
    fixtureSet(3, 0);
    fixtureSet(4, 1);
    firmware_access.gate.restore();

    // Shutdown may discard the captured RAM image after physical-reset
    // retirement, with either a headless boot hold or an adopted display.
    // It never depends on recovering the old scanout or writes its pixels.
    for ([_]bool{ false, true }) |adopt| {
        const before_manager = fixtureManager();
        holder.expected_generation = backendState().generation;
        const final_hold = try holdBoot(holder);
        try t.expect(!try finishBoot(91, final_hold.generation, 1));
        if (adopt) {
            _ = try prepareHeldNative(candidate, final_hold.generation);
            probe.result = .confirmed;
            _ = try commitNative(91, final_hold.generation);
        }
        try t.expectError(error.Busy, releaseTerminal(91, final_hold.generation, candidate.adapter_id));
        try t.expect(beginSystemTransition(0));
        const final_reset = try beginDeviceReset(91, final_hold.generation, candidate.adapter_id);
        try t.expectError(error.Busy, releaseTerminal(91, final_reset, candidate.adapter_id));
        try t.expectError(error.Stale, releaseTerminal(92, final_reset, candidate.adapter_id));
        try t.expectError(error.Stale, releaseTerminal(91, final_hold.generation, candidate.adapter_id));
        try t.expectError(error.Stale, releaseTerminal(91, final_reset, candidate.adapter_id + 1));
        try retireDeviceReset(91, final_reset);
        const before_release = backendState();
        const restores = probe.native_restores + probe.old_restores;
        const serial = fixtureManager().serial;
        const releases = probe.releases;
        fixtureSet(2, ~@as(u64, 0));
        try t.expectError(error.Exhausted, releaseTerminal(91, final_reset, candidate.adapter_id));
        try t.expectEqual(releases, probe.releases);
        fixtureSet(2, serial);
        probe.restore_ok = false;
        probe.release_ok = false;
        try t.expectError(error.Busy, releaseTerminal(91, final_reset, candidate.adapter_id));
        try t.expectEqualDeep(before_release, backendState());
        try t.expect(fixtureHeld() != null and retainsDriverOwner(91) and !systemTransitionQuiesced());
        var pixels: [64]u32 = undefined;
        const source: [*]volatile const u32 = @ptrCast(@alignCast(bootSnapshot().?.framebuffer.address));
        for (&pixels, 0..) |*pixel, index| pixel.* = source[index];
        probe.release_ok = true;
        try releaseTerminal(91, final_reset, candidate.adapter_id);
        try t.expectEqual(restores, probe.native_restores + probe.old_restores);
        try t.expectEqual(releases + 2, probe.releases);
        for (pixels, 0..) |pixel, index| try t.expectEqual(pixel, source[index]);
        try t.expect(systemTransitionQuiesced() and !retainsDriverOwner(91) and fixtureHeld() == null);
        try t.expect(backendState().state == .unavailable and backendState().generation > final_reset);
        try t.expect(firmware_access.gate.isRevoked() and !fill(0xFFFFFF) and !beginOutputCommit());
        try t.expectError(error.Stale, releaseTerminal(91, final_reset, candidate.adapter_id));
        try t.expectError(error.Busy, prepareNative(candidate));
        // Test-only restoration of the surrounding fixture; the real system
        // transition cannot be cancelled or republish the old framebuffer.
        fixtureSetManager(before_manager);
        fixtureSet(3, 0);
        fixtureSet(4, 1);
        firmware_access.gate.restore();
        publishStats();
    }
}

fn exerciseBootHold(native: NativeBackend) !void {
    const t = @import("std").testing;
    const Probe = struct {
        capture_ok: bool = true,
        release_ok: bool = true,
        restore_ok: bool = false,
        captures: u32 = 0,
        restores: u32 = 0,
        pixels: [64]u32 = undefined,
        fn capture(raw: usize, saved: *const BootSnapshot) bool {
            const self: *@This() = @ptrFromInt(raw);
            self.captures += 1;
            t.expect(firmware_access.gate.isRevoked() and !fill(0xAAAAAA)) catch unreachable;
            t.expect(retainsDriverOwner(91)) catch unreachable;
            const bytes: [*]volatile const u32 = @ptrCast(@alignCast(saved.framebuffer.address));
            for (&self.pixels, 0..) |*pixel, index| pixel.* = bytes[index];
            return self.capture_ok;
        }
        fn restore(raw: usize, _: u64, saved: *const BootSnapshot) bool {
            const self: *@This() = @ptrFromInt(raw);
            self.restores += 1;
            t.expect(firmware_access.gate.isRevoked() and !fill(0xBBBBBB)) catch unreachable;
            if (!self.restore_ok) return false;
            const bytes: [*]volatile u32 = @ptrCast(@alignCast(saved.framebuffer.address));
            for (self.pixels, 0..) |pixel, index| bytes[index] = pixel;
            return true;
        }
        fn release(raw: usize) bool { return @as(*@This(), @ptrFromInt(raw)).release_ok; }
    };
    var probe: Probe = .{};
    const before = backendState();
    var holder = BootHolder{ .owner = 91, .adapter_id = 4, .expected_generation = before.generation,
        .context = @intFromPtr(&probe), .capture = Probe.capture, .restore = Probe.restore, .release = Probe.release };
    holder.expected_generation -= 1;
    try t.expectError(error.Stale, holdBoot(holder));
    holder.expected_generation = before.generation;
    try t.expect(setBackendPolicy(.software_once));
    try t.expectError(error.Disabled, holdBoot(holder));
    try t.expect(setBackendPolicy(.automatic));
    try t.expect(firmware_access.gate.tryAcquire());
    try t.expectError(error.Busy, holdBoot(holder));
    firmware_access.gate.release();
    try t.expectEqual(@as(u32, 0), probe.captures);
    probe.capture_ok = false;
    const rejected = try holdBoot(holder);
    try t.expect(!rejected.captured and !rejected.retained and !firmware_access.gate.isRevoked());
    probe.release_ok = false;
    const partial = try holdBoot(holder);
    try t.expect(!partial.captured and partial.retained);
    try t.expectError(error.RestoreFailed, finishBoot(91, partial.generation, 0));
    probe.release_ok = true;
    try t.expect(try finishBoot(91, partial.generation, 0));
    probe.capture_ok = true;
    const held = try holdBoot(holder);
    try t.expect(held.captured and held.retained and !fill(0xCCCCCC));
    try t.expect(!firmware_access.gate.tryAcquire());
    try t.expectEqual(@as(u32, 0), presentCapabilities().flags);
    try t.expectError(error.Busy, holdBoot(holder));
    try t.expectError(error.Busy, prepareNative(native));
    try t.expectError(error.Busy, abortNative(91, held.generation));
    try t.expectError(error.Busy, commitNative(91, held.generation));
    try t.expectError(error.Stale, finishBoot(92, held.generation, 0));
    try t.expectError(error.Stale, finishBoot(91, rejected.generation, 0));
    try t.expectError(error.Invalid, finishBoot(91, held.generation, 3));
    try t.expect(!try finishBoot(91, held.generation, 1));
    try t.expectError(error.Busy, finishBoot(91, held.generation, 1));
    try t.expectError(error.Busy, finishBoot(91, held.generation, 0));
    try t.expectError(error.RestoreFailed, finishBoot(91, held.generation, 2));
    try t.expect(retainsDriverOwner(91) and firmware_access.gate.isRevoked());
    probe.restore_ok = true;
    probe.release_ok = false;
    try t.expectError(error.RestoreFailed, finishBoot(91, held.generation, 2));
    const calls = probe.restores;
    probe.release_ok = true;
    try t.expect(try finishBoot(91, held.generation, 2));
    try t.expectEqual(calls, probe.restores); // Proven hardware restore is not replayed.
    try t.expect(!retainsDriverOwner(91) and !firmware_access.gate.isRevoked());
    try t.expectEqual(before.generation, backendState().generation);
    try t.expectEqual(before.reset_generation, backendState().reset_generation);
    try t.expectError(error.Stale, finishBoot(91, held.generation, 0));
    try t.expect(fill(0x123456));

    // A retired firmware-startup generation has no native bridge left. Its
    // saved hardware owner still performs the real boot restore, once only.
    const reset_hold = try holdBoot(holder);
    try t.expect(!try finishBoot(91, reset_hold.generation, 1));
    const reset = try beginDeviceReset(91, reset_hold.generation, holder.adapter_id);
    try retireDeviceReset(91, reset);
    probe.restore_ok = false;
    try t.expectError(error.RestoreFailed, restoreBootBackend(91, reset));
    try t.expect(fixtureHeld() != null and firmware_access.gate.isRevoked() and !fill(0xFFFF00));
    probe.restore_ok = true; probe.release_ok = false;
    try t.expectError(error.RestoreFailed, restoreBootBackend(91, backendState().generation));
    const reset_calls = probe.restores;
    probe.release_ok = true;
    try restoreBootBackend(91, backendState().generation);
    try t.expectEqual(reset_calls, probe.restores);
    try t.expect(fixtureHeld() == null and !firmware_access.gate.isRevoked());
    holder.expected_generation = backendState().generation;

    // Terminal shutdown still permits the actual held-display recovery, but
    // neither its success nor a retry may reopen framebuffer access.
    const final_hold = try holdBoot(holder);
    try t.expect(!try finishBoot(91, final_hold.generation, 1));
    try t.expect(beginSystemTransition(0));
    try t.expect(!systemTransitionQuiesced());
    try t.expectError(error.Busy, finishBoot(91, final_hold.generation, 1));
    probe.restore_ok = false;
    try t.expectError(error.RestoreFailed, finishBoot(91, final_hold.generation, 2));
    try t.expect(!systemTransitionQuiesced() and retainsDriverOwner(91));
    probe.restore_ok = true;
    try t.expect(try finishBoot(91, final_hold.generation, 2));
    try t.expect(systemTransitionQuiesced());
    try t.expect(!fill(0xFFFFFF) and firmware_access.gate.isRevoked());
    // Test-only reset; production has no way to cancel a system transition.
    fixtureSet(3, 0);
    fixtureSet(4, 1);
    firmware_access.gate.restore();
}

fn exerciseSystemTransition(native: NativeBackend) !void {
    const t = @import("std").testing;
    try t.expect(!systemTransitionQuiesced());
    try t.expect(beginOutputCommit());
    try t.expect(!beginSystemTransition(0));
    endOutputCommit();
    try t.expect(firmware_access.gate.tryAcquire());
    try t.expect(!beginSystemTransition(0));
    firmware_access.gate.release();
    try t.expect(fill(0x123456));

    const pending = try prepareNative(native);
    try t.expect(beginSystemTransition(0));
    try t.expect(beginSystemTransition(0));
    try t.expect(!systemTransitionQuiesced());
    try t.expect(!beginOutputCommit() and !setBackendPolicy(.software));
    try t.expectError(error.Busy, prepareNative(native));
    try t.expectError(error.Busy, commitNative(native.owner, pending));
    try abortNative(native.owner, pending);
    try t.expect(systemTransitionQuiesced());
    try t.expect(!fill(0xFFFFFF) and !firmware_access.gate.tryAcquire());
    fixtureSet(3, 0);
    fixtureSet(4, 1);
    firmware_access.gate.restore();

    const active = try prepareNative(native);
    try t.expectEqual(CommitResult.confirmed, try commitNative(native.owner, active));
    try t.expect(beginSystemTransition(0));
    try t.expect(!systemTransitionQuiesced());
    try restoreBootBackend(native.owner, active);
    try t.expect(systemTransitionQuiesced());
    try t.expect(!fill(0xFFFFFF) and !firmware_access.gate.tryAcquire());
}

fn nameSlice(name: []const u8) []const u8 {
    return name[0 .. @import("std").mem.indexOfScalar(u8, name, 0) orelse name.len];
}

fn testFrame(pixels: [*]u32) fb.Framebuffer {
    return .{ .address = @ptrCast(pixels), .width = 8, .height = 8, .pitch = 32, .bpp = 32, .memory_model = 1, .red_mask_size = 8, .red_mask_shift = 16, .green_mask_size = 8, .green_mask_shift = 8, .blue_mask_size = 8, .blue_mask_shift = 0, .unused = .{0} ** 5, .edid_size = 0, .edid = null };
}

fn testTarget(name: []const u8, kind: DeviceKind, frame: *fb.Framebuffer) DisplayTarget {
    return .{ .name = name, .kind = kind, .framebuffer = frame, .flags = DeviceFlags.visible | DeviceFlags.cpu_present | DeviceFlags.rgb32 | DeviceFlags.xrgb32, .mode = .{ .width = 8, .height = 8, .pitch = 32, .bpp = 32, .red_mask_size = 8, .red_mask_shift = 16, .green_mask_size = 8, .green_mask_shift = 8, .blue_mask_size = 8, .blue_mask_shift = 0 }, .mapping = .{ .kind = if (kind == .bootfb) .bootloader_framebuffer else .native_scanout, .virt_base = @intFromPtr(frame.address), .byte_len = 256, .volatile_cpu_writes = true } };
}

test "external blit error falls back once and preserves exact damage" {
    const testing = @import("std").testing;
    const FailBackend = struct {
        fn present(_: usize, _: *const blit_backend.Job) callconv(.c) i32 {
            return -77;
        }
    };

    var target: [64]u32 align(32) = .{0xA5A5_A5A5} ** 64;
    var frame = fb.Framebuffer{
        .address = @ptrCast(target[0..].ptr),
        .width = 8,
        .height = 8,
        .pitch = 8 * @sizeOf(u32),
        .bpp = 32,
        .memory_model = 1,
        .red_mask_size = 8,
        .red_mask_shift = 16,
        .green_mask_size = 8,
        .green_mask_shift = 8,
        .blue_mask_size = 8,
        .blue_mask_shift = 0,
        .unused = .{0} ** 5,
        .edid_size = 0,
        .edid = null,
    };
    registerBootBackend(.{
        .name = "test-bootfb",
        .kind = .bootfb,
        .flags = DeviceFlags.visible | DeviceFlags.cpu_present | DeviceFlags.rgb32 | DeviceFlags.xrgb32,
        .mode = .{ .width = 8, .height = 8, .pitch = 32, .bpp = 32 },
        .framebuffer = &frame,
    });
    defer {
        fixtureSet(4, 0);
        fixtureSet(5, 0);
        fixtureSet(14, 0);
        fixtureSet(15, 0);
        fixtureSetManager(.{});
        fixtureSet(12, 0);
        fixtureSet(13, 0);
        fixtureSet(16, 0);
    }

    const descriptor = blit_backend.Descriptor{
        .flags = blit_backend.REQUIRED_FLAGS | blit_backend.FLAG_CPU_FAST_COPY,
        .max_regions = MAX_PRESENT_REGIONS,
        .present = FailBackend.present,
    };
    try testing.expectEqual(@as(i32, 0), blit_backend.register(91, "FAILBLIT", &descriptor));
    defer _ = blit_backend.unregister(91, "FAILBLIT");

    var source: [64]u32 align(32) = undefined;
    for (&source, 0..) |*pixel, index| pixel.* = 0x0010_0000 | @as(u32, @intCast(index));
    const regions = [_]PresentRegion{
        .{ .dst_x = 1, .dst_y = 1, .src_x = 1, .src_y = 1, .w = 2, .h = 2 },
        .{ .dst_x = 5, .dst_y = 5, .src_x = 5, .src_y = 5, .w = 2, .h = 2 },
    };
    const outcome = presentXrgb32Regions(source[0..].ptr, source.len, 8, regions[0..], 44, 0, false);
    try testing.expect(outcome.success);
    try testing.expect(outcome.fallback);
    try testing.expect(!outcome.accelerated);
    try testing.expectEqual(@as(i32, -77), outcome.backend_error);
    try testing.expectEqual(@as(u32, 2), outcome.region_count);
    try testing.expectEqual(@as(u32, 8), outcome.pixel_count);
    try testing.expectEqual(outcome.fence, outcome.completed_fence);
    try testing.expect(presentFenceCompleted(outcome.fence));

    var y: usize = 0;
    while (y < 8) : (y += 1) {
        var x: usize = 0;
        while (x < 8) : (x += 1) {
            const damaged = (x >= 1 and x < 3 and y >= 1 and y < 3) or
                (x >= 5 and x < 7 and y >= 5 and y < 7);
            try testing.expectEqual(if (damaged) source[y * 8 + x] else 0xA5A5_A5A5, target[y * 8 + x]);
        }
    }
}

