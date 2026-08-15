const r4x_api = @import("r4x_api.zig");
const keyboard = @import("../driver/input/keyboard.zig");
const mouse = @import("../driver/input/mouse.zig");
const heap = @import("../memory/heap.zig");
const services = @import("../kernel/services.zig");
const sync = @import("../sched/sync.zig");
const desktop_events = @import("../kernel/desktop_events.zig");
const timer = @import("../kernel/timer.zig");
const r4draw = @import("r4draw.zig");

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

const clipboard_service_name = "CLIPSVC";
const clipboard_service_op_write = r4x_api.clipboard_service_op_write;
const clipboard_service_op_read = r4x_api.clipboard_service_op_read;
const clipboard_service_op_info = r4x_api.clipboard_service_op_info;
const clipboard_service_op_clear = r4x_api.clipboard_service_op_clear;
// 0.56.40: hz-neutral (1200 ms; bei 100 Hz wie zuvor 120 Ticks).
const clipboard_service_timeout_ticks: u64 = @max(1, (1200 * @as(u64, timer.DEFAULT_HZ)) / 1000);
const clipboard_info_payload_size: usize = 16;

pub const MouseApiState = r4x_api.Mouse;

pub const KeyboardLayoutInfo = r4x_api.KeyboardLayoutInfo;

pub const ClipboardInfo = r4x_api.ClipboardInfo;

pub const RemoteFrameInfo = r4x_api.RemoteFrameInfo;

// 0.56.38: Shared-Frame-Mapping - liefert dem Konsumenten (RDPSVC) die
// Adresse des Remote-Frame-Puffers direkt (R4X laeuft im gemeinsamen,
// voll vertrauten Adressraum; Leitbild: kein Sicherheitsmodell). Die
// generation steigt bei jeder Neuallokation des Puffers; der Konsument
// ruft remoteFrameMap pro Frame neu (billiger Tabellen-Call) und liest
// nur bei unveraenderter Geometrie direkt.
pub const RemoteFrameMapInfo = r4x_api.RemoteFrameMapInfo;

pub const RemoteInputEvent = r4x_api.RemoteInputEvent;

pub const RemoteInputStatus = r4x_api.RemoteInputStatus;

const RemoteRect = struct {
    x: u32 = 0,
    y: u32 = 0,
    w: u32 = 0,
    h: u32 = 0,
};

var clipboard_text: [clipboard_text_size]u8 = .{0} ** clipboard_text_size;
var clipboard_len: usize = 0;
var clipboard_rev: u32 = 0;
var remote_frame_memory: ?[]u8 = null;
var remote_frame_pixels: ?[]u32 = null;
var remote_frame_capacity_pixels: usize = 0;
var remote_frame_info: RemoteFrameInfo = .{};
var remote_frame_ready: bool = false;
// 0.56.38: Shared-Frame-Zustand. Sobald ein Mapping ausgegeben wurde,
// darf ein Realloc den alten Puffer nicht mehr sofort freigeben (der
// Konsument koennte noch mitten in einem Frame-Read stehen); alte
// Puffer wandern in einen kleinen Retired-Ring. Geometriewechsel sind
// Boot-/Konfigurationsereignisse - der Ring laeuft praktisch nie um.
var remote_frame_generation: u64 = 0;
var remote_frame_mapped: bool = false;
var remote_frame_retired: [4]?[]u8 = .{ null, null, null, null };
var remote_frame_retired_next: usize = 0;
// 0.56.27: Event-Wait fuer remoteFrameWait - remoteFramePublish weckt die
// Queue, Waiter pruefen die Revision als waitUnless-Praedikat (kein
// Lost-Wakeup, Muster 0.56.19/0.56.22).
var remote_frame_waitq: sync.WaitQueue = sync.WaitQueue.init();
// 0.56.40: hz-neutral (250-ms-Slice; bei 100 Hz wie zuvor 25 Ticks).
const REMOTE_FRAME_WAIT_SLICE_TICKS: u64 = @max(1, (250 * @as(u64, timer.DEFAULT_HZ)) / 1000);

const RemoteFrameWaitCtx = struct {
    last_revision: u32,
};

