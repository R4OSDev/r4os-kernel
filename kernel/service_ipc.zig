// Canonical channel/worker ownership is native. This facade keeps the
// unchanged typed Zig consumers and handler ABI; no second mutable state.
const k = @import("log.zig");
const sync = @import("../sched/sync.zig");
pub const MAX_CHANNELS: usize = 16;
pub const QUEUE_DEPTH: usize = 8;
pub const MAX_MESSAGE_SIZE: usize = 4096;
pub const WAIT_FOREVER: u64 = sync.WAIT_FOREVER;
pub const CHANNEL_ECHO: u32 = 1;
pub const CHANNEL_NET_DHCP: u32 = 2;
pub const CHANNEL_NET_DNS: u32 = 3;
pub const CHANNEL_NET_TCP: u32 = 4;
pub const CHANNEL_NET_UDP: u32 = 5;
pub const ServiceHandler = *const fn (channel_id: u32, request: []const u8, response: []u8) i32;
pub const Summary = extern struct {
    max_channels: u32 = @intCast(MAX_CHANNELS),
    active_channels: u32 = 0,
    max_message_size: u32 = @intCast(MAX_MESSAGE_SIZE),
    queue_depth: u32 = @intCast(QUEUE_DEPTH),
    sends: u64 = 0,
    receives: u64 = 0,
    drops: u64 = 0,
    errors: u64 = 0,
    echo_tests: u64 = 0,
};

pub const ChannelInfo = extern struct {
    id: u32 = 0,
    active: u32 = 0,
    queued: u32 = 0,
    queue_depth: u32 = @intCast(QUEUE_DEPTH),
    max_message_size: u32 = @intCast(MAX_MESSAGE_SIZE),
    has_handler: u32 = 0,
    reserved0: u32 = 0,
    reserved1: u32 = 0,
    opens: u64 = 0,
    closes: u64 = 0,
    sends: u64 = 0,
    receives: u64 = 0,
    drops: u64 = 0,
    name: [16]u8 = .{0} ** 16,
};

