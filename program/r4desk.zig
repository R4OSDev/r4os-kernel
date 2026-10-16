//! Temporary fixed projection to native R4DESK input/clipboard/capture owners.
const r4x_api = @import("r4x_api.zig");
const remote_frame_state = @import("remote_frame_state.zig");
pub const name = "R4DESK";
pub const clipboard_text_size: usize = 4096;
pub const clipboard_max_text_bytes = r4x_api.clipboard_max_text_bytes;
pub const clipboard_flag_has_text = r4x_api.clipboard_flag_has_text;
pub const clipboard_flag_text = r4x_api.clipboard_flag_text;
pub const clipboard_error_invalid = r4x_api.clipboard_error_invalid;
pub const clipboard_error_too_large = r4x_api.clipboard_error_too_large;
pub const clipboard_error_buffer_too_small = r4x_api.clipboard_error_buffer_too_small;
pub const remote_frame_magic = r4x_api.remote_frame_magic;
pub const remote_frame_version = r4x_api.remote_frame_version;
pub const remote_frame_format_xrgb32 = r4x_api.remote_frame_format_xrgb32;
pub const remote_frame_flag_ready = r4x_api.remote_frame_flag_ready;
pub const remote_frame_flag_dirty_valid = r4x_api.remote_frame_flag_dirty_valid;
pub const remote_frame_flag_cursor_valid = r4x_api.remote_frame_flag_cursor_valid;
pub const remote_frame_cursor_flag_visible = r4x_api.remote_frame_cursor_flag_visible;
pub const remote_frame_error_invalid = r4x_api.remote_frame_error_invalid;
pub const remote_frame_error_unavailable = r4x_api.remote_frame_error_unavailable;
pub const remote_frame_error_buffer_too_small = r4x_api.remote_frame_error_buffer_too_small;
pub const remote_frame_error_out_of_range = r4x_api.remote_frame_error_out_of_range;
pub const remote_frame_error_oom = r4x_api.remote_frame_error_oom;
pub const remote_frame_error_unsupported = r4x_api.remote_frame_error_unsupported;
pub const remote_input_magic = r4x_api.remote_input_magic;
pub const remote_input_version = r4x_api.remote_input_version;
pub const remote_input_kind_key_down = r4x_api.remote_input_kind_key_down;
pub const remote_input_kind_key_up = r4x_api.remote_input_kind_key_up;
pub const remote_input_kind_mouse_move = r4x_api.remote_input_kind_mouse_move;
pub const remote_input_kind_mouse_buttons = r4x_api.remote_input_kind_mouse_buttons;
pub const remote_input_kind_mouse_wheel = r4x_api.remote_input_kind_mouse_wheel;
pub const remote_input_flag_down = r4x_api.remote_input_flag_down;
pub const remote_input_flag_up = r4x_api.remote_input_flag_up;
pub const remote_input_flag_absolute = r4x_api.remote_input_flag_absolute;
pub const remote_input_error_invalid = r4x_api.remote_input_error_invalid;
pub const remote_input_error_empty = r4x_api.remote_input_error_empty;
pub const remote_input_error_full = r4x_api.remote_input_error_full;
pub const remote_input_error_unsupported = r4x_api.remote_input_error_unsupported;
pub const remote_input_queue_capacity: usize = 64;
pub const MouseApiState = r4x_api.Mouse;
pub const KeyboardLayoutInfo = r4x_api.KeyboardLayoutInfo;
pub const ClipboardInfo = r4x_api.ClipboardInfo;
pub const RemoteFrameInfo = r4x_api.RemoteFrameInfo;
pub const RemoteFrameLease = r4x_api.RemoteFrameLease;
pub const RemoteFrameCaptureStats = r4x_api.RemoteFrameCaptureStats;
pub const CaptureOwner = remote_frame_state.Owner;
pub const RemoteFrameMapInfo = r4x_api.RemoteFrameMapInfo;
pub const DisplayDamageRect = r4x_api.DisplayDamageRect;
pub const RemoteInputEvent = r4x_api.RemoteInputEvent;
pub const RemoteInputStatus = r4x_api.RemoteInputStatus;
pub const PhysicalKeyEvent = r4x_api.PhysicalKeyEvent;