fn predStillWaitFrame(raw: *anyopaque) bool {
    const c: *RemoteFrameWaitCtx = @ptrCast(@alignCast(raw));
    return !(remote_frame_ready and remote_frame_info.revision != c.last_revision);
}
var remote_input_queue: [remote_input_queue_capacity]RemoteInputEvent = .{RemoteInputEvent{}} ** remote_input_queue_capacity;
var remote_input_head: usize = 0;
var remote_input_tail: usize = 0;
var remote_input_pending: usize = 0;
var remote_input_pushed: u32 = 0;
var remote_input_polled: u32 = 0;
var remote_input_dropped: u32 = 0;
var remote_input_sequence: u32 = 0;
var remote_input_last: RemoteInputEvent = .{};

pub fn mouseState(out: *MouseApiState) callconv(.c) void {
    const s = mouse.snapshotForApi();
    out.* = .{
        .x = s.x,
        .y = s.y,
        .dx = s.dx,
        .dy = s.dy,
        .wheel = s.wheel,
        .buttons = s.buttons,
        .present = if (s.present) 1 else 0,
        .reserved = 0,
        .packets = s.packets,
    };
}

pub fn mouseShow() callconv(.c) void {
    r4draw.markDisplayUsed();
    mouse.enableCursor();
}

pub fn mouseHide() callconv(.c) void {
    mouse.disableCursor();
}

pub fn keyboardLayoutCurrent(out: *KeyboardLayoutInfo) callconv(.c) i32 {
    fillKeyboardLayoutInfo(out, keyboard.activeLayoutIndex()) orelse return -1;
    return 1;
}

pub fn keyboardLayoutAt(index: u32, out: *KeyboardLayoutInfo) callconv(.c) i32 {
    fillKeyboardLayoutInfo(out, @intCast(index)) orelse return 0;
    return 1;
}

pub fn keyboardLayoutSet(layout_name: [*:0]const u8) callconv(.c) i32 {
    var name_buf: [32]u8 = .{0} ** 32;
    const value = copyZ(layout_name, name_buf[0..]) orelse return -1;
    return if (keyboard.trySetLayoutByName(value)) 0 else -1;
}

pub fn clipboardWrite(data: [*]const u8, len: u32) callconv(.c) i32 {
    if (@intFromPtr(data) == 0) return clipboard_error_invalid;
    const count: usize = @intCast(len);
    if (count > clipboard_max_text_bytes) return clipboard_error_too_large;
    var i: usize = 0;
    while (i < count) : (i += 1) {
        if (data[i] == 0) return clipboard_error_invalid;
    }
    const text = data[0..count];
    if (clipboardServiceWrite(text)) |result| return result;
    return clipboardWriteLocal(text);
}

pub fn clipboardRead(out: [*]u8, capacity: u32) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return clipboard_error_invalid;
    const out_len: usize = @intCast(capacity);
    if (clipboardServiceRead(out, out_len)) |result| return result;
    return clipboardReadLocal(out, out_len);
}

pub fn clipboardRevision() callconv(.c) u32 {
    var info = ClipboardInfo{};
    if (clipboardServiceInfo(&info)) return info.revision;
    return clipboard_rev;
}

pub fn clipboardInfo(out: *ClipboardInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return clipboard_error_invalid;
    if (clipboardServiceInfo(out)) return 0;
    out.* = .{
        .capacity = @intCast(clipboard_max_text_bytes),
        .length = @intCast(clipboard_len),
        .revision = clipboard_rev,
        .flags = clipboard_flag_text | (if (clipboard_len > 0) clipboard_flag_has_text else 0),
    };
    return 0;
}

pub fn clipboardClear() callconv(.c) i32 {
    if (clipboardServiceClear()) |result| return result;
    clearClipboardStorage();
    bumpClipboardRevision();
    return 0;
}

pub fn remoteFrameInfo(out: *RemoteFrameInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return remote_frame_error_invalid;
    out.* = remote_frame_info;
    return if (remote_frame_ready) 0 else remote_frame_error_unavailable;
}

pub fn remoteFrameRead(offset_pixels: u32, out: [*]u32, pixel_count: u32, out_info: *RemoteFrameInfo) callconv(.c) i32 {
    if (@intFromPtr(out_info) != 0) out_info.* = remote_frame_info;
    if (!remote_frame_ready) return remote_frame_error_unavailable;
    const pixels = remote_frame_pixels orelse return remote_frame_error_unavailable;
    const offset: usize = @intCast(offset_pixels);
    if (offset > pixels.len) return remote_frame_error_out_of_range;
    if (pixel_count == 0) return 0;
    if (@intFromPtr(out) == 0) return remote_frame_error_invalid;
    const remaining = pixels.len - offset;
    if (remaining == 0) return 0;
    const requested: usize = @intCast(pixel_count);
    const count = @min(requested, remaining);
    @memcpy(out[0..count], pixels[offset .. offset + count]);
    return @intCast(count);
}