pub const PerformanceSummary = extern struct {
    worker_started: u32 = 0,
    worker_task_id: u32 = 0,
    active_channels: u32 = 0,
    queue_used: u32 = 0,
    queue_ready: u32 = 0,
    queue_running: u32 = 0,
    queue_limit: u32 = 0,
    reserved0: u32 = 0,
    send_calls: u64 = 0,
    recv_calls: u64 = 0,
    request_calls: u64 = 0,
    handler_queued: u64 = 0,
    handler_started: u64 = 0,
    handler_completed: u64 = 0,
    handler_failures: u64 = 0,
    handler_direct: u64 = 0,
    handler_waits: u64 = 0,
    handler_wait_timeouts: u64 = 0,
    handler_queue_ns: u64 = 0,
    handler_queue_max_ns: u64 = 0,
    handler_run_ns: u64 = 0,
    handler_run_max_ns: u64 = 0,
    handler_e2e_ns: u64 = 0,
    handler_e2e_max_ns: u64 = 0,
    timing_unavailable: u64 = 0,
    request_bytes: u64 = 0,
    response_bytes: u64 = 0,
    payload_copy_bytes: u64 = 0,
    payload_clear_bytes: u64 = 0,
    queue_full: u64 = 0,
    queue_empty: u64 = 0,
    admission_waits: u64 = 0,
    admission_timeouts: u64 = 0,
    recv_buffer_small: u64 = 0,
    response_searches: u64 = 0,
    response_search_slots: u64 = 0,
    stale_drops: u64 = 0,
    lock_contentions: u64 = 0,
    irq_denied: u64 = 0,
    worker_idle_waits: u64 = 0,
    worker_wakes: u64 = 0,
};
const NativeBytes = extern struct { data: [*]const u8, len: usize };
fn nativeBytes(value: []const u8) NativeBytes {
    return .{ .data = value.ptr, .len = value.len };
}
extern fn r4os_rust_ipc_control(u8, u32) callconv(.c) i32;
extern fn r4os_rust_ipc_register(u32, NativeBytes, usize) callconv(.c) i32;
extern fn r4os_rust_ipc_io(u8, u32, NativeBytes, ?[*]u8, usize, u64) callconv(.c) i32;
extern fn r4os_rust_ipc_summary(*Summary) callconv(.c) i32;
extern fn r4os_rust_ipc_channel(u32, *ChannelInfo) callconv(.c) i32;
extern fn r4os_rust_ipc_performance(u32, bool, *PerformanceSummary) callconv(.c) i32;
pub fn init() void {
    _ = r4os_rust_ipc_control(0, 0);
}
pub fn startRuntimeWorker() bool {
    return r4os_rust_ipc_control(1, 0) != 0;
}
pub fn registerService(channel_id: u32, name: []const u8, handler: ServiceHandler) i32 {
    return r4os_rust_ipc_register(channel_id, nativeBytes(name), @intFromPtr(handler));
}
pub fn open(channel_id: u32) i32 {
    return r4os_rust_ipc_control(2, channel_id);
}
pub fn close(channel_id: u32) i32 {
    return r4os_rust_ipc_control(3, channel_id);
}
pub fn poll(channel_id: u32) i32 {
    return r4os_rust_ipc_control(4, channel_id);
}
pub fn send(channel_id: u32, payload: []const u8) i32 {
    return r4os_rust_ipc_io(0, channel_id, nativeBytes(payload), null, 0, 0);
}
pub fn request(channel_id: u32, payload: []const u8, out: []u8, timeout_ticks: u64) i32 {
    return r4os_rust_ipc_io(1, channel_id, nativeBytes(payload), out.ptr, out.len, timeout_ticks);
}
pub fn recv(channel_id: u32, out: []u8) i32 {
    return r4os_rust_ipc_io(2, channel_id, nativeBytes(""), out.ptr, out.len, 0);
}
pub fn echoSmoke() bool {
    return r4os_rust_ipc_control(5, 0) != 0;
}
pub fn summary(out: *Summary) i32 {
    return r4os_rust_ipc_summary(out);
}
pub fn channelInfo(channel_id: u32, out: *ChannelInfo) i32 {
    return r4os_rust_ipc_channel(channel_id, out);
}
pub fn performanceSummary() PerformanceSummary {
    var out: PerformanceSummary = undefined;
    _ = r4os_rust_ipc_performance(0, false, &out);
    return out;
}
pub fn performanceSummaryFor(channel_id: u32, out: *PerformanceSummary) i32 {
    return r4os_rust_ipc_performance(channel_id, true, out);
}
// This stateless invoker keeps the handler's original Zig calling convention.
export fn r4os_zig_ipc_handler(address: usize, id: u32, input: NativeBytes, output: [*]u8, capacity: usize) callconv(.c) i32 {
    const handler: ServiceHandler = @ptrFromInt(address);
    return handler(id, if (input.len == 0) "" else input.data[0..input.len], output[0..capacity]);
}
pub fn dumpStatus() void {
    var s: Summary = .{};
    _ = summary(&s);
    k.puts("IPC service bus\r\n");
    k.puts("  Model: synchronized channel queue, isolated handler worker\r\n");
    k.puts("  Limits: channels=");
    k.putDec(MAX_CHANNELS);
    k.puts(" queue_depth=");
    k.putDec(QUEUE_DEPTH);
    k.puts(" message_bytes=");
    k.putDec(MAX_MESSAGE_SIZE);
    k.puts("\r\n");
    k.puts("  Counters: active=");
    k.putDec(s.active_channels);
    k.puts(" sends=");
    k.putDec(s.sends);
    k.puts(" receives=");
    k.putDec(s.receives);
    k.puts(" drops=");
    k.putDec(s.drops);
    k.puts(" errors=");
    k.putDec(s.errors);
    k.puts(" echo_tests=");
    k.putDec(s.echo_tests);
    k.puts("\r\n");
    var channel_id: u32 = 1;
    while (channel_id <= MAX_CHANNELS) : (channel_id += 1) {
        var info: ChannelInfo = .{};
        if (channelInfo(channel_id, &info) <= 0) continue;
        k.puts("  channel ");
        k.putDec(info.id);
        if (info.name[0] != 0) {
            k.puts(" service=");
            k.puts(fixedText(info.name[0..]));
        }
        k.puts(": queued=");
        k.putDec(info.queued);
        k.puts(" opens=");
        k.putDec(info.opens);
        k.puts(" closes=");
        k.putDec(info.closes);
        k.puts(" sends=");
        k.putDec(info.sends);
        k.puts(" receives=");
        k.putDec(info.receives);
        k.puts(" drops=");
        k.putDec(info.drops);
        k.puts("\r\n");
    }
}

fn fixedText(bytes: []const u8) []const u8 {
    var len: usize = 0;
    while (len < bytes.len and bytes[len] != 0) : (len += 1) {}
    return bytes[0..len];
}

