// 0.56.32: Service-IPC (vormals kernel/ipc.zig) - Kanal-basiertes IPC
// fuer die Netz-Service-Bruecke. Struktur-Umzug ohne Verhaltensaenderung;
// kernel/ipc.zig bleibt als Kompat-Shim bestehen.
const k = @import("log.zig");

pub const MAX_CHANNELS: usize = 16;
pub const QUEUE_DEPTH: usize = 8;
// 0.56.37: 1024 -> 4096 (Durchsatz-Hebel aus der 0.56.25-Diagnose:
// jeder net-service-Chunk ist ein Service-Roundtrip; 4x groessere
// Messages = ~4x weniger Roundtrips pro grossem Transfer). Statischer
// Speicher waechst um ~384 KB (16 Kanaele x 8 Slots), Stack-Puffer der
// Nutzer bleiben mit 4 KB unter den 64-KB-Task-Stacks (Guard-Pages
// 0.56.15). MUSS mit SDK abi.ipc_max_message_size uebereinstimmen.
pub const MAX_MESSAGE_SIZE: usize = 4096;
pub const CHANNEL_ECHO: u32 = 1;
pub const CHANNEL_NET_DHCP: u32 = 2;
pub const CHANNEL_NET_DNS: u32 = 3;
pub const CHANNEL_NET_TCP: u32 = 4;
pub const CHANNEL_NET_UDP: u32 = 5;

pub const ServiceHandler = *const fn (channel_id: u32, request: []const u8, response: []u8) i32;

const Message = struct {
    len: u16 = 0,
    data: [MAX_MESSAGE_SIZE]u8 = .{0} ** MAX_MESSAGE_SIZE,
};

const Channel = struct {
    active: bool = false,
    id: u32 = 0,
    head: usize = 0,
    tail: usize = 0,
    count: usize = 0,
    opens: u64 = 0,
    closes: u64 = 0,
    sends: u64 = 0,
    receives: u64 = 0,
    drops: u64 = 0,
    service_name: []const u8 = "",
    handler: ?ServiceHandler = null,
    queue: [QUEUE_DEPTH]Message = .{Message{}} ** QUEUE_DEPTH,
};

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

var initialized = false;
var channels: [MAX_CHANNELS]Channel = .{Channel{}} ** MAX_CHANNELS;
var total_sends: u64 = 0;
var total_receives: u64 = 0;
var total_drops: u64 = 0;
var total_errors: u64 = 0;
var echo_tests: u64 = 0;

pub fn init() void {
    if (initialized) return;
    initialized = true;
    _ = open(CHANNEL_ECHO);
}

pub fn registerService(channel_id: u32, name: []const u8, handler: ServiceHandler) i32 {
    const idx = index(channel_id) orelse return fail();
    if (!channels[idx].active) _ = open(channel_id);
    channels[idx].service_name = name;
    channels[idx].handler = handler;
    return 0;
}

pub fn open(channel_id: u32) i32 {
    const idx = index(channel_id) orelse return fail();
    var ch = &channels[idx];
    if (!ch.active) {
        ch.* = .{ .active = true, .id = channel_id };
    }
    ch.opens += 1;
    return @intCast(channel_id);
}

pub fn close(channel_id: u32) i32 {
    const idx = index(channel_id) orelse return fail();
    var ch = &channels[idx];
    if (!ch.active) return fail();
    ch.closes += 1;
    return 0;
}

pub fn poll(channel_id: u32) i32 {
    const idx = index(channel_id) orelse return fail();
    const ch = &channels[idx];
    if (!ch.active) return fail();
    return @intCast(ch.count);
}

pub fn send(channel_id: u32, payload: []const u8) i32 {
    if (payload.len > MAX_MESSAGE_SIZE) return fail();
    const idx = index(channel_id) orelse return fail();
    if (!channels[idx].active) _ = open(channel_id);
    var ch = &channels[idx];
    if (ch.handler) |handler| {
        var response: [MAX_MESSAGE_SIZE]u8 = .{0} ** MAX_MESSAGE_SIZE;
        const produced = handler(channel_id, payload, response[0..]);
        if (produced < 0 or produced > @as(i32, @intCast(MAX_MESSAGE_SIZE))) return fail();
        if (enqueue(ch, response[0..@intCast(produced)]) < 0) return fail();
        ch.sends += 1;
        total_sends += 1;
        return @intCast(payload.len);
    }
    if (enqueue(ch, payload) < 0) return fail();
    ch.sends += 1;
    total_sends += 1;
    return @intCast(payload.len);
}