pub fn remoteFrameWait(last_revision: u32, timeout_ticks: u64, out: *RemoteFrameInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return remote_frame_error_invalid;
    if (!remote_frame_ready) {
        out.* = remote_frame_info;
        return remote_frame_error_unavailable;
    }

    var waited: u64 = 0;
    while (true) {
        if (remote_frame_ready and remote_frame_info.revision != last_revision) {
            out.* = remote_frame_info;
            return 1;
        }
        if (waited >= timeout_ticks) break;
        const slice = @min(REMOTE_FRAME_WAIT_SLICE_TICKS, timeout_ticks - waited);
        var wait_ctx = RemoteFrameWaitCtx{ .last_revision = last_revision };
        _ = remote_frame_waitq.waitUnless(slice, "remote-frame", predStillWaitFrame, &wait_ctx);
        waited += slice;
    }
    out.* = remote_frame_info;
    return 0;
}

// 0.56.28: blockierender Aktivitaets-Wait fuer den Desktop-Renderer.
// last_seq = zuletzt gesehene Sequenz (0 beim ersten Aufruf), out_seq
// erhaelt den neuen Stand. 1 = Aktivitaet, 0 = Timeout.
pub fn desktopActivityWait(last_seq: u64, timeout_ticks: u64, out_seq: *u64) callconv(.c) i32 {
    if (@intFromPtr(out_seq) == 0) return remote_input_error_invalid;
    return desktop_events.wait(last_seq, timeout_ticks, out_seq);
}

pub fn remoteFramePublish(info: *const RemoteFrameInfo, pixels_ptr: [*]const u32, pixel_count: u32) callconv(.c) i32 {
    if (@intFromPtr(info) == 0) return remote_frame_error_invalid;
    if (info.format != remote_frame_format_xrgb32 or info.bytes_per_pixel != 4) return remote_frame_error_unsupported;
    if (info.width == 0 or info.height == 0 or info.stride_pixels < info.width) return remote_frame_error_invalid;

    const total_pixels_u64 = @as(u64, info.width) * @as(u64, info.height);
    const max_frame_pixels: u64 = 0xffff_ffff / @sizeOf(u32);
    if (total_pixels_u64 == 0 or total_pixels_u64 > max_frame_pixels) return remote_frame_error_invalid;
    const source_pixels_u64 = @as(u64, info.stride_pixels) * @as(u64, info.height);
    if (source_pixels_u64 > @as(u64, pixel_count)) return remote_frame_error_invalid;
    if (@intFromPtr(pixels_ptr) == 0) return remote_frame_error_invalid;

    const total_pixels: usize = @intCast(total_pixels_u64);
    if (!ensureRemoteFrameCapacity(total_pixels)) return remote_frame_error_oom;
    const dest = remote_frame_pixels orelse return remote_frame_error_oom;
    const source = pixels_ptr[0..@as(usize, @intCast(source_pixels_u64))];

    const geometry_changed = !remote_frame_ready or
        remote_frame_info.width != info.width or
        remote_frame_info.height != info.height;
    const rect = normalizeRemoteRect(info, geometry_changed);
    copyRemoteFrameRect(dest, source, info.width, info.stride_pixels, rect);

    const revision = bumpRemoteFrameRevision();
    remote_frame_info = .{
        .magic = remote_frame_magic,
        .version = remote_frame_version,
        .flags = remote_frame_flag_ready | remote_frame_flag_dirty_valid | remote_frame_flag_cursor_valid,
        .format = remote_frame_format_xrgb32,
        .width = info.width,
        .height = info.height,
        .stride_pixels = info.width,
        .bytes_per_pixel = 4,
        .revision = revision,
        .frame_pixels = @intCast(total_pixels),
        .frame_bytes = @intCast(total_pixels * @sizeOf(u32)),
        .dirty_x = @intCast(rect.x),
        .dirty_y = @intCast(rect.y),
        .dirty_w = rect.w,
        .dirty_h = rect.h,
        .cursor_x = info.cursor_x,
        .cursor_y = info.cursor_y,
        .cursor_flags = info.cursor_flags,
    };
    remote_frame_ready = true;
    _ = remote_frame_waitq.wakeAll();

    const copied = @as(u64, rect.w) * @as(u64, rect.h);
    return if (copied > 0x7fff_ffff) 0x7fff_ffff else @intCast(copied);
}