comptime {
    if (@offsetOf(Summary, "max_channels") != 0) @compileError("Native IPC Summary.max_channels offset drift");
    if (@offsetOf(Summary, "active_channels") != 4) @compileError("Native IPC Summary.active_channels offset drift");
    if (@offsetOf(Summary, "max_message_size") != 8) @compileError("Native IPC Summary.max_message_size offset drift");
    if (@offsetOf(Summary, "queue_depth") != 12) @compileError("Native IPC Summary.queue_depth offset drift");
    if (@offsetOf(Summary, "sends") != 16) @compileError("Native IPC Summary.sends offset drift");
    if (@offsetOf(Summary, "receives") != 24) @compileError("Native IPC Summary.receives offset drift");
    if (@offsetOf(Summary, "drops") != 32) @compileError("Native IPC Summary.drops offset drift");
    if (@offsetOf(Summary, "errors") != 40) @compileError("Native IPC Summary.errors offset drift");
    if (@offsetOf(Summary, "echo_tests") != 48) @compileError("Native IPC Summary.echo_tests offset drift");
    if (@sizeOf(Summary) != 56 or @alignOf(Summary) != 8) @compileError("Native IPC Summary layout drift");
    if (@offsetOf(ChannelInfo, "id") != 0) @compileError("Native IPC ChannelInfo.id offset drift");
    if (@offsetOf(ChannelInfo, "active") != 4) @compileError("Native IPC ChannelInfo.active offset drift");
    if (@offsetOf(ChannelInfo, "queued") != 8) @compileError("Native IPC ChannelInfo.queued offset drift");
    if (@offsetOf(ChannelInfo, "queue_depth") != 12) @compileError("Native IPC ChannelInfo.queue_depth offset drift");
    if (@offsetOf(ChannelInfo, "max_message_size") != 16) @compileError("Native IPC ChannelInfo.max_message_size offset drift");
    if (@offsetOf(ChannelInfo, "has_handler") != 20) @compileError("Native IPC ChannelInfo.has_handler offset drift");
    if (@offsetOf(ChannelInfo, "reserved0") != 24) @compileError("Native IPC ChannelInfo.reserved0 offset drift");
    if (@offsetOf(ChannelInfo, "reserved1") != 28) @compileError("Native IPC ChannelInfo.reserved1 offset drift");
    if (@offsetOf(ChannelInfo, "opens") != 32) @compileError("Native IPC ChannelInfo.opens offset drift");
    if (@offsetOf(ChannelInfo, "closes") != 40) @compileError("Native IPC ChannelInfo.closes offset drift");
    if (@offsetOf(ChannelInfo, "sends") != 48) @compileError("Native IPC ChannelInfo.sends offset drift");
    if (@offsetOf(ChannelInfo, "receives") != 56) @compileError("Native IPC ChannelInfo.receives offset drift");
    if (@offsetOf(ChannelInfo, "drops") != 64) @compileError("Native IPC ChannelInfo.drops offset drift");
    if (@offsetOf(ChannelInfo, "name") != 72) @compileError("Native IPC ChannelInfo.name offset drift");
    if (@sizeOf(ChannelInfo) != 88 or @alignOf(ChannelInfo) != 8) @compileError("Native IPC ChannelInfo layout drift");
    if (@offsetOf(PerformanceSummary, "worker_started") != 0) @compileError("Native IPC PerformanceSummary.worker_started offset drift");
    if (@offsetOf(PerformanceSummary, "worker_task_id") != 4) @compileError("Native IPC PerformanceSummary.worker_task_id offset drift");
    if (@offsetOf(PerformanceSummary, "active_channels") != 8) @compileError("Native IPC PerformanceSummary.active_channels offset drift");
    if (@offsetOf(PerformanceSummary, "queue_used") != 12) @compileError("Native IPC PerformanceSummary.queue_used offset drift");
    if (@offsetOf(PerformanceSummary, "queue_ready") != 16) @compileError("Native IPC PerformanceSummary.queue_ready offset drift");
    if (@offsetOf(PerformanceSummary, "queue_running") != 20) @compileError("Native IPC PerformanceSummary.queue_running offset drift");
    if (@offsetOf(PerformanceSummary, "queue_limit") != 24) @compileError("Native IPC PerformanceSummary.queue_limit offset drift");
    if (@offsetOf(PerformanceSummary, "reserved0") != 28) @compileError("Native IPC PerformanceSummary.reserved0 offset drift");
    if (@offsetOf(PerformanceSummary, "send_calls") != 32) @compileError("Native IPC PerformanceSummary.send_calls offset drift");
    if (@offsetOf(PerformanceSummary, "recv_calls") != 40) @compileError("Native IPC PerformanceSummary.recv_calls offset drift");
    if (@offsetOf(PerformanceSummary, "request_calls") != 48) @compileError("Native IPC PerformanceSummary.request_calls offset drift");
    if (@offsetOf(PerformanceSummary, "handler_queued") != 56) @compileError("Native IPC PerformanceSummary.handler_queued offset drift");
    if (@offsetOf(PerformanceSummary, "handler_started") != 64) @compileError("Native IPC PerformanceSummary.handler_started offset drift");
    if (@offsetOf(PerformanceSummary, "handler_completed") != 72) @compileError("Native IPC PerformanceSummary.handler_completed offset drift");
    if (@offsetOf(PerformanceSummary, "handler_failures") != 80) @compileError("Native IPC PerformanceSummary.handler_failures offset drift");
    if (@offsetOf(PerformanceSummary, "handler_direct") != 88) @compileError("Native IPC PerformanceSummary.handler_direct offset drift");
    if (@offsetOf(PerformanceSummary, "handler_waits") != 96) @compileError("Native IPC PerformanceSummary.handler_waits offset drift");
    if (@offsetOf(PerformanceSummary, "handler_wait_timeouts") != 104) @compileError("Native IPC PerformanceSummary.handler_wait_timeouts offset drift");
    if (@offsetOf(PerformanceSummary, "handler_queue_ns") != 112) @compileError("Native IPC PerformanceSummary.handler_queue_ns offset drift");
    if (@offsetOf(PerformanceSummary, "handler_queue_max_ns") != 120) @compileError("Native IPC PerformanceSummary.handler_queue_max_ns offset drift");
    if (@offsetOf(PerformanceSummary, "handler_run_ns") != 128) @compileError("Native IPC PerformanceSummary.handler_run_ns offset drift");
    if (@offsetOf(PerformanceSummary, "handler_run_max_ns") != 136) @compileError("Native IPC PerformanceSummary.handler_run_max_ns offset drift");
    if (@offsetOf(PerformanceSummary, "handler_e2e_ns") != 144) @compileError("Native IPC PerformanceSummary.handler_e2e_ns offset drift");
    if (@offsetOf(PerformanceSummary, "handler_e2e_max_ns") != 152) @compileError("Native IPC PerformanceSummary.handler_e2e_max_ns offset drift");
    if (@offsetOf(PerformanceSummary, "timing_unavailable") != 160) @compileError("Native IPC PerformanceSummary.timing_unavailable offset drift");
    if (@offsetOf(PerformanceSummary, "request_bytes") != 168) @compileError("Native IPC PerformanceSummary.request_bytes offset drift");
    if (@offsetOf(PerformanceSummary, "response_bytes") != 176) @compileError("Native IPC PerformanceSummary.response_bytes offset drift");
    if (@offsetOf(PerformanceSummary, "payload_copy_bytes") != 184) @compileError("Native IPC PerformanceSummary.payload_copy_bytes offset drift");
    if (@offsetOf(PerformanceSummary, "payload_clear_bytes") != 192) @compileError("Native IPC PerformanceSummary.payload_clear_bytes offset drift");
    if (@offsetOf(PerformanceSummary, "queue_full") != 200) @compileError("Native IPC PerformanceSummary.queue_full offset drift");
    if (@offsetOf(PerformanceSummary, "queue_empty") != 208) @compileError("Native IPC PerformanceSummary.queue_empty offset drift");
    if (@offsetOf(PerformanceSummary, "admission_waits") != 216) @compileError("Native IPC PerformanceSummary.admission_waits offset drift");
    if (@offsetOf(PerformanceSummary, "admission_timeouts") != 224) @compileError("Native IPC PerformanceSummary.admission_timeouts offset drift");
    if (@offsetOf(PerformanceSummary, "recv_buffer_small") != 232) @compileError("Native IPC PerformanceSummary.recv_buffer_small offset drift");
    if (@offsetOf(PerformanceSummary, "response_searches") != 240) @compileError("Native IPC PerformanceSummary.response_searches offset drift");
    if (@offsetOf(PerformanceSummary, "response_search_slots") != 248) @compileError("Native IPC PerformanceSummary.response_search_slots offset drift");
    if (@offsetOf(PerformanceSummary, "stale_drops") != 256) @compileError("Native IPC PerformanceSummary.stale_drops offset drift");
    if (@offsetOf(PerformanceSummary, "lock_contentions") != 264) @compileError("Native IPC PerformanceSummary.lock_contentions offset drift");
    if (@offsetOf(PerformanceSummary, "irq_denied") != 272) @compileError("Native IPC PerformanceSummary.irq_denied offset drift");
    if (@offsetOf(PerformanceSummary, "worker_idle_waits") != 280) @compileError("Native IPC PerformanceSummary.worker_idle_waits offset drift");
    if (@offsetOf(PerformanceSummary, "worker_wakes") != 288) @compileError("Native IPC PerformanceSummary.worker_wakes offset drift");
    if (@sizeOf(PerformanceSummary) != 296 or @alignOf(PerformanceSummary) != 8) @compileError("Native IPC PerformanceSummary layout drift");
    if (@sizeOf(NativeBytes) != 16 or @offsetOf(NativeBytes, "len") != 8) @compileError("Native IPC bytes layout drift");
}
