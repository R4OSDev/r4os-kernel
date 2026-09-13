const fb = @import("framebuffer.zig");
const ownership = @import("ownership.zig");
const font = @import("../kernel/font.zig");
const blit_backend = @import("blit_backend.zig");
const cpu = @import("../platform/cpu.zig");
const paging = @import("../memory/paging.zig");
const timer = @import("../kernel/timer.zig");
pub const backend_state = @import("backend_state.zig");
const firmware_access = @import("firmware_access.zig");

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

pub const Mode = struct {
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

pub const Rect = struct {
    x: u32 = 0,
    y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
};

pub const MAX_PRESENT_REGIONS: usize = 8;
pub const PRESENT_FORMAT_XRGB32: u32 = 1;

pub const PresentRegion = blit_backend.Region;

pub const PresentOutcome = struct {
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

pub const PresentCapabilities = struct {
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

pub const Mapping = struct {
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

pub const DeviceOps = struct {
    fill: ?*const fn (device: *Device, rgb: u32) bool = null,
    rect: ?*const fn (device: *Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) bool = null,
    present_packed32_rect: ?*const fn (device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool = null,
    present_xrgb32_rect: ?*const fn (device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool = null,
    put_packed32: ?*const fn (device: *Device, x: u64, y: u64, color32: u32) bool = null,
    put_xrgb32: ?*const fn (device: *Device, x: u64, y: u64, rgb: u32) bool = null,
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

pub const Device = struct {
    name: []const u8 = "none",
    kind: DeviceKind = .none,
    flags: u32 = 0,
    mode: Mode = .{},
    mapping: Mapping = .{},
    framebuffer: ?*fb.Framebuffer = null,
    ops: *const DeviceOps = &empty_ops,
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

const empty_ops: DeviceOps = .{};
const bootfb_ops: DeviceOps = .{
    .fill = bootfbFill,
    .rect = bootfbRect,
    .present_packed32_rect = bootfbPresentPacked32Rect,
    .present_xrgb32_rect = bootfbPresentXrgb32Rect,
    .put_packed32 = bootfbPutPacked32,
    .put_xrgb32 = bootfbPutXrgb32,
};

var bootfb_device: Device = .{ .ops = &bootfb_ops };
var primary_device: ?*Device = null;
var present_generation: u64 = 0;
var completed_fence: u64 = 0;
var execution = ownership.Execution.init("display-present");
var system_transition: bool = false;
pub fn beginOutputCommit() bool {
    if (!execution.tryEnter()) return false;
    if (system_transition) {
        execution.leave();
        return false;
    }
    return true;
}
pub fn endOutputCommit() void { execution.leave(); }

// Drain presentation before the R4D owner is entered (callbacks acquire that
// owner in the opposite direction). Teardown may still restore/release the
// retained display, but cannot reopen normal output during a system reset.
pub fn beginSystemTransition(timeout_ticks: u64) bool {
    if (!execution.enter(timeout_ticks)) return false;
    defer execution.leave();
    if (system_transition) return true;
    if (!firmware_access.gate.isRevoked() and !firmware_access.gate.tryRevoke()) return false;
    system_transition = true;
    primary_device = null;
    publishStats();
    return true;
}

pub fn systemTransitionQuiesced() bool {
    if (!execution.tryEnter()) return false;
    defer execution.leave();
    return system_transition and primary_device == null and held_boot == null and native_backend == null and
        backend_manager.value.owner == 0 and backend_manager.value.pending_owner == 0;
}
var completed_stats: Stats = .{};
var presentation_statistics: @import("presentation_stats.zig").Owner = .{};
var backend_manager: backend_state.Manager = .{};
var completed_backend_state: backend_state.Snapshot = .{};
var completed_boot_mode: Mode = .{};
var completed_boot_mapping: Mapping = .{};
var completed_firmware_writable: bool = true;
var completed_output_name: [blit_backend.NAME_BYTES]u8 = .{0} ** blit_backend.NAME_BYTES;

pub const BackendView = struct {
    state: backend_state.Snapshot,
    device: Stats,
    boot_mode: Mode,
    boot_mapping: Mapping,
    firmware_writable: bool,
    name: [blit_backend.NAME_BYTES]u8,
};

pub fn backendView() BackendView {
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    return .{ .state = completed_backend_state, .device = completed_stats, .boot_mode = completed_boot_mode, .boot_mapping = completed_boot_mapping, .firmware_writable = completed_firmware_writable, .name = completed_output_name };
}

pub const BootSnapshot = struct {
    mode: Mode,
    mapping: Mapping,
    framebuffer: fb.Framebuffer,
};

pub const CommitResult = enum { confirmed, old_preserved, output_lost };
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
    // normal presentation succeeds; false triggers the same proven recovery.
    begin_cpu: ?*const fn (usize) bool = null,
    end_cpu: ?*const fn (usize, bool, ?Rect) bool = null,
};

pub const TransitionError = backend_state.Error || error{ Unavailable, RestoreFailed };
var native_backend: ?NativeBackend = null;
var native_device: Device = .{};
var saved_boot: ?BootSnapshot = null;
var native_hold_generation: u64 = 0;
var native_restore_confirmed = false;

// Firmware bringup precedes output/queue discovery. This is a hold of the
// existing boot display, not a fabricated native output or completion queue.
pub const BootHolder = struct {
    owner: usize,
    adapter_id: u32,
    expected_generation: u64,
    context: usize,
    capture: *const fn (usize, *const BootSnapshot) bool,
    // True proves hardware quiescence and the original CPU scanout mapping,
    // and restores its saved pixels. A timeout/INIT_DONE is not this proof.
    restore: *const fn (usize, u64, *const BootSnapshot) bool,
    release: *const fn (usize) bool,
    // Native recovery enters from the display owner, outside the original
    // hold/finish caller. Its bridge may need separate lifetime admission.
    release_adopted: ?*const fn (usize) bool = null,
};
const HeldBoot = struct { holder: BootHolder, captured: bool = false, effects: bool = false, restored: bool = false };
var held_boot: ?HeldBoot = null;
pub const BootHoldResult = struct { generation: u64, captured: bool, retained: bool };

pub fn holdBoot(holder: BootHolder) TransitionError!BootHoldResult {
    if (!beginOutputCommit()) return error.Busy;
    defer execution.leave();
    const boot = &(saved_boot orelse return error.Unavailable);
    if (held_boot != null or native_backend != null) return error.Busy;
    if (holder.expected_generation == 0 or holder.expected_generation != backend_manager.value.generation) return error.Stale;
    const generation = try backend_manager.begin(holder.owner, holder.adapter_id);
    if (!firmware_access.gate.tryRevoke()) {
        backend_manager.abort(holder.owner, generation, .prepare_failed) catch unreachable;
        publishStats();
        return error.Busy;
    }
    held_boot = .{ .holder = holder };
    primary_device = null;
    publishStats();
    if (holder.capture(holder.context, boot)) {
        held_boot.?.captured = true;
        return .{ .generation = generation, .captured = true, .retained = true };
    }
    // Capture cannot touch the GPU. Even a partial CPU allocation/lease has
    // an exact owner, and failed cleanup can be retried through finishBoot.
    const released = holder.release(holder.context);
    if (released) releaseBootLocked(holder.owner, generation, .prepare_failed);
    return .{ .generation = generation, .captured = false, .retained = !released };
}

// 0: abandon before any possible device effect; 1: latch BEFORE the first
// possible effect; 2: recover using the retained hardware owner. No automatic
// reopen/unload is allowed after operation 1, including callback/time errors.
pub fn finishBoot(owner: usize, generation: u64, operation: u32) TransitionError!bool {
    if (!execution.tryEnter()) return error.Busy;
    defer execution.leave();
    if (native_backend != null) return error.Busy;
    try backend_manager.checkPending(owner, generation);
    const hold = if (held_boot) |*value| value else return error.Stale;
    if (operation > 2) return error.Invalid;
    if (operation == 1) {
        if (system_transition) return error.Busy;
        if (!hold.captured) return error.Invalid;
        if (hold.effects) return error.Busy;
        hold.effects = true;
        return false;
    }
    if (hold.effects and operation != 2) return error.Busy;
    if (hold.effects and !hold.restored) {
        const boot = &(saved_boot orelse return error.Unavailable);
        if (!hold.holder.restore(hold.holder.context, generation, boot)) return error.RestoreFailed;
        hold.restored = true;
    }
    if (!hold.holder.release(hold.holder.context)) return error.RestoreFailed;
    releaseBootLocked(owner, generation, .none);
    return true;
}

fn releaseBootLocked(owner: usize, generation: u64, reason: backend_state.Reason) void {
    backend_manager.abort(owner, generation, reason) catch unreachable;
    held_boot = null;
    if (!system_transition) {
        primary_device = &bootfb_device;
        firmware_access.gate.restore();
    }
    publishStats();
}

pub fn bootSnapshot() ?BootSnapshot {
    const token = ownership.enterState(); defer ownership.leaveState(token);
    return saved_boot;
}

const CpuWrite = struct {
    callback: ?*const fn (usize, bool, ?Rect) bool = null,
    context: usize = 0,
    active: bool = true,
    damage: ?Rect = null,
    fn finish(self: *CpuWrite, changed: bool) bool {
        if (!self.active) return true;
        self.active = false;
        const callback = self.callback orelse return true;
        if (callback(self.context, changed, self.damage)) return true;
        if (native_backend) |backend| restoreBootLocked(backend.owner, backend_manager.value.generation) catch {};
        return false;
    }
};
fn beginCpuWrite(device: *const Device) ?CpuWrite {
    if (device.kind != .native) return .{};
    const backend = native_backend orelse return null;
    if (backend.begin_cpu) |begin| {
        if (backend.end_cpu == null or !begin(backend.context)) return null;
        return .{ .callback = backend.end_cpu, .context = backend.context };
    }
    return if (backend.end_cpu == null) CpuWrite{} else null;
}

pub fn backendState() backend_state.Snapshot {
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    return completed_backend_state;
}

pub fn retainsDriverOwner(owner: usize) bool {
    const current = backendState();
    return owner != 0 and (current.owner == owner or current.pending_owner == owner);
}

pub fn setBackendPolicy(policy: backend_state.Policy) bool {
    if (!beginOutputCommit()) return false;
    defer execution.leave();
    backend_manager.setPolicy(policy) catch return false;
    publishStats();
    return true;
}

pub fn rejectNativeBackend() void {
    if (!execution.tryEnter()) return;
    defer execution.leave();
    backend_manager.reject(.backend_rejected);
    publishStats();
}

// Kernel-side seam for the native R4D adapter. Preparation only publishes a
// retained candidate; no callback may alter hardware before commitNative.
// This first seam accepts CPU-addressable native scanout. GPU BO/submit and
// virtual shadow upload capabilities are added by their subsequent owners.
// Geometry and pixel layout remain fixed until the atomic output contract can
// also rebuild the surface pipeline and notify all desktop consumers.
pub fn prepareNative(backend: NativeBackend) TransitionError!u64 {
    return prepareNativeImpl(backend, 0);
}

pub fn prepareHeldNative(backend: NativeBackend, held_generation: u64) TransitionError!u64 {
    if (held_generation == 0) return error.Stale;
    return prepareNativeImpl(backend, held_generation);
}

fn prepareNativeImpl(backend: NativeBackend, held_generation: u64) TransitionError!u64 {
    if (!beginOutputCommit()) return error.Busy;
    defer execution.leave();
    const boot = saved_boot orelse return error.Unavailable;
    const target = backend.target;
    const frame = target.framebuffer orelse return error.Invalid;
    if ((backend.begin_cpu == null) != (backend.end_cpu == null)) return error.Invalid;
    if (target.kind != .native or target.mapping.kind != .native_scanout or
        target.name.len == 0 or target.name.len >= blit_backend.NAME_BYTES or
        target.mode.width != boot.mode.width or target.mode.height != boot.mode.height or
        target.mode.bpp != boot.mode.bpp or
        target.mode.red_mask_size != boot.mode.red_mask_size or target.mode.red_mask_shift != boot.mode.red_mask_shift or
        target.mode.green_mask_size != boot.mode.green_mask_size or target.mode.green_mask_shift != boot.mode.green_mask_shift or
        target.mode.blue_mask_size != boot.mode.blue_mask_size or target.mode.blue_mask_shift != boot.mode.blue_mask_shift or
        !fb.supportsRgb32(frame) or frame.width == 0 or frame.height == 0 or
        frame.width != target.mode.width or frame.height != target.mode.height or
        frame.pitch != target.mode.pitch or target.mode.bpp != 32 or
        frame.red_mask_size != target.mode.red_mask_size or frame.red_mask_shift != target.mode.red_mask_shift or
        frame.green_mask_size != target.mode.green_mask_size or frame.green_mask_shift != target.mode.green_mask_shift or
        frame.blue_mask_size != target.mode.blue_mask_size or frame.blue_mask_shift != target.mode.blue_mask_shift or
        (frame.pitch & 3) != 0 or (@intFromPtr(frame.address) & 3) != 0 or
        target.mapping.virt_base != @intFromPtr(frame.address) or
        frame.height > ~@as(u64, 0) / frame.pitch or
        target.mapping.byte_len < frame.pitch * frame.height or
        target.mapping.virt_base > ~@as(u64, 0) - target.mapping.byte_len or
        (target.flags & DeviceFlags.cpu_present) == 0) return error.Invalid;
    if (native_backend != null) return error.Busy;
    const generation = if (held_generation != 0) blk: {
        try backend_manager.checkPending(backend.owner, held_generation);
        const hold = held_boot orelse return error.Stale;
        if (hold.holder.owner != backend.owner or hold.holder.adapter_id != backend.adapter_id) return error.Stale;
        if (!hold.captured or !hold.effects or hold.restored) return error.Invalid;
        // The GPU already has effects. Keep both the pending generation and
        // the closed writer gate; there is no intervening bootfb admission.
        break :blk held_generation;
    } else try backend_manager.begin(backend.owner, backend.adapter_id);
    native_backend = backend;
    native_hold_generation = held_generation;
    native_restore_confirmed = false;
    native_device = .{
        .name = "native",
        .kind = .native,
        .flags = target.flags,
        .mode = target.mode,
        .mapping = target.mapping,
        .framebuffer = frame,
        .ops = &bootfb_ops,
    };
    publishStats();
    return generation;
}

pub fn abortNative(owner: usize, generation: u64) TransitionError!void {
    if (!execution.tryEnter()) return error.Busy;
    defer execution.leave();
    if (held_boot != null and native_hold_generation == 0) return error.Busy;
    if (native_hold_generation != 0) {
        try backend_manager.checkPending(owner, generation);
    } else try backend_manager.abort(owner, generation, .prepare_failed);
    native_backend = null;
    native_device = .{};
    native_hold_generation = 0;
    native_restore_confirmed = false;
    publishStats();
}

pub fn commitNative(owner: usize, generation: u64) TransitionError!CommitResult {
    if (!beginOutputCommit()) return error.Busy;
    defer execution.leave();
    if (held_boot != null and native_hold_generation == 0) return error.Busy;
    try backend_manager.checkPending(owner, generation);
    const backend = native_backend orelse return error.Unavailable;
    const boot = &(saved_boot orelse return error.Unavailable);
    // The present guard excludes normal writers. Legacy console/fatal writers
    // have their own nonblocking admission: never overtake their WC stores.
    if (native_hold_generation == 0 and !firmware_access.gate.tryRevoke()) return error.Busy;
    publishStats();
    const result = backend.commit(backend.context, generation, boot);
    switch (result) {
        .confirmed => {
            backend_manager.commit(owner, generation, true) catch unreachable;
            primary_device = &native_device;
        },
        .old_preserved => {
            if (native_hold_generation == 0) {
                backend_manager.abort(owner, generation, .commit_failed) catch unreachable;
                firmware_access.gate.restore();
            }
            native_backend = null;
            native_device = .{};
            native_hold_generation = 0;
            native_restore_confirmed = false;
        },
        .output_lost => {
            backend_manager.failCommit(owner, generation) catch unreachable;
            primary_device = null;
        },
    }
    publishStats();
    return result;
}

pub fn restoreBootBackend(owner: usize, generation: u64) TransitionError!void {
    if (!execution.tryEnter()) return error.Busy;
    defer execution.leave();
    return restoreBootLocked(owner, generation);
}

// Caller holds DisplayExecution across the real device transaction. Neither
// routine changes the immutable boot snapshot or calls a driver callback.
pub fn replaceNativeFrameLocked(owner: usize, generation: u64, frame: *fb.Framebuffer, bytes: u64) TransitionError!void {
    const backend = if (native_backend) |*value| value else return error.Unavailable;
    try backend_manager.checkActive(owner, generation);
    if (backend.owner != owner or system_transition or !fb.isNativeXrgb32(frame) or frame.width == 0 or frame.height == 0 or
        frame.width > 65536 or frame.height > 65536 or frame.pitch > ~@as(u32, 0) or frame.pitch < frame.width * 4 or
        frame.pitch & 3 != 0 or @intFromPtr(frame.address) & 3 != 0 or bytes != frame.pitch * frame.height or
        @intFromPtr(frame.address) > ~@as(u64, 0) - bytes) return error.Invalid;
    try backend_manager.modeResult(owner, generation, true);
    native_device.mode.width = @intCast(frame.width);
    native_device.mode.height = @intCast(frame.height);
    native_device.mode.pitch = @intCast(frame.pitch);
    native_device.mapping = .{ .kind = .native_scanout, .virt_base = @intFromPtr(frame.address), .byte_len = bytes };
    native_device.framebuffer = frame;
    native_device.flags &= ~DeviceFlags.fixed_mode;
    backend.target.mode = native_device.mode;
    backend.target.mapping = native_device.mapping;
    backend.target.framebuffer = frame;
    backend.target.flags = native_device.flags;
    primary_device = &native_device;
    publishStats();
}
pub fn loseNativeModeLocked(owner: usize, generation: u64) TransitionError!void {
    try backend_manager.modeResult(owner, generation, false);
    primary_device = null;
    publishStats();
}
fn restoreBootLocked(owner: usize, generation: u64) TransitionError!void {
    if (@import("builtin").os.tag == .freestanding and @import("native_driver.zig").modePending()) return error.Busy;
    const backend = native_backend orelse return error.Unavailable;
    const boot = &(saved_boot orelse return error.Unavailable);
    const recovery_generation = try backend_manager.beginRecovery(owner, generation);
    primary_device = null;
    publishStats();
    // The callback runs under the wait-spanning execution guard, never under
    // the program-state owner. A failed restore retains all driver resources.
    if (!native_restore_confirmed and !backend.restore(backend.context, recovery_generation, boot)) {
        backend_manager.recoveryFailed(owner, recovery_generation) catch unreachable;
        publishStats();
        return error.RestoreFailed;
    }
    native_restore_confirmed = true;
    if (held_boot) |hold| {
        const release = hold.holder.release_adopted orelse hold.holder.release;
        if (!release(hold.holder.context)) {
            backend_manager.recoveryFailed(owner, recovery_generation) catch unreachable;
            publishStats();
            return error.RestoreFailed;
        }
        held_boot = null;
        native_hold_generation = 0;
    }
    backend_manager.restoreBoot(owner, recovery_generation) catch |err| {
        backend_manager.recoveryFailed(owner, recovery_generation) catch unreachable;
        publishStats();
        return err;
    };
    if (!system_transition) {
        primary_device = &bootfb_device;
        firmware_access.gate.restore();
    }
    native_backend = null;
    native_device = .{};
    native_restore_confirmed = false;
    publishStats();
}

pub fn registerBootBackend(target: DisplayTarget) void {
    if (!beginOutputCommit()) return;
    defer execution.leave();
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    // Boot registration cannot be used to bypass native takeover/recovery.
    if (backend_manager.value.generation != 0) return;
    bootfb_device = .{
        .name = target.name,
        .kind = target.kind,
        .flags = target.flags,
        .mode = target.mode,
        .mapping = target.mapping,
        .framebuffer = target.framebuffer,
        .ops = &bootfb_ops,
    };
    primary_device = &bootfb_device;
    present_generation = 0;
    completed_fence = 0;
    backend_manager.initBoot();
    if (target.framebuffer) |frame| saved_boot = .{ .mode = target.mode, .mapping = target.mapping, .framebuffer = frame.* };
    completed_backend_state = backend_manager.value;
    completed_stats = captureStats();
    publishGeometryLocked(completed_stats);
    completed_boot_mode = target.mode;
    completed_boot_mapping = target.mapping;
    copyName(&completed_output_name, target.name);
}

pub fn activeBackendRegistered() bool {
    return stats().registered;
}
pub fn activeBackendName() []const u8 {
    return stats().name;
}
pub fn activeBackendKind() DeviceKind {
    return stats().kind;
}
pub fn activeMode() ?Mode {
    const current = stats();
    return if (current.registered) current.mode else null;
}
pub fn activeMapping() ?Mapping {
    const current = stats();
    return if (current.registered) current.mapping else null;
}

pub fn enableFramebufferWriteCombining() bool {
    if (!execution.tryEnter()) return false;
    defer execution.leave();
    defer publishStats();
    const device = primary_device orelse return false;
    // Native system BO cache policy belongs to the common memory owner.
    if (device.kind != .bootfb) return false;
    if (!cpu.writeCombiningBasisAvailable()) {
        device.mapping.cache_policy = .write_combining_unsupported;
        return false;
    }
    if (device.mapping.virt_base == 0 or device.mapping.byte_len == 0) {
        device.mapping.cache_policy = .write_combining_failed;
        return false;
    }
    if (!paging.setWriteCombiningRange(device.mapping.virt_base, device.mapping.byte_len)) {
        device.mapping.cache_policy = .write_combining_failed;
        return false;
    }
    device.mapping.cache_policy = .pat_write_combining;
    if (device.kind == .bootfb) {
        if (saved_boot) |*snapshot| snapshot.mapping = device.mapping;
    }
    return true;
}

pub fn stats() Stats {
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    return completed_stats;
}

pub fn bindPresentationStats(owner: usize, owner_generation: u64, binding: @import("r4os_kernel_contract").GfxBackendBinding,
    generation: u64) @import("presentation_stats.zig").Error!void
{
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    if (completed_backend_state.pending_owner != owner or completed_backend_state.pending_generation != generation or
        completed_backend_state.pending_adapter_id != binding.adapter_id) return error.Stale;
    try presentation_statistics.bind(owner, owner_generation, binding, generation);
}
pub fn publishPresentationStats(owner: usize, owner_generation: u64, value: @import("r4os_kernel_contract").DisplayPresentationStats)
    @import("presentation_stats.zig").Error!void
{
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    try presentation_statistics.publish(owner, owner_generation, value, completed_backend_state);
}
pub fn presentationStats(head: u32) @import("presentation_stats.zig").Error!@import("r4os_kernel_contract").DisplayPresentationStats {
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    return presentation_statistics.read(head, completed_backend_state);
}

fn publishStats() void {
    asm volatile ("sfence" ::: .{ .memory = true });
    const value = captureStats();
    const token = ownership.enterState();
    completed_stats = value;
    publishGeometryLocked(value);
    completed_backend_state = backend_manager.value;
    if (saved_boot) |boot| {
        completed_boot_mode = boot.mode;
        completed_boot_mapping = boot.mapping;
    }
    completed_firmware_writable = !firmware_access.gate.isRevoked();
    const output_name = if (backend_manager.value.owner != 0)
        (if (native_backend) |backend| backend.target.name else "none")
    else
        value.name;
    copyName(&completed_output_name, output_name);
    ownership.leaveState(token);
    if (@import("builtin").os.tag == .freestanding)
        @import("outputs.zig").bootActive(value.registered and value.kind == .bootfb);
}

fn publishGeometryLocked(value: Stats) void {
    @import("surface_pipeline.zig").publishTargetLocked(if (value.registered) .{
        .width = value.mode.width, .height = value.mode.height,
        .pitch = value.mode.pitch, .bpp = value.mode.bpp,
    } else .{});
}

fn captureStats() Stats {
    const device = primary_device orelse return .{};
    return .{
        .registered = true,
        .name = device.name,
        .kind = device.kind,
        .flags = device.flags,
        .mode = device.mode,
        .mapping = device.mapping,
        .present_count = device.present_count,
        .present_pixels_total = device.present_pixels_total,
        .present_bytes_total = device.present_bytes_total,
        .last_present_pixels = device.last_present_pixels,
        .last_present_bytes = device.last_present_bytes,
        .last_present_rect = device.last_present_rect,
        .last_present_reason = device.last_present_reason,
        .last_present_converted = device.last_present_converted,
        .full_present_count = device.full_present_count,
        .partial_present_count = device.partial_present_count,
        .fill_present_count = device.fill_present_count,
        .rect_present_count = device.rect_present_count,
        .packed32_present_count = device.packed32_present_count,
        .xrgb32_present_count = device.xrgb32_present_count,
        .conversion_present_count = device.conversion_present_count,
        .present_total_ticks = device.present_total_ticks,
        .present_max_ticks = device.present_max_ticks,
        .present_last_ticks = device.present_last_ticks,
        .present_slow_count = device.present_slow_count,
    };
}

pub fn fill(rgb: u32) bool {
    if (!execution.tryEnter()) return false;
    defer execution.leave();
    const device = primary_device orelse return false;
    const op = device.ops.fill orelse return false;
    var write = beginCpuWrite(device) orelse return false;
    defer _ = write.finish(false);
    const start = timer.tickCount();
    const ok = op(device, rgb);
    if (ok) write.damage = device.last_present_rect;
    if (!write.finish(ok)) return false;
    if (ok) {
        recordPresentTiming(device, start);
        publishStats();
    }
    return ok;
}

pub fn rect(x: i32, y: i32, w: u32, h: u32, rgb: u32) bool {
    if (!execution.tryEnter()) return false;
    defer execution.leave();
    const device = primary_device orelse return false;
    const op = device.ops.rect orelse return false;
    var write = beginCpuWrite(device) orelse return false;
    defer _ = write.finish(false);
    const start = timer.tickCount();
    const ok = op(device, x, y, w, h, rgb);
    if (ok) write.damage = device.last_present_rect;
    if (!write.finish(ok)) return false;
    if (ok) {
        recordPresentTiming(device, start);
        publishStats();
    }
    return ok;
}

pub fn textZ(font_id: ?u32, x: i32, y: i32, value: [*:0]const u8, fg: u32, bg: u32) bool {
    if (@intFromPtr(value) == 0 or !execution.tryEnter()) return false;
    defer execution.leave();
    const device = primary_device orelse return false;
    const f = device.framebuffer orelse return false;
    if (!fb.supportsRgb32(f)) return false;
    var write = beginCpuWrite(device) orelse return false;
    defer _ = write.finish(false);
    var length: usize = 0;
    while (length < 4096 and value[length] != 0) : (length += 1) {}
    var catalog = font.acquireCatalog();
    defer catalog.release();
    const state = catalog.state();
    const selected = state.normalizeFontId(font_id orelse state.currentFontId());
    const line_height = state.glyphHeightForFont(selected);
    const packed_fg = fb.packRgb(f, fg);
    const packed_bg = fb.packRgb(f, bg);
    var pen_x: i64 = x;
    var pen_y: i64 = y;
    var offset: usize = 0;
    var pixels: u64 = 0;
    var bounds = Rect{};
    const start = timer.tickCount();
    while (offset < length) {
        const scalar = font.decodeUtf8Scalar(value[0..length], offset);
        offset += scalar.consumed;
        if (scalar.codepoint == '\r') continue;
        if (scalar.codepoint == '\n') {
            pen_x = x;
            pen_y += line_height;
            continue;
        }
        const glyph = state.glyphBitmapForFont(selected, scalar.codepoint);
        if (clipSignedRect(pen_x, pen_y, @max(glyph.width, glyph.advance), glyph.line_height, device.mode)) |clipped| {
            bounds = if (pixels == 0) clipped else mergeRect(bounds, clipped);
            pixels += @as(u64, clipped.w) * clipped.h;
            for (0..clipped.h) |row| {
                const gy: usize = @intCast(@as(i64, clipped.y) + @as(i64, @intCast(row)) - pen_y);
                for (0..clipped.w) |column| {
                    const gx: usize = @intCast(@as(i64, clipped.x) + @as(i64, @intCast(column)) - pen_x);
                    const ink = gy < glyph.height and gy < glyph.rows.len and gx < glyph.width and gx < 64 and
                        (glyph.rows[gy] & (@as(u64, 1) << @intCast(gx))) != 0;
                    fb.putPacked32(f, clipped.x + column, clipped.y + row, if (ink) packed_fg else packed_bg);
                }
            }
        }
        pen_x += glyph.advance;
    }
    if (pixels == 0) return false;
    write.damage = bounds;
    if (!write.finish(true)) return false;
    recordPresentAggregate(device, .rect, bounds, pixels, false);
    recordPresentTiming(device, start);
    publishStats();
    return true;
}

pub fn putPacked32(x: u64, y: u64, color32: u32) bool {
    if (!execution.tryEnter()) return false;
    defer execution.leave();
    const device = primary_device orelse return false;
    const op = device.ops.put_packed32 orelse return false;
    var write = beginCpuWrite(device) orelse return false;
    const ok = op(device, x, y, color32);
    if (ok) write.damage = .{ .x = @intCast(x), .y = @intCast(y), .w = 1, .h = 1 };
    return write.finish(ok) and ok;
}

pub fn putXrgb32(x: u64, y: u64, rgb: u32) bool {
    if (!execution.tryEnter()) return false;
    defer execution.leave();
    const device = primary_device orelse return false;
    const op = device.ops.put_xrgb32 orelse return false;
    var write = beginCpuWrite(device) orelse return false;
    const ok = op(device, x, y, rgb);
    if (ok) write.damage = .{ .x = @intCast(x), .y = @intCast(y), .w = 1, .h = 1 };
    return write.finish(ok) and ok;
}

pub fn presentPacked32Rect(x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    if (!execution.tryEnter()) return false;
    defer execution.leave();
    const device = primary_device orelse return false;
    const op = device.ops.present_packed32_rect orelse return false;
    var write = beginCpuWrite(device) orelse return false;
    defer _ = write.finish(false);
    const start = timer.tickCount();
    const ok = op(device, x0, y0, w, h, src, src_stride_pixels);
    if (ok) write.damage = device.last_present_rect;
    if (!write.finish(ok)) return false;
    if (ok) {
        recordPresentTiming(device, start);
        publishStats();
    }
    return ok;
}

pub fn presentXrgb32Rect(x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    if (x0 > ~@as(u32, 0) or y0 > ~@as(u32, 0) or w > ~@as(u32, 0) or h > ~@as(u32, 0) or
        src_stride_pixels > ~@as(u32, 0) or (src.len & 3) != 0 or (@intFromPtr(src.ptr) & 3) != 0)
    {
        return false;
    }
    const pixels: [*]const u32 = @ptrCast(@alignCast(src.ptr));
    const region = PresentRegion{
        .dst_x = @intCast(x0),
        .dst_y = @intCast(y0),
        .src_x = 0,
        .src_y = 0,
        .w = @intCast(w),
        .h = @intCast(h),
    };
    return presentXrgb32Regions(
        pixels,
        @intCast(src.len / @sizeOf(u32)),
        @intCast(src_stride_pixels),
        (&region)[0..1],
        0,
        0,
        false,
    ).success;
}

/// The sole productive XRGB32 present/statistics path. All regions are
/// validated before the first visible write. The external backend is one
/// synchronous optimization attempt; every absence, incompatibility or
/// callback error falls back to the current owner's CPU scanout copy for the
/// complete generation. A retired firmware framebuffer is never consulted.
pub fn presentXrgb32Regions(
    source: [*]const u32,
    source_pixel_count: u32,
    source_stride_pixels: u32,
    regions: []const PresentRegion,
    source_generation: u64,
    input_tick: u64,
    input_tick_valid: bool,
) PresentOutcome {
    var outcome = PresentOutcome{ .source_generation = source_generation };
    if (!execution.tryEnter()) return outcome;
    defer execution.leave();
    const device = primary_device orelse return outcome;
    const f = device.framebuffer orelse return outcome;
    if (!fb.supportsRgb32(f) or source_pixel_count == 0 or source_stride_pixels == 0 or
        regions.len == 0 or regions.len > MAX_PRESENT_REGIONS)
    {
        return outcome;
    }

    var pixels_total: u64 = 0;
    var bounds = Rect{};
    for (regions, 0..) |region, index| {
        if (!validPresentRegion(region, device.mode, source_pixel_count, source_stride_pixels)) return outcome;
        const region_pixels = @as(u64, region.w) * region.h;
        if (pixels_total > ~@as(u32, 0) - region_pixels) return outcome;
        pixels_total += region_pixels;
        if (index == 0) {
            bounds = .{ .x = region.dst_x, .y = region.dst_y, .w = region.w, .h = region.h };
        } else {
            bounds = mergeRect(bounds, .{ .x = region.dst_x, .y = region.dst_y, .w = region.w, .h = region.h });
        }
    }

    var write = beginCpuWrite(device) orelse return outcome;
    defer _ = write.finish(false);
    const start = timer.tickCount();
    var external = blit_backend.InvokeResult{};
    if (fb.isNativeXrgb32(f) and (f.pitch & 3) == 0) {
        const job = blit_backend.Job{
            .target_address = @intFromPtr(f.address),
            .target_width = @intCast(f.width),
            .target_height = @intCast(f.height),
            .target_pitch_pixels = @intCast(f.pitch / @sizeOf(u32)),
            .source_pixel_count = source_pixel_count,
            .source_address = @intFromPtr(source),
            .source_stride_pixels = source_stride_pixels,
            .region_count = @intCast(regions.len),
            .regions_address = @intFromPtr(regions.ptr),
        };
        external = blit_backend.invoke(&job);
    }

    if (external.attempted and external.result == 0) {
        outcome.accelerated = true;
        outcome.backend_name = external.name;
    } else {
        const source_bytes = @as([*]const u8, @ptrCast(source))[0 .. @as(usize, source_pixel_count) * @sizeOf(u32)];
        for (regions) |region| {
            if (!bootfbCopyXrgb32Region(device, region, source_bytes, source_stride_pixels)) return outcome;
        }
        outcome.fallback = true;
        outcome.fallback_regions = @intCast(regions.len);
        outcome.backend_error = if (external.attempted) external.result else 0;
        copyName(outcome.backend_name[0..], if (device.kind == .bootfb) "bootfb-cpu" else "native-cpu");
    }

    write.damage = bounds;
    if (!write.finish(true)) return outcome;
    const completed_tick = timer.tickCount();
    recordPresentAggregate(device, .xrgb32_present, bounds, pixels_total, false);
    recordPresentTimingAt(device, start, completed_tick);
    // Pixel stores precede the fence, including write-combining fallback.
    asm volatile ("sfence" ::: .{ .memory = true });
    const token = ownership.enterState();
    present_generation +%= 1;
    if (present_generation == 0) present_generation = 1;
    completed_fence = present_generation;
    completed_stats = captureStats();
    ownership.leaveState(token);
    outcome.success = true;
    outcome.present_generation = present_generation;
    outcome.fence = present_generation;
    outcome.completed_fence = completed_fence;
    outcome.region_count = @intCast(regions.len);
    outcome.pixel_count = @intCast(pixels_total);
    outcome.present_tick = completed_tick;
    outcome.elapsed_ticks = if (input_tick_valid and completed_tick >= input_tick) completed_tick - input_tick else 0;
    return outcome;
}

pub fn presentCapabilities() PresentCapabilities {
    const current = stats();
    if (!current.registered) return .{ .flags = 0, .formats = 0, .max_regions = 0, .backend_kind = 0 };
    var result = PresentCapabilities{
        .flags = 1 | 2 | 4 | 32, // Ordered stores; never a VBlank claim.
        .backend_kind = if (current.kind == .bootfb) 1 else 3,
    };
    const cpu_name: []const u8 = if (current.kind == .bootfb) "bootfb-cpu" else "native-cpu";
    copyName(result.backend_name[0..], cpu_name);
    copyName(result.fallback_name[0..], cpu_name);
    const external = blit_backend.snapshot();
    const m = current.mode;
    const target_compatible = current.registered and m.bpp == 32 and m.red_mask_size == 8 and m.red_mask_shift == 16 and
        m.green_mask_size == 8 and m.green_mask_shift == 8 and m.blue_mask_size == 8 and m.blue_mask_shift == 0 and (m.pitch & 3) == 0;
    if (external.active and target_compatible) {
        result.flags |= 8 | 16;
        result.backend_kind = 2;
        result.backend_name = external.name;
        result.max_regions = @intCast(@min(@as(usize, external.max_regions), MAX_PRESENT_REGIONS));
    }
    return result;
}

pub fn presentFenceCompleted(fence: u64) bool {
    return fence != 0 and fence <= highestCompletedFence();
}

pub fn highestCompletedFence() u64 {
    const token = ownership.enterState();
    defer ownership.leaveState(token);
    return completed_fence;
}

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
        cpu_frames: u32 = 0,
        fn beginCpu(raw: usize) bool {
            const self: *@This() = @ptrFromInt(raw);
            t.expect(!self.cpu_active) catch unreachable;
            self.cpu_active = true;
            return true;
        }
        fn endCpu(raw: usize, changed: bool, damage: ?Rect) bool {
            const self: *@This() = @ptrFromInt(raw);
            t.expect(self.cpu_active) catch unreachable;
            self.cpu_active = false;
            if (changed) self.cpu_frames += 1;
            if (changed and self.cpu_frames == 1) t.expectEqualDeep(Rect{ .x = 2, .y = 2, .w = 2, .h = 2 }, damage.?) catch unreachable;
            return self.cpu_ok;
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
        primary_device = null;
        system_transition = false;
        bootfb_device = .{ .ops = &bootfb_ops };
        native_device = .{};
        native_backend = null;
        native_hold_generation = 0;
        native_restore_confirmed = false;
        held_boot = null;
        backend_manager = .{};
        completed_backend_state = .{};
        completed_stats = .{};
        present_generation = 0;
        completed_fence = 0;
        saved_boot = null;
        firmware_access.gate = .{};
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
    probe.cpu_ok = false;
    try t.expect(!fill(0x112233)); // Failed upload triggers the same recovery.
    try t.expect(!probe.cpu_active and probe.cpu_frames == 2);
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
            t.expect(firmware_access.gate.isRevoked() and held_boot.?.captured and held_boot.?.effects) catch unreachable;
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
        .release = Probe.release, .release_adopted = Probe.releaseAdopted };
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
    try t.expect(native_backend == null and held_boot != null and retainsDriverOwner(91));
    try t.expectEqual(held.generation, backendState().pending_generation);
    try t.expect(firmware_access.gate.isRevoked() and !fill(0x123456));
    try t.expectEqual(@as(u32, 0), probe.old_restores + probe.releases);

    _ = try prepareHeldNative(candidate, held.generation);
    probe.result = .confirmed;
    try t.expectEqual(CommitResult.confirmed, try commitNative(91, held.generation));
    try t.expectEqual(backend_state.State.software_native, backendState().state);
    try t.expectEqual(held.generation, backendState().generation);
    try t.expect(held_boot != null and firmware_access.gate.isRevoked());
    try t.expectError(error.Busy, finishBoot(91, held.generation, 2));
    try t.expectError(error.RestoreFailed, restoreBootBackend(91, held.generation));
    try t.expectEqual(@as(u32, 1), probe.native_restores);
    try t.expect(held_boot != null and retainsDriverOwner(91));
    try t.expectError(error.Stale, restoreBootBackend(91, held.generation));
    probe.restore_ok = true;
    try t.expectError(error.RestoreFailed, restoreBootBackend(91, backendState().generation));
    try t.expectEqual(@as(u32, 2), probe.native_restores);
    try t.expectEqual(@as(u32, 1), probe.releases);
    try t.expect(held_boot != null and firmware_access.gate.isRevoked());
    probe.release_ok = true;
    try restoreBootBackend(91, backendState().generation);
    try t.expectEqual(@as(u32, 2), probe.native_restores);
    try t.expectEqual(@as(u32, 2), probe.releases);
    try t.expectEqual(@as(u32, 0), probe.old_restores);
    try t.expectEqual(@as(u32, 1), probe.captures);
    try t.expect(held_boot == null and !retainsDriverOwner(91) and !firmware_access.gate.isRevoked());

    for ([_]CommitResult{ .output_lost, .confirmed }) |result| {
        holder.expected_generation = backendState().generation;
        const next = try holdBoot(holder);
        try t.expect(!try finishBoot(91, next.generation, 1));
        _ = try prepareHeldNative(candidate, next.generation);
        probe.result = result;
        try t.expectEqual(result, try commitNative(91, next.generation));
        try t.expect(held_boot != null and retainsDriverOwner(91));
        if (result == .output_lost) try t.expectEqual(backend_state.State.unavailable, backendState().state);
        if (result == .confirmed) {
            try t.expect(beginSystemTransition(0));
            try t.expect(!systemTransitionQuiesced());
        }
        try restoreBootBackend(91, backendState().generation);
        try t.expect(held_boot == null and !retainsDriverOwner(91));
    }
    try t.expect(systemTransitionQuiesced() and firmware_access.gate.isRevoked() and !fill(0xFFFFFF));
    try t.expectEqual(@as(u32, 0), probe.old_restores);
    system_transition = false;
    primary_device = &bootfb_device;
    firmware_access.gate.restore();
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
    system_transition = false;
    primary_device = &bootfb_device;
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
    system_transition = false;
    primary_device = &bootfb_device;
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

// Included only by the explicit display-reject-test boot build. The candidate
// never owns/programs hardware: its callback rejects the commit before writes.
pub fn probeRejectedTakeover() bool {
    const Probe = struct {
        fn commit(_: usize, _: u64, _: *const BootSnapshot) CommitResult {
            return .old_preserved;
        }
        fn restore(_: usize, _: u64, _: *const BootSnapshot) bool {
            return false;
        }
    };
    const boot = saved_boot orelse return false;
    const before = backendState();
    if (before.state != .bootfb or before.policy != .automatic) return false;
    const owner = ~@as(u32, 0);
    var frame = boot.framebuffer;
    var mapping = boot.mapping;
    mapping.kind = .native_scanout;
    const candidate = NativeBackend{ .owner = owner, .adapter_id = ~@as(u32, 0), .target = .{ .name = "rejection-probe", .kind = .native, .flags = DeviceFlags.cpu_present, .framebuffer = &frame, .mode = boot.mode, .mapping = mapping }, .commit = Probe.commit, .restore = Probe.restore };
    const generation = prepareNative(candidate) catch return false;
    const result = commitNative(owner, generation) catch {
        abortNative(owner, generation) catch {};
        return false;
    };
    const after = backendState();
    return result == .old_preserved and after.state == .bootfb and after.reason == .commit_failed and
        after.generation == before.generation and !retainsDriverOwner(owner) and !firmware_access.gate.isRevoked();
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
        primary_device = null;
        bootfb_device = .{ .ops = &bootfb_ops };
        present_generation = 0;
        completed_fence = 0;
        backend_manager = .{};
        completed_backend_state = .{};
        completed_stats = .{};
        saved_boot = null;
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

pub fn operationNames(flags: u32) []const u8 {
    if ((flags & DeviceFlags.xrgb32) != 0) return "present/blit/fill/rect/xrgb32";
    if ((flags & DeviceFlags.packed32) != 0) return "present/blit/fill/rect/packed32";
    if ((flags & DeviceFlags.rect) != 0) return "fill/rect";
    return "none";
}

pub fn presentReasonName(reason: PresentReason) []const u8 {
    return switch (reason) {
        .none => "none",
        .fill => "fill",
        .rect => "rect",
        .packed32_present => "packed32-present",
        .xrgb32_present => "xrgb32-present",
    };
}

pub fn mappingKindName(kind: MappingKind) []const u8 {
    return switch (kind) {
        .none => "none",
        .bootloader_framebuffer => "bootloader-framebuffer",
        .native_scanout => "native-scanout",
    };
}

pub fn cachePolicyName(policy: CachePolicy) []const u8 {
    return switch (policy) {
        .unknown => "unknown",
        .bootloader_default => "bootloader-default",
        .pat_write_combining => "pat-write-combining",
        .write_combining_unsupported => "write-combining-unsupported",
        .write_combining_failed => "write-combining-failed",
    };
}

fn bootfbFill(device: *Device, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.fill(f, rgb);
    recordPresent(device, .fill, 0, 0, @intCast(f.width), @intCast(f.height), false);
    return true;
}

fn clipSignedRect(x: i64, y: i64, w: u32, h: u32, mode: Mode) ?Rect {
    const left = @max(@as(i64, 0), x);
    const top = @max(@as(i64, 0), y);
    const right = @min(@as(i64, mode.width), x + w);
    const bottom = @min(@as(i64, mode.height), y + h);
    if (left >= right or top >= bottom) return null;
    return .{ .x = @intCast(left), .y = @intCast(top), .w = @intCast(right - left), .h = @intCast(bottom - top) };
}

fn bootfbRect(device: *Device, x: i32, y: i32, w: u32, h: u32, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    const clipped = clipSignedRect(x, y, w, h, device.mode) orelse return false;
    fb.rect(f, clipped.x, clipped.y, clipped.w, clipped.h, rgb);
    recordPresent(device, .rect, clipped.x, clipped.y, clipped.w, clipped.h, false);
    return true;
}

fn bootfbPutPacked32(device: *Device, x: u64, y: u64, color32: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.putPacked32(f, x, y, color32);
    return true;
}

fn bootfbPutXrgb32(device: *Device, x: u64, y: u64, rgb: u32) bool {
    const f = device.framebuffer orelse return false;
    fb.putPacked32(f, x, y, fb.packRgb(f, rgb));
    return true;
}

fn bootfbPresentPacked32Rect(device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    return bootfbCopyPacked32Rect(device, .packed32_present, x0, y0, w, h, src, src_stride_pixels);
}

fn bootfbCopyPacked32Rect(device: *Device, reason: PresentReason, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const f = device.framebuffer orelse return false;
    if (!fb.supportsRgb32(f) or w == 0 or h == 0) return false;
    if (x0 >= f.width or y0 >= f.height) return false;

    const clipped_w = @min(w, f.width - x0);
    const clipped_h = @min(h, f.height - y0);
    const src_stride_bytes = src_stride_pixels * 4;
    const row_bytes = clipped_w * 4;
    if (src.len < (clipped_h - 1) * src_stride_bytes + row_bytes) return false;

    var y: u64 = 0;
    while (y < clipped_h) : (y += 1) {
        const src_offset: usize = @intCast(y * src_stride_bytes);
        const dst = f.address + (y0 + y) * f.pitch + x0 * 4;
        copyToVisible(dst, src[src_offset .. src_offset + @as(usize, @intCast(row_bytes))]);
    }
    recordPresent(device, reason, @intCast(x0), @intCast(y0), @intCast(clipped_w), @intCast(clipped_h), false);
    return true;
}

fn bootfbPresentXrgb32Rect(device: *Device, x0: u64, y0: u64, w: u64, h: u64, src: []const u8, src_stride_pixels: u64) bool {
    const f = device.framebuffer orelse return false;
    if (!fb.supportsRgb32(f) or w == 0 or h == 0) return false;
    if (x0 >= f.width or y0 >= f.height) return false;

    const clipped_w = @min(w, f.width - x0);
    const clipped_h = @min(h, f.height - y0);
    const src_stride_bytes = src_stride_pixels * 4;
    const row_bytes = clipped_w * 4;
    if (src.len < (clipped_h - 1) * src_stride_bytes + row_bytes) return false;

    if (fb.isNativeXrgb32(f)) {
        return bootfbCopyPacked32Rect(device, .xrgb32_present, x0, y0, clipped_w, clipped_h, src, src_stride_pixels);
    }

    var y: u64 = 0;
    while (y < clipped_h) : (y += 1) {
        const src_offset: usize = @intCast(y * src_stride_bytes);
        var x: u64 = 0;
        while (x < clipped_w) : (x += 1) {
            const pixel_offset = src_offset + @as(usize, @intCast(x * 4));
            fb.putPacked32(f, x0 + x, y0 + y, fb.packRgb(f, readXrgb32(src, pixel_offset)));
        }
    }
    recordPresent(device, .xrgb32_present, @intCast(x0), @intCast(y0), @intCast(clipped_w), @intCast(clipped_h), true);
    return true;
}

fn bootfbCopyXrgb32Region(device: *Device, region: PresentRegion, src: []const u8, src_stride_pixels: u32) bool {
    const f = device.framebuffer orelse return false;
    const src_stride_bytes = @as(u64, src_stride_pixels) * @sizeOf(u32);
    const row_bytes = @as(u64, region.w) * @sizeOf(u32);
    const first_offset = (@as(u64, region.src_y) * src_stride_pixels + region.src_x) * @sizeOf(u32);
    const last_end = first_offset + (@as(u64, region.h) - 1) * src_stride_bytes + row_bytes;
    if (last_end > src.len) return false;

    var y: u32 = 0;
    while (y < region.h) : (y += 1) {
        const src_offset: usize = @intCast(first_offset + @as(u64, y) * src_stride_bytes);
        if (fb.isNativeXrgb32(f)) {
            const dst = f.address + (@as(u64, region.dst_y) + y) * f.pitch + @as(u64, region.dst_x) * @sizeOf(u32);
            copyToVisible(dst, src[src_offset .. src_offset + @as(usize, @intCast(row_bytes))]);
            continue;
        }
        var x: u32 = 0;
        while (x < region.w) : (x += 1) {
            const pixel_offset = src_offset + @as(usize, x) * @sizeOf(u32);
            fb.putPacked32(
                f,
                @as(u64, region.dst_x) + x,
                @as(u64, region.dst_y) + y,
                fb.packRgb(f, readXrgb32(src, pixel_offset)),
            );
        }
    }
    return true;
}

fn validPresentRegion(region: PresentRegion, mode: Mode, source_pixel_count: u32, source_stride_pixels: u32) bool {
    if (region.w == 0 or region.h == 0 or region.src_x >= source_stride_pixels) return false;
    if (region.w > source_stride_pixels - region.src_x) return false;
    if (region.dst_x >= mode.width or region.dst_y >= mode.height) return false;
    if (region.w > mode.width - region.dst_x or region.h > mode.height - region.dst_y) return false;
    const last_row = @as(u64, region.src_y) + region.h - 1;
    const last_end = last_row * source_stride_pixels + region.src_x + region.w;
    return last_end <= source_pixel_count;
}

fn mergeRect(a: Rect, b: Rect) Rect {
    const left = @min(a.x, b.x);
    const top = @min(a.y, b.y);
    const right = @max(@as(u64, a.x) + a.w, @as(u64, b.x) + b.w);
    const bottom = @max(@as(u64, a.y) + a.h, @as(u64, b.y) + b.h);
    return .{
        .x = left,
        .y = top,
        .w = @intCast(right - left),
        .h = @intCast(bottom - top),
    };
}

fn copyName(out: []u8, name: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(name.len, out.len - 1);
    if (count != 0) @memcpy(out[0..count], name[0..count]);
}

fn recordPresent(device: *Device, reason: PresentReason, x: u32, y: u32, w: u32, h: u32, converted: bool) void {
    const pixels = @as(u64, w) * h;
    recordPresentAggregate(device, reason, .{ .x = x, .y = y, .w = w, .h = h }, pixels, converted);
}

fn recordPresentAggregate(device: *Device, reason: PresentReason, bounds: Rect, pixels: u64, converted: bool) void {
    const bytes = pixels * 4;
    device.present_count += 1;
    device.present_pixels_total += pixels;
    device.present_bytes_total += bytes;
    device.last_present_pixels = pixels;
    device.last_present_bytes = bytes;
    device.last_present_rect = bounds;
    device.last_present_reason = reason;
    device.last_present_converted = converted;
    if (converted) device.conversion_present_count += 1;
    if (bounds.x == 0 and bounds.y == 0 and bounds.w == device.mode.width and bounds.h == device.mode.height and pixels == @as(u64, device.mode.width) * device.mode.height) {
        device.full_present_count += 1;
    } else {
        device.partial_present_count += 1;
    }
    switch (reason) {
        .none => {},
        .fill => device.fill_present_count += 1,
        .rect => device.rect_present_count += 1,
        .packed32_present => device.packed32_present_count += 1,
        .xrgb32_present => device.xrgb32_present_count += 1,
    }
}

fn recordPresentTiming(device: *Device, start: u64) void {
    recordPresentTimingAt(device, start, timer.tickCount());
}

fn recordPresentTimingAt(device: *Device, start: u64, end: u64) void {
    const elapsed = if (end >= start) end - start else 0;
    device.present_total_ticks +%= elapsed;
    device.present_last_ticks = elapsed;
    if (elapsed > device.present_max_ticks) device.present_max_ticks = elapsed;
    if (elapsed > 1) device.present_slow_count +%= 1;
}

fn copyToVisible(dst: [*]volatile u8, src: []const u8) void {
    // 0.56.12: Vorab-validierter, subcall-freier u32-Zeilenpfad. Der alte
    // Loop rief readXrgb32 PRO PIXEL (Bounds-Check + Alignment-Check je
    // Wort) - bei Millionen Pixeln pro Frame reiner Overhead. Ist sowohl
    // Ziel ALS AUCH Quelle 4-Byte-ausgerichtet (Normalfall: Framebuffer
    // und Present-Puffer sind seiten-/wortausgerichtet), laeuft eine
    // reine Wort-fuer-Wort-Kopie ganz ohne Pro-Pixel-Check.
    const word_len = src.len & ~@as(usize, 3);
    if ((@intFromPtr(dst) & 3) == 0 and (@intFromPtr(src.ptr) & 3) == 0) {
        const dst_words: [*]volatile u32 = @ptrCast(@alignCast(dst));
        const src_words: [*]const u32 = @ptrCast(@alignCast(src.ptr));
        const count = word_len / 4;
        var wi: usize = 0;
        while (wi < count) : (wi += 1) dst_words[wi] = src_words[wi];
        var i: usize = word_len;
        while (i < src.len) : (i += 1) dst[i] = src[i];
        return;
    }
    // Ziel wortausgerichtet, Quelle nicht: Woerter aus Bytes assemblieren,
    // aber ohne den Pro-Wort-Bounds-Check des alten readXrgb32.
    if ((@intFromPtr(dst) & 3) == 0 and word_len == src.len) {
        const dst_words: [*]volatile u32 = @ptrCast(@alignCast(dst));
        const count = word_len / 4;
        var wi: usize = 0;
        while (wi < count) : (wi += 1) {
            const o = wi * 4;
            dst_words[wi] = @as(u32, src[o]) |
                (@as(u32, src[o + 1]) << 8) |
                (@as(u32, src[o + 2]) << 16) |
                (@as(u32, src[o + 3]) << 24);
        }
        return;
    }
    var i: usize = 0;
    while (i < src.len) : (i += 1) dst[i] = src[i];
}

fn readXrgb32(src: []const u8, offset: usize) u32 {
    if (offset + 3 >= src.len) return 0;
    const ptr = &src[offset];
    if ((@intFromPtr(ptr) & 3) == 0) {
        const word: *const u32 = @ptrCast(@alignCast(ptr));
        return word.*;
    }
    return @as(u32, src[offset + 0]) |
        (@as(u32, src[offset + 1]) << 8) |
        (@as(u32, src[offset + 2]) << 16) |
        (@as(u32, src[offset + 3]) << 24);
}

pub const DisplayManager = struct {
    // Marker type for the current singleton manager. In this transition step the
    // module still owns the active backend internally; callers should treat the
    // exported fill/rect/put/present functions as the DisplayManager boundary.
};