pub fn remoteInputPush(event: *const RemoteInputEvent) callconv(.c) i32 {
    if (@intFromPtr(event) == 0) return remote_input_error_invalid;
    if (event.magic != remote_input_magic or event.version != remote_input_version) return remote_input_error_invalid;
    if (!validRemoteInputKind(event.kind)) return remote_input_error_invalid;
    if (remote_input_pending >= remote_input_queue_capacity) {
        remote_input_tail = (remote_input_tail + 1) % remote_input_queue_capacity;
        remote_input_pending -= 1;
        remote_input_dropped +%= 1;
    }

    var stored = event.*;
    stored.sequence = nextRemoteInputSequence();
    remote_input_queue[remote_input_head] = stored;
    remote_input_head = (remote_input_head + 1) % remote_input_queue_capacity;
    remote_input_pending += 1;
    remote_input_pushed +%= 1;
    remote_input_last = stored;
    // Publish the complete queue entry before waking the Desktop. Otherwise
    // it can poll an empty queue and sleep again without a second signal.
    desktop_events.signal();
    return 1;
}

pub fn remoteInputPoll(out: *RemoteInputEvent) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return remote_input_error_invalid;
    if (remote_input_pending == 0) {
        out.* = RemoteInputEvent{};
        return 0;
    }
    out.* = remote_input_queue[remote_input_tail];
    remote_input_tail = (remote_input_tail + 1) % remote_input_queue_capacity;
    remote_input_pending -= 1;
    remote_input_polled +%= 1;
    return 1;
}

pub fn remoteInputStatus(out: *RemoteInputStatus) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return remote_input_error_invalid;
    out.* = .{
        .capacity = @intCast(remote_input_queue_capacity),
        .pending = @intCast(remote_input_pending),
        .dropped = remote_input_dropped,
        .pushed = remote_input_pushed,
        .polled = remote_input_polled,
        .last_sequence = remote_input_last.sequence,
        .last_kind = remote_input_last.kind,
        .last_buttons = remote_input_last.buttons,
        .last_key = remote_input_last.key,
        .last_x = remote_input_last.x,
        .last_y = remote_input_last.y,
        .last_wheel = remote_input_last.wheel,
    };
    return 0;
}

fn clipboardWriteLocal(data: []const u8) i32 {
    var i: usize = 0;
    while (i < data.len) : (i += 1) clipboard_text[i] = data[i];
    clipboard_text[data.len] = 0;
    if (data.len + 1 < clipboard_text.len) @memset(clipboard_text[data.len + 1 ..], 0);
    clipboard_len = data.len;
    bumpClipboardRevision();
    return @intCast(data.len);
}

fn clipboardReadLocal(out: [*]u8, capacity: usize) i32 {
    if (clipboard_len == 0) {
        if (capacity > 0) out[0] = 0;
        return 0;
    }
    if (capacity <= clipboard_len) {
        if (capacity > 0) out[0] = 0;
        return clipboard_error_buffer_too_small;
    }
    const count = @min(clipboard_len, capacity - 1);
    var i: usize = 0;
    while (i < count) : (i += 1) out[i] = clipboard_text[i];
    out[count] = 0;
    return @intCast(count);
}

fn clipboardServiceWrite(data: []const u8) ?i32 {
    var header: services.ApiMessageHeader = .{};
    var response: [1]u8 = .{0};
    const got = clipboardServiceCall(clipboard_service_op_write, data, response[0..], &header) orelse return null;
    if (got < 0) return null;
    return header.status;
}

fn clipboardServiceRead(out: [*]u8, capacity: usize) ?i32 {
    var info = ClipboardInfo{};
    if (!clipboardServiceInfo(&info)) return null;
    const text_len: usize = @intCast(info.length);
    if (text_len == 0) {
        if (capacity > 0) out[0] = 0;
        return 0;
    }
    if (capacity <= text_len) {
        if (capacity > 0) out[0] = 0;
        return clipboard_error_buffer_too_small;
    }

    var header: services.ApiMessageHeader = .{};
    const response_capacity = @min(capacity, services.API_MAX_PAYLOAD);
    const response = out[0..response_capacity];
    const got = clipboardServiceCall(clipboard_service_op_read, "", response, &header) orelse return null;
    if (got == services.API_ERR_BUFFER_TOO_SMALL) return clipboard_error_buffer_too_small;
    if (got < 0) return null;
    if (header.status < 0) return header.status;
    const count: usize = @intCast(got);
    if (count >= capacity) {
        if (capacity > 0) out[0] = 0;
        return clipboard_error_buffer_too_small;
    }
    out[count] = 0;
    return @intCast(count);
}