fn enqueue(ch: *Channel, payload: []const u8) i32 {
    if (ch.count >= QUEUE_DEPTH) {
        ch.drops += 1;
        total_drops += 1;
        return fail();
    }
    var msg = &ch.queue[ch.tail];
    msg.len = @intCast(payload.len);
    if (payload.len != 0) @memcpy(msg.data[0..payload.len], payload);
    ch.tail = (ch.tail + 1) % QUEUE_DEPTH;
    ch.count += 1;
    return @intCast(payload.len);
}

pub fn recv(channel_id: u32, out: []u8) i32 {
    const idx = index(channel_id) orelse return fail();
    var ch = &channels[idx];
    if (!ch.active) return fail();
    if (ch.count == 0) return 0;
    const msg = &ch.queue[ch.head];
    if (out.len < msg.len) return fail();
    if (msg.len != 0) @memcpy(out[0..msg.len], msg.data[0..msg.len]);
    ch.head = (ch.head + 1) % QUEUE_DEPTH;
    ch.count -= 1;
    ch.receives += 1;
    total_receives += 1;
    return @intCast(msg.len);
}

pub fn echoSmoke() bool {
    const payload = "R4IPC-ECHO";
    var out: [MAX_MESSAGE_SIZE]u8 = undefined;
    if (send(CHANNEL_ECHO, payload) != @as(i32, @intCast(payload.len))) return false;
    const got = recv(CHANNEL_ECHO, out[0..]);
    if (got != @as(i32, @intCast(payload.len))) return false;
    if (!memEql(out[0..payload.len], payload)) return false;
    echo_tests += 1;
    return true;
}

pub fn summary(out: *Summary) void {
    out.* = .{
        .active_channels = activeCount(),
        .sends = total_sends,
        .receives = total_receives,
        .drops = total_drops,
        .errors = total_errors,
        .echo_tests = echo_tests,
    };
}

pub fn channelInfo(channel_id: u32, out: *ChannelInfo) i32 {
    const idx = index(channel_id) orelse return fail();
    const ch = &channels[idx];
    out.* = .{
        .id = channel_id,
        .active = if (ch.active) 1 else 0,
        .queued = @intCast(ch.count),
        .has_handler = if (ch.handler != null) 1 else 0,
        .opens = ch.opens,
        .closes = ch.closes,
        .sends = ch.sends,
        .receives = ch.receives,
        .drops = ch.drops,
    };
    copyFixed(out.name[0..], if (ch.service_name.len != 0) ch.service_name else if (channel_id == CHANNEL_ECHO) "echo" else "");
    return if (ch.active) 1 else 0;
}

pub fn dumpStatus() void {
    var s: Summary = .{};
    summary(&s);
    k.puts("IPC service bus\r\n");
    k.puts("  Model: trusted mailbox/message-queue, no permissions\r\n");
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
    var i: usize = 0;
    while (i < channels.len) : (i += 1) {
        const ch = &channels[i];
        if (!ch.active) continue;
        k.puts("  channel ");
        k.putDec(ch.id);
        if (ch.id == CHANNEL_ECHO) k.puts(" echo");
        if (ch.service_name.len != 0) {
            k.puts(" service=");
            k.puts(ch.service_name);
        }
        k.puts(": queued=");
        k.putDec(ch.count);
        k.puts(" opens=");
        k.putDec(ch.opens);
        k.puts(" closes=");
        k.putDec(ch.closes);
        k.puts(" sends=");
        k.putDec(ch.sends);
        k.puts(" receives=");
        k.putDec(ch.receives);
        k.puts(" drops=");
        k.putDec(ch.drops);
        k.puts("\r\n");
    }
}

fn copyFixed(out: []u8, text: []const u8) void {
    @memset(out, 0);
    const len = if (text.len < out.len - 1) text.len else out.len - 1;
    if (len != 0) @memcpy(out[0..len], text[0..len]);
}

fn index(channel_id: u32) ?usize {
    if (channel_id == 0 or channel_id > MAX_CHANNELS) return null;
    return @intCast(channel_id - 1);
}

fn activeCount() u32 {
    var count: u32 = 0;
    for (&channels) |*ch| {
        if (ch.active) count += 1;
    }
    return count;
}

fn fail() i32 {
    total_errors += 1;
    return -1;
}

fn memEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}