const Args = extern struct { owner: CaptureOwner = .{}, a: u64 = 0, b: u64 = 0, value: u32 = 0, other: u32 = 0, input: ?*const anyopaque = null, second: ?*const anyopaque = null, third: ?*const anyopaque = null };
extern fn r4os_rust_r4desk(u32, *const Args, ?*anyopaque) u64;
fn signed(op: u32, args: Args, out: ?*anyopaque) i32 {
    return @bitCast(@as(u32, @truncate(r4os_rust_r4desk(op, &args, out))));
}
pub fn mouseState(out: *MouseApiState) callconv(.c) void {
    _ = r4os_rust_r4desk(0, &Args{}, out);
}
pub fn mouseMotion(out: *r4x_api.MouseMotion) callconv(.c) i32 {
    return signed(1, .{}, out);
}
pub fn mouseShow() callconv(.c) void {
    _ = r4os_rust_r4desk(2, &Args{}, null);
}
pub fn mouseHide() callconv(.c) void {
    _ = r4os_rust_r4desk(3, &Args{}, null);
}
pub fn physicalKeyPoll(out: *PhysicalKeyEvent) callconv(.c) i32 {
    return signed(4, .{}, out);
}
pub fn keyboardLayoutCurrent(out: *KeyboardLayoutInfo) callconv(.c) i32 {
    return signed(5, .{}, out);
}
pub fn keyboardLayoutAt(index: u32, out: *KeyboardLayoutInfo) callconv(.c) i32 {
    return signed(6, .{ .value = index }, out);
}
pub fn keyboardLayoutSet(layout_name: [*:0]const u8) callconv(.c) i32 {
    return signed(7, .{ .input = layout_name }, null);
}
pub fn clipboardWrite(data: [*]const u8, len: u32) callconv(.c) i32 {
    return signed(8, .{ .input = data, .value = len }, null);
}
pub fn clipboardRead(out: [*]u8, capacity: u32) callconv(.c) i32 {
    return signed(9, .{ .value = capacity }, out);
}
pub fn clipboardRevision() callconv(.c) u32 {
    return @intCast(r4os_rust_r4desk(10, &Args{}, null));
}
pub fn clipboardInfo(out: *ClipboardInfo) callconv(.c) i32 {
    return signed(11, .{}, out);
}
pub fn clipboardClear() callconv(.c) i32 {
    return signed(12, .{}, null);
}
pub fn remoteFrameInfo(out: *RemoteFrameInfo) callconv(.c) i32 {
    return signed(13, .{}, out);
}
pub fn remoteFrameRead(offset_pixels: u32, out: [*]u32, pixel_count: u32, out_info: *RemoteFrameInfo) callconv(.c) i32 {
    return signed(14, .{ .a = offset_pixels, .value = pixel_count, .second = out_info }, out);
}
pub fn remoteFrameWait(last_revision: u32, timeout_ticks: u64, out: *RemoteFrameInfo) callconv(.c) i32 {
    return signed(15, .{ .value = last_revision, .a = timeout_ticks }, out);
}
pub fn desktopActivityWait(last_seq: u64, timeout_ticks: u64, out_seq: *u64) callconv(.c) i32 {
    return signed(16, .{ .a = last_seq, .b = timeout_ticks }, out_seq);
}
pub fn desktopActivityNotify() callconv(.c) i32 {
    return signed(17, .{}, null);
}
pub fn remoteFrameAcquire(owner: CaptureOwner) i32 {
    return signed(18, .{ .owner = owner }, null);
}
pub fn remoteFrameRelease(owner: CaptureOwner) i32 {
    return signed(19, .{ .owner = owner }, null);
}
pub fn remoteFramePublisher(owner: CaptureOwner) bool {
    return r4os_rust_r4desk(20, &Args{ .owner = owner }, null) != 0;
}
pub fn remoteFrameSourceReset(owner: CaptureOwner) i32 {
    return signed(21, .{ .owner = owner }, null);
}
pub fn remoteFrameStopped(owner: CaptureOwner) void {
    _ = r4os_rust_r4desk(22, &Args{ .owner = owner }, null);
}
pub fn remoteFrameConsumers() callconv(.c) u32 {
    return @intCast(r4os_rust_r4desk(23, &Args{}, null));
}
pub fn remoteFramePublish(info: *const RemoteFrameInfo, pixels_ptr: [*]const u32, pixel_count: u32) callconv(.c) i32 {
    return signed(24, .{ .input = info, .second = pixels_ptr, .value = pixel_count }, null);
}
pub fn remoteFramePublishRegions(
    info: *const RemoteFrameInfo,
    pixels_ptr: [*]const u32,
    pixel_count: u32,
    regions_ptr: [*]const DisplayDamageRect,
    region_count: u32,
) callconv(.c) i32 {
    return signed(25, .{ .input = info, .second = pixels_ptr, .value = pixel_count, .third = regions_ptr, .other = region_count }, null);
}
pub fn remoteInputPush(event: *const RemoteInputEvent) callconv(.c) i32 {
    return signed(26, .{ .input = event }, null);
}
pub fn remoteInputPoll(out: *RemoteInputEvent) callconv(.c) i32 {
    return signed(27, .{}, out);
}
pub fn remoteInputStatus(out: *RemoteInputStatus) callconv(.c) i32 {
    return signed(28, .{}, out);
}
pub fn remoteFrameMap(out: *RemoteFrameMapInfo) callconv(.c) i32 {
    return signed(29, .{}, out);
}
pub fn remoteFrameSnapshotAcquire(owner: CaptureOwner, expected_revision: u32, out_info: *RemoteFrameInfo, out_lease: *RemoteFrameLease) i32 {
    return signed(30, .{ .owner = owner, .value = expected_revision, .second = out_lease }, out_info);
}
pub fn remoteFrameSnapshotRelease(owner: CaptureOwner, lease: *const RemoteFrameLease) i32 {
    return signed(31, .{ .owner = owner, .input = lease }, null);
}
pub fn remoteFrameCaptureStats(out: *RemoteFrameCaptureStats) callconv(.c) i32 {
    return signed(32, .{}, out);
}
comptime {
    if (@sizeOf(Args) != 64 or @offsetOf(Args, "input") != 40) @compileError("R4DESK C args layout");
}