fn clipboardServiceInfo(out: *ClipboardInfo) bool {
    var header: services.ApiMessageHeader = .{};
    var response: [clipboard_info_payload_size]u8 = .{0} ** clipboard_info_payload_size;
    const got = clipboardServiceCall(clipboard_service_op_info, "", response[0..], &header) orelse return false;
    if (got != @as(i32, @intCast(clipboard_info_payload_size)) or header.status != services.API_OK) return false;
    out.* = .{
        .capacity = readLe32(response[0..4]),
        .length = readLe32(response[4..8]),
        .revision = readLe32(response[8..12]),
        .flags = readLe32(response[12..16]),
    };
    return out.capacity == @as(u32, @intCast(clipboard_max_text_bytes)) and out.length <= @as(u32, @intCast(clipboard_max_text_bytes));
}

fn clipboardServiceClear() ?i32 {
    var header: services.ApiMessageHeader = .{};
    var response: [1]u8 = .{0};
    const got = clipboardServiceCall(clipboard_service_op_clear, "", response[0..], &header) orelse return null;
    if (got < 0) return null;
    return header.status;
}

fn clipboardServiceCall(op: u16, request: []const u8, response: []u8, header: *services.ApiMessageHeader) ?i32 {
    if (request.len > services.API_MAX_PAYLOAD or response.len > services.API_MAX_PAYLOAD) return null;
    var info: services.ApiInfo = .{};
    const open = services.apiOpen(clipboard_service_name, &info, 0);
    if (open != services.API_OK or info.handle == 0) return null;

    header.* = .{ .magic = 0, .version = 0 };
    const request_id_raw = services.submitRequest(info.handle, 0, op, request);
    if (request_id_raw <= 0) return null;
    const request_id: u32 = @intCast(request_id_raw);

    var waited: u64 = 0;
    while (waited <= clipboard_service_timeout_ticks) : (waited += 1) {
        const result = services.takeResponse(info.handle, request_id, header, response);
        if (result < 0) return result;
        if (result > 0 or header.magic == services.API_MAGIC) return result;
        waitOneTick("clipboard-service");
    }
    _ = services.cancelRequest(info.handle, request_id);
    return services.API_ERR_TIMEOUT;
}

fn fillKeyboardLayoutInfo(out: *KeyboardLayoutInfo, index: usize) ?void {
    const layout_name = keyboard.layoutNameAt(index) orelse return null;
    const display = keyboard.layoutDisplayAt(index) orelse return null;
    out.* = .{
        .index = @intCast(index),
        .count = @intCast(keyboard.layoutCount()),
    };
    copyFixedZ(out.name[0..], layout_name);
    copyFixedZ(out.display[0..], display);
}

fn waitOneTick(reason: []const u8) void {
    var wait = sync.TimerWait.init();
    _ = wait.waitReason(1, reason);
}

fn clearClipboardStorage() void {
    @memset(clipboard_text[0..], 0);
    clipboard_len = 0;
}

fn bumpClipboardRevision() void {
    clipboard_rev +%= 1;
    if (clipboard_rev == 0) clipboard_rev = 1;
}

fn ensureRemoteFrameCapacity(pixel_count: usize) bool {
    if (pixel_count == 0) return false;
    if (remote_frame_capacity_pixels >= pixel_count and remote_frame_pixels != null) return true;
    if (pixel_count > (~@as(usize, 0)) / @sizeOf(u32)) return false;
    const bytes = pixel_count * @sizeOf(u32);
    const memory = heap.alloc(bytes, @alignOf(u32)) orelse return false;
    if (remote_frame_memory) |old| {
        if (remote_frame_mapped) {
            // 0.56.38: Mapping ausgegeben -> alten Puffer zurueckhalten
            // statt freigeben (Konsument koennte noch lesen). Der Ring
            // gibt erst den aeltesten Zurueckgehaltenen frei.
            if (remote_frame_retired[remote_frame_retired_next]) |oldest| _ = heap.free(oldest);
            remote_frame_retired[remote_frame_retired_next] = old;
            remote_frame_retired_next = (remote_frame_retired_next + 1) % remote_frame_retired.len;
        } else {
            _ = heap.free(old);
        }
    }
    const aligned: [*]align(@alignOf(u32)) u8 = @alignCast(memory.ptr);
    const raw: [*]u32 = @ptrCast(aligned);
    remote_frame_memory = memory;
    remote_frame_pixels = raw[0..pixel_count];
    remote_frame_capacity_pixels = pixel_count;
    remote_frame_ready = false;
    remote_frame_info = .{};
    remote_frame_generation +%= 1;
    return true;
}

