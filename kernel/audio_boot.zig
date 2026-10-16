const boot_status = @import("boot_status.zig");
const fatal = @import("fatal.zig");
const MAX_SID_MODEL: usize = 16;

pub const BackendState = enum {
    skipped,
    disabled,
    selected,
    failed,
    r4d_planned,
    r4d_missing,
    r4d_loaded,
    r4d_registered,
    r4d_active,
};

pub const BackendStatus = struct {
    state: BackendState = .skipped,
    selected: bool = false,
    disabled: bool = false,
    attempted: bool = false,
    failed: bool = false,
    r4d_expected: bool = false,
    r4d_loaded: bool = false,
    registered: bool = false,
    active: bool = false,
    reason: []const u8 = "not selected",
};

pub const R4dPlanResult = enum {
    loaded,
    already_active,
    missing,
    init_failed,
    load_failed,
};

pub const Status = struct {
    initialized: bool = false,
    core_initialized: bool = false,
    platform_irq_ready: bool = false,
    config_visible: bool = false,
    sid_option_present: bool = false,
    sid_option_valid: bool = false,
    sid_requested_model: [MAX_SID_MODEL]u8 = .{0} ** MAX_SID_MODEL,
    sid_requested_model_len: usize = 0,
    sid_model: [MAX_SID_MODEL]u8 = .{0} ** MAX_SID_MODEL,
    sid_model_len: usize = 0,
    sid_reason: []const u8 = "not initialized",
    hda: BackendStatus = .{},
    ac97: BackendStatus = .{},
    reason: []const u8 = "not initialized",
};

const Span = extern struct {
    pointer: ?[*]const u8 = null,
    len: usize = 0,
    fn bytes(self: @This()) []const u8 {
        return if (self.len == 0) "" else self.pointer.?[0..self.len];
    }
};
const OptionalSpan = extern struct { present: bool = false, value: Span = .{} };
const WireBackend = extern struct { state: u32, selected: bool, disabled: bool, attempted: bool, failed: bool, r4d_expected: bool, r4d_loaded: bool, registered: bool, active: bool, reason: Span };
const WireStatus = extern struct { initialized: bool, core_initialized: bool, platform_irq_ready: bool, config_visible: bool, sid_option_present: bool, sid_option_valid: bool, sid_requested_model: [16]u8, sid_requested_model_len: usize, sid_model: [16]u8, sid_model_len: usize, sid_reason: Span, hda: WireBackend, ac97: WireBackend, reason: Span };
extern fn r4os_rust_audio_boot(op: u32, value: u32, out: ?*anyopaque) bool;
fn backend(w: WireBackend) BackendStatus {
    return .{ .state = @enumFromInt(w.state), .selected = w.selected, .disabled = w.disabled, .attempted = w.attempted, .failed = w.failed, .r4d_expected = w.r4d_expected, .r4d_loaded = w.r4d_loaded, .registered = w.registered, .active = w.active, .reason = w.reason.bytes() };
}
pub fn init() bool {
    return r4os_rust_audio_boot(0, 0, null);
}
pub fn isInitialized() bool {
    return r4os_rust_audio_boot(1, 0, null);
}
pub fn status() Status {
    var w: WireStatus = undefined;
    _ = r4os_rust_audio_boot(2, 0, &w);
    return .{ .initialized = w.initialized, .core_initialized = w.core_initialized, .platform_irq_ready = w.platform_irq_ready, .config_visible = w.config_visible, .sid_option_present = w.sid_option_present, .sid_option_valid = w.sid_option_valid, .sid_requested_model = w.sid_requested_model, .sid_requested_model_len = w.sid_requested_model_len, .sid_model = w.sid_model, .sid_model_len = w.sid_model_len, .sid_reason = w.sid_reason.bytes(), .hda = backend(w.hda), .ac97 = backend(w.ac97), .reason = w.reason.bytes() };
}
pub fn hdaStatus() BackendStatus {
    var w: WireBackend = undefined;
    _ = r4os_rust_audio_boot(3, 0, &w);
    return backend(w);
}
pub fn ac97Status() BackendStatus {
    var w: WireBackend = undefined;
    _ = r4os_rust_audio_boot(4, 0, &w);
    return backend(w);
}
pub fn sidModel() []const u8 {
    var span: Span = .{};
    _ = r4os_rust_audio_boot(5, 0, &span);
    return span.bytes();
}
pub fn sidRequestedModel() []const u8 {
    var span: Span = .{};
    _ = r4os_rust_audio_boot(6, 0, &span);
    return span.bytes();
}
pub fn reportAc97R4dPlan(result: R4dPlanResult) void {
    _ = r4os_rust_audio_boot(7, @intFromEnum(result), null);
}
pub fn reportHdaR4dPlan(result: R4dPlanResult) void {
    _ = r4os_rust_audio_boot(8, @intFromEnum(result), null);
}
pub fn backendStateName(state: BackendState) []const u8 {
    var span: Span = .{};
    _ = r4os_rust_audio_boot(9, @intFromEnum(state), &span);
    return span.bytes();
}
pub fn dumpStatus() void {
    _ = r4os_rust_audio_boot(10, 0, null);
}
export fn r4os_zig_audio_boot_fail(data: [*]const u8, len: usize) callconv(.c) bool {
    return fatal.fail(.audio, data[0..len]);
}
export fn r4os_zig_audio_boot_status(data: [*]const u8, len: usize) callconv(.c) void {
    boot_status.statusLine(data[0..len]);
}
comptime {
    if (@sizeOf(WireBackend) != 32 or @sizeOf(WireStatus) != 152 or @sizeOf(OptionalSpan) != 24) @compileError("private audio boot layout");
}