// 0.56.38: Shared-Frame-Mapping fuer den Doppelkopie-freien Lesepfad
// (Befund 9.2). remoteFrameRead bleibt als Kompat-Pfad vollstaendig
// erhalten; Konsumenten validieren generation/capacity pro Frame.
pub fn remoteFrameMap(out: *RemoteFrameMapInfo) callconv(.c) i32 {
    if (@intFromPtr(out) == 0) return remote_frame_error_invalid;
    out.* = .{};
    const pixels = remote_frame_pixels orelse return remote_frame_error_unavailable;
    if (!remote_frame_ready) return remote_frame_error_unavailable;
    remote_frame_mapped = true;
    out.pixels_addr = @intFromPtr(pixels.ptr);
    out.capacity_pixels = remote_frame_capacity_pixels;
    out.generation = remote_frame_generation;
    return 0;
}

fn bumpRemoteFrameRevision() u32 {
    var next = remote_frame_info.revision +% 1;
    if (next == 0) next = 1;
    return next;
}

fn validRemoteInputKind(kind: u32) bool {
    return kind == remote_input_kind_key_down or
        kind == remote_input_kind_key_up or
        kind == remote_input_kind_mouse_move or
        kind == remote_input_kind_mouse_buttons or
        kind == remote_input_kind_mouse_wheel;
}

fn nextRemoteInputSequence() u32 {
    remote_input_sequence +%= 1;
    if (remote_input_sequence == 0) remote_input_sequence = 1;
    return remote_input_sequence;
}

fn normalizeRemoteRect(info: *const RemoteFrameInfo, force_full: bool) RemoteRect {
    if (force_full or info.dirty_w == 0 or info.dirty_h == 0) {
        return .{ .x = 0, .y = 0, .w = info.width, .h = info.height };
    }

    const width_i: i64 = @intCast(info.width);
    const height_i: i64 = @intCast(info.height);
    var left: i64 = @intCast(info.dirty_x);
    var top: i64 = @intCast(info.dirty_y);
    var right = left + @as(i64, @intCast(info.dirty_w));
    var bottom = top + @as(i64, @intCast(info.dirty_h));
    if (left < 0) left = 0;
    if (top < 0) top = 0;
    if (right > width_i) right = width_i;
    if (bottom > height_i) bottom = height_i;
    if (right <= left or bottom <= top) {
        return .{ .x = 0, .y = 0, .w = info.width, .h = info.height };
    }
    return .{
        .x = @intCast(left),
        .y = @intCast(top),
        .w = @intCast(right - left),
        .h = @intCast(bottom - top),
    };
}

fn copyRemoteFrameRect(dest: []u32, source: []const u32, width: u32, source_stride: u32, rect: RemoteRect) void {
    const dst_stride: usize = @intCast(width);
    const src_stride: usize = @intCast(source_stride);
    const x: usize = @intCast(rect.x);
    const count: usize = @intCast(rect.w);
    var row: u32 = 0;
    while (row < rect.h) : (row += 1) {
        const y: usize = @intCast(rect.y + row);
        const dst_offset = y * dst_stride + x;
        const src_offset = y * src_stride + x;
        @memcpy(dest[dst_offset .. dst_offset + count], source[src_offset .. src_offset + count]);
    }
}

fn copyZ(ptr: [*:0]const u8, out: []u8) ?[]const u8 {
    var len: usize = 0;
    while (len < out.len and ptr[len] != 0) : (len += 1) out[len] = ptr[len];
    if (len == out.len) return null;
    return out[0..len];
}

fn copyFixedZ(out: []u8, value: []const u8) void {
    @memset(out, 0);
    if (out.len == 0) return;
    const count = @min(value.len, out.len - 1);
    if (count > 0) @memcpy(out[0..count], value[0..count]);
}

fn readLe32(data: []const u8) u32 {
    return @as(u32, data[0]) |
        (@as(u32, data[1]) << 8) |
        (@as(u32, data[2]) << 16) |
        (@as(u32, data[3]) << 24);
}
