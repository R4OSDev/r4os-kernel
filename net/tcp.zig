pub const IPV4_PROTOCOL: u8 = 6;
pub const HEADER_SIZE: usize = 20;
pub const MAX_CONNECTIONS: usize = 16;
pub const MAX_LISTENERS: usize = 4;
pub const BUFFER_SIZE: usize = 256 * 1024;
pub const MAX_ADVERTISED_WINDOW: u16 = 0xFFFF;
pub const OUT_OF_ORDER_SLOT_COUNT: usize = 48;
pub const OUT_OF_ORDER_SLOT_SIZE: usize = 2048;
// 0.56.20: Retransmit-Kopie auf Segmentgroesse gedeckelt (Befund 13.1.4) -
// Sendesegmente sind MSS-gebunden, 256K pro Slot waren reine Verschwendung.
pub const LAST_PAYLOAD_SIZE: usize = 2048;
// 0.56.20: Frist fuer TIME_WAIT-/LAST_ACK-/FIN_WAIT-Slots (Befund 13.1.3);
// gleicher Wert wie timing.DEFAULT_TCP_TIME_WAIT_TICKS.
pub const TIME_WAIT_TICKS: u64 = @import("timing.zig").msToTicks(3_000);
// 0.56.20: Groessen-Nachweis fuer Boot-Log/Diagnose (Befund 13.1.4).
pub fn connectionSlotBytes() usize {
    return @sizeOf(Connection);
}

pub const FLAG_FIN: u16 = 0x001;
pub const FLAG_SYN: u16 = 0x002;
pub const FLAG_RST: u16 = 0x004;
pub const FLAG_PSH: u16 = 0x008;
pub const FLAG_ACK: u16 = 0x010;

pub const State = enum(u8) {
    closed = 0,
    syn_sent = 1,
    established = 2,
    fin_wait = 3,
    syn_received = 4,
    // 0.56.20: Minimal-Teardown (Befund 13.1.3).
    last_ack = 5,
    time_wait = 6,
};

pub const SegmentView = struct {
    source_ip: [4]u8,
    dest_ip: [4]u8,
    source_port: u16,
    dest_port: u16,
    seq: u32,
    ack: u32,
    flags: u16,
    window: u16,
    payload: []const u8,
};

pub const ConnectionInfo = r4x_api.TcpConnectionInfo;
pub const Summary = r4x_api.TcpSummary;

pub const CleanupResult = struct {
    connections: u32 = 0,
    listeners: u32 = 0,
};

pub const Stats = struct {
    rx_segments: u64 = 0,
    tx_segments: u64 = 0,
    syn_tx: u64 = 0,
    synack_rx: u64 = 0,
    synack_tx: u64 = 0,
    ack_tx: u64 = 0,
    data_tx: u64 = 0,
    data_rx: u64 = 0,
    fin_tx: u64 = 0,
    rst_rx: u64 = 0,
    listen_syn_rx: u64 = 0,
    accepts: u64 = 0,
    retransmits: u64 = 0,
    rx_drops: u64 = 0,
    malformed: u64 = 0,
    checksum_errors: u64 = 0,
    timeouts: u64 = 0,
    self_tests: u64 = 0,
    last_flags: u16 = 0,
    last_source_port: u16 = 0,
    last_dest_port: u16 = 0,
    last_seq: u32 = 0,
    last_ack: u32 = 0,
    last_payload_len: u32 = 0,
    last_error: []const u8 = "none",
};

pub const SendPlan = struct {
    local_port: u16,
    remote_port: u16,
    remote_ip: [4]u8,
    seq: u32,
    ack: u32,
    rx_window: u16,
};

pub const RetransmitPlan = struct {
    local_port: u16,
    remote_port: u16,
    remote_ip: [4]u8,
    seq: u32,
    ack: u32,
    flags: u16,
    rx_window: u16,
    payload: []const u8,
};

const Connection = struct {
    used: bool = false,
    id: u32 = 0,
    state: State = .closed,
    local_port: u16 = 0,
    remote_port: u16 = 0,
    remote_ip: [4]u8 = .{0} ** 4,
    seq: u32 = 0,
    ack: u32 = 0,
    tx_bytes: u64 = 0,
    rx_bytes: u64 = 0,
    retransmits: u32 = 0,
    tx_window: u32 = 0,
    tx_ack: u32 = 0,
    rx_drops: u32 = 0,
    accepted_claimed: bool = false,
    time_wait_until: u64 = 0,
    rx_head: usize = 0,
    rx_len: usize = 0,
    rx: [BUFFER_SIZE]u8 = .{0} ** BUFFER_SIZE,
    last_valid: bool = false,
    last_seq: u32 = 0,
    last_ack: u32 = 0,
    last_flags: u16 = 0,
    last_len: usize = 0,
    last_payload: [LAST_PAYLOAD_SIZE]u8 = .{0} ** LAST_PAYLOAD_SIZE,
    out_of_order: [OUT_OF_ORDER_SLOT_COUNT]OutOfOrderSegment = .{OutOfOrderSegment{}} ** OUT_OF_ORDER_SLOT_COUNT,
};

const OutOfOrderSegment = struct {
    valid: bool = false,
    seq: u32 = 0,
    len: usize = 0,
    payload: [OUT_OF_ORDER_SLOT_SIZE]u8 = .{0} ** OUT_OF_ORDER_SLOT_SIZE,
};

const Listener = struct {
    used: bool = false,
    port: u16 = 0,
};

var stats: Stats = .{};
var connections: [MAX_CONNECTIONS]Connection = .{Connection{}} ** MAX_CONNECTIONS;
var listeners: [MAX_LISTENERS]Listener = .{Listener{}} ** MAX_LISTENERS;
var next_id: u32 = 1;
var next_port: u16 = 49152;

pub fn reset() void {
    stats = .{};
    connections = .{Connection{}} ** MAX_CONNECTIONS;
    listeners = .{Listener{}} ** MAX_LISTENERS;
    next_id = 1;
    next_port = 49152;
}

pub fn getStats() Stats {
    return stats;
}

pub fn summary(out: *Summary) void {
    out.* = .{
        // Public payload defaults are intentionally neutral. Runtime limits
        // must therefore be populated here instead of depending on the old
        // kernel-local struct initializers removed by the generated ABI.
        .max_connections = @intCast(MAX_CONNECTIONS),
        .active_connections = activeCount(),
        .buffer_size = @intCast(BUFFER_SIZE),
        .active_listeners = listenerCount(),
        .syn_tx = stats.syn_tx,
        .synack_rx = stats.synack_rx,
        .synack_tx = stats.synack_tx,
        .ack_tx = stats.ack_tx,
        .data_tx = stats.data_tx,
        .data_rx = stats.data_rx,
        .fin_tx = stats.fin_tx,
        .rst_rx = stats.rst_rx,
        .listen_syn_rx = stats.listen_syn_rx,
        .accepts = stats.accepts,
        .retransmits = stats.retransmits,
        .rx_drops = stats.rx_drops,
        .last_source_port = stats.last_source_port,
        .last_dest_port = stats.last_dest_port,
        .last_seq = stats.last_seq,
        .last_ack = stats.last_ack,
        .last_payload_len = stats.last_payload_len,
        .checksum_errors = stats.checksum_errors,
        .timeouts = stats.timeouts,
        .self_tests = stats.self_tests,
    };
}

pub fn connectionInfo(index: u32, out: *ConnectionInfo) i32 {
    if (index >= MAX_CONNECTIONS) return 0;
    const c = &connections[@intCast(index)];
    if (!c.used) return 0;
    out.* = .{
        .id = c.id,
        .state = @intFromEnum(c.state),
        .local_port = c.local_port,
        .remote_port = c.remote_port,
        .remote_ip = c.remote_ip,
        .tx_bytes = c.tx_bytes,
        .rx_bytes = c.rx_bytes,
        .pending_rx = @intCast(c.rx_len),
        .retransmits = c.retransmits,
        .rx_window = @intCast(rxFree(c)),
        .tx_window = c.tx_window,
        .tx_ack = c.tx_ack,
        .rx_drops = c.rx_drops,
        .seq = c.seq,
        .ack = c.ack,
        .last_seq = c.last_seq,
        .last_ack = c.last_ack,
        .last_flags = c.last_flags,
        .last_payload_len = @intCast(c.last_len),
    };
    return 1;
}

// 0.56.20: Selbsttest-Name statt "connect" - dies ist KEIN produktiver
// Client-Pfad, sondern der Loopback-Aufbau fuer Probes (Befund 13.1.5).
pub fn selfTestConnect(remote_ip: [4]u8, remote_port: u16) i32 {
    const idx = allocConnection() orelse return -1;
    const c = &connections[idx];
    c.* = .{
        .used = true,
        .id = next_id,
        .state = .syn_sent,
        .local_port = next_port,
        .remote_port = remote_port,
        .remote_ip = remote_ip,
        .seq = 0x1000 + next_id,
    };
    next_id +%= 1;
    next_port +%= 1;
    stats.syn_tx += 1;
    stats.synack_rx += 1;
    stats.ack_tx += 1;
    c.ack = 0x2001;
    c.state = .established;
    return @intCast(c.id);
}

pub fn beginLiveConnect(remote_ip: [4]u8, remote_port: u16, initial_seq: u32) i32 {
    const idx = allocConnection() orelse {
        stats.last_error = "no-conn";
        return -1;
    };
    const c = &connections[idx];
    c.* = .{
        .used = true,
        .id = next_id,
        .state = .syn_sent,
        .local_port = next_port,
        .remote_port = remote_port,
        .remote_ip = remote_ip,
        .seq = initial_seq,
        .ack = 0,
        .tx_window = 0,
    };
    next_id +%= 1;
    if (next_id == 0) next_id = 1;
    next_port +%= 1;
    if (next_port < 49152) next_port = 49152;
    stats.last_error = "syn-sent";
    return @intCast(c.id);
}

pub fn listen(port: u16) bool {
    if (port == 0) return false;
    if (hasListener(port)) return true;
    var i: usize = 0;
    while (i < listeners.len) : (i += 1) {
        if (listeners[i].used) continue;
        listeners[i] = .{ .used = true, .port = port };
        stats.last_error = "listen";
        return true;
    }
    stats.last_error = "listen-full";
    return false;
}

pub fn closeListener(port: u16) void {
    var i: usize = 0;
    while (i < listeners.len) : (i += 1) {
        if (listeners[i].used and listeners[i].port == port) {
            listeners[i] = .{};
            stats.last_error = "listen-close";
            return;
        }
    }
}

pub fn hasListener(port: u16) bool {
    var i: usize = 0;
    while (i < listeners.len) : (i += 1) {
        if (listeners[i].used and listeners[i].port == port) return true;
    }
    return false;
}

pub fn acceptInbound(view: SegmentView, initial_seq: u32) ?u32 {
    if (!hasListener(view.dest_port)) return null;
    if ((view.flags & FLAG_SYN) == 0 or (view.flags & FLAG_ACK) != 0) return null;
    if (matchConnection(view)) |existing| {
        if (existing.state == .syn_received) return existing.id;
        resetConnectionSlot(existing);
        stats.last_error = "syn-reuse";
    }
    const idx = allocConnection() orelse {
        stats.last_error = "accept-full";
        return null;
    };
    const c = &connections[idx];
    c.* = .{
        .used = true,
        .id = next_id,
        .state = .syn_received,
        .local_port = view.dest_port,
        .remote_port = view.source_port,
        .remote_ip = view.source_ip,
        .seq = initial_seq,
        .ack = view.seq +% 1,
        .tx_window = view.window,
    };
    next_id +%= 1;
    if (next_id == 0) next_id = 1;
    stats.listen_syn_rx += 1;
    stats.last_error = "syn-rx";
    return c.id;
}

pub fn abort(conn_id: u32, reason: []const u8) void {
    const c = byId(conn_id) orelse return;
    c.state = .closed;
    c.used = false;
    stats.last_error = reason;
}

pub fn markTimeout(reason: []const u8) void {
    stats.timeouts += 1;
    stats.last_error = reason;
}

pub fn recordMalformed(reason: []const u8) void {
    stats.malformed += 1;
    stats.last_error = reason;
}

pub fn recordChecksumError() void {
    stats.checksum_errors += 1;
    stats.last_error = "checksum";
}

pub fn setError(reason: []const u8) void {
    stats.last_error = reason;
}

pub fn established(conn_id: u32) bool {
    const c = byId(conn_id) orelse return false;
    return c.state == .established;
}

pub fn connectionWithDataOnPort(port: u16) ?u32 {
    var i: usize = 0;
    while (i < connections.len) : (i += 1) {
        const c = &connections[i];
        if (c.used and c.state == .established and c.local_port == port and !c.accepted_claimed and c.rx_len != 0) return c.id;
    }
    return null;
}

pub fn connectionOnPort(port: u16) ?u32 {
    var i: usize = 0;
    while (i < connections.len) : (i += 1) {
        const c = &connections[i];
        if (c.used and c.state == .established and c.local_port == port and !c.accepted_claimed) return c.id;
    }
    return null;
}

pub fn claimAccepted(conn_id: u32) bool {
    const c = byId(conn_id) orelse return false;
    if (c.accepted_claimed) return false;
    c.accepted_claimed = true;
    return true;
}

pub fn connectionIdForSegment(view: SegmentView) ?u32 {
    const c = matchConnection(view) orelse return null;
    return c.id;
}

pub fn abortConnectionsOnPort(port: u16, reason: []const u8) void {
    var i: usize = 0;
    while (i < connections.len) : (i += 1) {
        const c = &connections[i];
        if (!c.used or c.local_port != port) continue;
        resetConnectionSlot(c);
        stats.last_error = reason;
    }
}

pub fn abortAll(reason: []const u8) CleanupResult {
    var result: CleanupResult = .{};
    var i: usize = 0;
    while (i < connections.len) : (i += 1) {
        if (!connections[i].used) continue;
        resetConnectionSlot(&connections[i]);
        result.connections += 1;
    }
    i = 0;
    while (i < listeners.len) : (i += 1) {
        if (!listeners[i].used) continue;
        listeners[i] = .{};
        result.listeners += 1;
    }
    if (result.connections != 0 or result.listeners != 0) stats.last_error = reason;
    return result;
}

pub fn planSend(conn_id: u32) ?SendPlan {
    const c = byId(conn_id) orelse return null;
    if (c.state == .closed) return null;
    return .{
        .local_port = c.local_port,
        .remote_port = c.remote_port,
        .remote_ip = c.remote_ip,
        .seq = c.seq,
        .ack = c.ack,
        .rx_window = advertisedWindow(c),
    };
}

pub fn planRetransmit(conn_id: u32) ?RetransmitPlan {
    const c = byId(conn_id) orelse return null;
    if (!c.last_valid or c.state == .closed) return null;
    return .{
        .local_port = c.local_port,
        .remote_port = c.remote_port,
        .remote_ip = c.remote_ip,
        .seq = c.last_seq,
        .ack = c.last_ack,
        .flags = c.last_flags,
        .rx_window = advertisedWindow(c),
        .payload = c.last_payload[0..c.last_len],
    };
}

pub fn commitSent(conn_id: u32, flags: u16, payload: []const u8) void {
    const c = byId(conn_id) orelse return;
    rememberSent(c, flags, payload);
    const payload_len = payload.len;
    if ((flags & FLAG_SYN) != 0) {
        if ((flags & FLAG_ACK) != 0) {
            stats.synack_tx += 1;
            stats.last_error = "synack-tx";
        } else {
            stats.syn_tx += 1;
            stats.last_error = "syn";
        }
        return;
    }
    if ((flags & FLAG_ACK) != 0) stats.ack_tx += 1;
    if (payload_len != 0) {
        c.seq +%= @intCast(payload_len);
        c.tx_window -|= @intCast(payload_len);
        c.tx_bytes += @intCast(payload_len);
        stats.data_tx += 1;
        stats.last_error = "data";
    }
    if ((flags & FLAG_FIN) != 0) {
        c.seq +%= 1;
        c.state = .fin_wait;
        stats.fin_tx += 1;
        stats.last_error = "fin";
    }
}

pub fn recordRetransmit(conn_id: u32, reason: []const u8) void {
    const c = byId(conn_id) orelse return;
    c.retransmits += 1;
    stats.retransmits += 1;
    stats.last_error = reason;
}

// 0.56.22: Nicht-konsumierende Lesbarkeits-Pruefung fuer die
// eventgetriebenen Waits (closed zaehlt als "lesbar", damit der
// Warteende-Fall aufweckt und read() den Status liefert).
// 0.56.23: tx_ack-Getter fuer die RTO-Messung in core (tcp.zig bleibt
// bewusst ohne Timer-Import).
pub fn txAckOf(conn_id: u32) u32 {
    const c = byId(conn_id) orelse return 0;
    return c.tx_ack;
}

pub fn readable(conn_id: u32) bool {
    const c = byId(conn_id) orelse return true;
    return c.rx_len != 0 or c.state == .closed;
}

pub fn closed(conn_id: u32) bool {
    const c = byId(conn_id) orelse return true;
    // 0.56.20: last_ack/time_wait sind aus ANWENDUNGSSICHT beendet (kein
    // Datenfluss mehr) - nur die Slot-Hygiene laeuft noch (reapTimeWait).
    // Ohne diese Sicht haette der aktive Close-Pfad (waitForTcpClosed)
    // FINs bis zum Limit retransmittiert und FTP-Empfaenger haetten das
    // Datenende nicht erkannt (Teil-Dateien im Gate beobachtet).
    return c.state == .closed or c.state == .time_wait or c.state == .last_ack;
}

// 0.56.20: Selbsttest-Name statt "write" (Befund 13.1.5).
pub fn loopbackWrite(conn_id: u32, data: []const u8) i32 {
    const c = byId(conn_id) orelse return -1;
    if (c.state != .established or data.len > BUFFER_SIZE) return -1;
    c.tx_bytes += @intCast(data.len);
    stats.data_tx += 1;
    const copy_len = writeRx(c, data);
    c.rx_bytes += @intCast(copy_len);
    stats.data_rx += 1;
    return @intCast(data.len);
}

pub fn read(conn_id: u32, out: []u8) i32 {
    const c = byId(conn_id) orelse return -1;
    const len = if (out.len < c.rx_len) out.len else c.rx_len;
    if (len != 0) {
        var copied: usize = 0;
        while (copied < len) {
            const run = @min(len - copied, c.rx.len - c.rx_head);
            @memcpy(out[copied .. copied + run], c.rx[c.rx_head .. c.rx_head + run]);
            c.rx_head = (c.rx_head + run) % c.rx.len;
            c.rx_len -= run;
            copied += run;
        }
        if (c.rx_len == 0) c.rx_head = 0;
        flushOutOfOrderPayload(c);
        // 0.56.39: Slot NICHT im selben Aufruf freigeben, der noch Daten
        // liefert. Vorher setzte "closed && rx_len==0" hier c.used=false -
        // ein Segment, das nach diesem Read noch eintraf (Retransmit,
        // out-of-order-Nachzuegler), fand dann keinen belegten Slot mehr
        // und ging verloren: FTP-STOR verlor exakt einen Read-Chunk
        // (3916 = tcp_read_max) und meldete trotzdem 226 complete
        // (0.56.37-4-KB-Chunks vergroesserten das Verlustfenster).
        // Drain-then-close: der Slot bleibt belegt, bis ein Read bei
        // leerem Puffer und closed sauber EOF(-1) liefert (Zweig unten)
        // oder der Konsument explizit schliesst (FTPSVC
        // finishDataConnection ruft closeTcpHandle).
    } else if (c.state == .closed) {
        c.used = false;
        return -1;
    }
    return @intCast(len);
}

pub fn close(conn_id: u32) i32 {
    const c = byId(conn_id) orelse return -1;
    if (c.state == .closed) {
        c.used = false;
        return 0;
    }
    if (c.state != .fin_wait) {
        c.state = .fin_wait;
        stats.fin_tx += 1;
    }
    c.state = .closed;
    c.used = false;
    return 0;
}

pub fn bufferLimitProbe() bool {
    const remote: [4]u8 = .{ 198, 51, 100, 90 };
    const local: [4]u8 = .{ 192, 0, 2, 2 };
    const before_drops = stats.rx_drops;
    const conn = selfTestConnect(remote, 65090);
    if (conn <= 0) return false;
    const conn_id: u32 = @intCast(conn);
    defer _ = close(conn_id);

    const c = byId(conn_id) orelse return false;
    var chunk: [512]u8 = .{0x51} ** 512;
    var filled: usize = 0;
    while (filled < BUFFER_SIZE) {
        const want = @min(chunk.len, BUFFER_SIZE - filled);
        const copied = writeRx(c, chunk[0..want]);
        if (copied != want) return false;
        filled += copied;
    }
    if (rxFree(c) != 0) return false;

    const view = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = c.ack,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "X",
    };
    _ = applyRxView(view) orelse return false;
    return stats.rx_drops == before_drops + 1 and c.rx_drops == 1 and memEql(stats.last_error, "rx-full");
}

pub fn outOfOrderProbe() bool {
    if (!smallOutOfOrderProbe()) return false;
    return burstOutOfOrderProbe();
}

fn smallOutOfOrderProbe() bool {
    const remote: [4]u8 = .{ 198, 51, 100, 91 };
    const local: [4]u8 = .{ 192, 0, 2, 2 };
    const before_drops = stats.rx_drops;
    const conn = selfTestConnect(remote, 65091);
    if (conn <= 0) return false;
    const conn_id: u32 = @intCast(conn);
    defer _ = close(conn_id);

    const c = byId(conn_id) orelse return false;
    const base_seq = c.ack;
    const third = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = base_seq + 6,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "GHI",
    };
    _ = applyRxView(third) orelse return false;
    if (outOfOrderCount(c) != 1 or c.rx_len != 0 or c.ack != base_seq) return false;

    const second = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = base_seq + 3,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "DEF",
    };
    _ = applyRxView(second) orelse return false;
    if (outOfOrderCount(c) != 2 or c.rx_len != 0 or c.ack != base_seq) return false;

    const first = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = base_seq,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "ABC",
    };
    _ = applyRxView(first) orelse return false;
    var out: [9]u8 = .{0} ** 9;
    const got = read(conn_id, out[0..]);
    stats.self_tests += 1;
    return got == 9 and
        memEql(out[0..], "ABCDEFGHI") and
        outOfOrderCount(c) == 0 and
        stats.rx_drops == before_drops;
}

fn burstOutOfOrderProbe() bool {
    const segment_count: usize = 40;
    const segment_size: usize = 1024;
    const remote: [4]u8 = .{ 198, 51, 100, 92 };
    const local: [4]u8 = .{ 192, 0, 2, 2 };
    const before_drops = stats.rx_drops;
    const conn = selfTestConnect(remote, 65092);
    if (conn <= 0) return false;
    const conn_id: u32 = @intCast(conn);
    defer _ = close(conn_id);

    const c = byId(conn_id) orelse return false;
    const base_seq = c.ack;
    var payload: [segment_size]u8 = undefined;
    var segment_index: usize = segment_count;
    while (segment_index > 1) {
        segment_index -= 1;
        fillOutOfOrderProbePayload(payload[0..], segment_index);
        const view = SegmentView{
            .source_ip = remote,
            .dest_ip = local,
            .source_port = c.remote_port,
            .dest_port = c.local_port,
            .seq = base_seq + @as(u32, @intCast(segment_index * segment_size)),
            .ack = c.seq,
            .flags = FLAG_ACK | FLAG_PSH,
            .window = MAX_ADVERTISED_WINDOW,
            .payload = payload[0..],
        };
        _ = applyRxView(view) orelse return false;
    }
    if (outOfOrderCount(c) != segment_count - 1 or c.rx_len != 0 or c.ack != base_seq) return false;

    fillOutOfOrderProbePayload(payload[0..], 0);
    const first = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = base_seq,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = payload[0..],
    };
    _ = applyRxView(first) orelse return false;
    if (outOfOrderCount(c) != 0 or c.rx_len != segment_count * segment_size) return false;

    var read_buf: [segment_size]u8 = undefined;
    segment_index = 0;
    while (segment_index < segment_count) : (segment_index += 1) {
        const got = read(conn_id, read_buf[0..]);
        if (got != @as(i32, @intCast(segment_size))) return false;
        if (!verifyOutOfOrderProbePayload(read_buf[0..], segment_index)) return false;
    }
    stats.self_tests += 1;
    return outOfOrderCount(c) == 0 and
        c.rx_len == 0 and
        stats.rx_drops == before_drops;
}

pub fn applyRxView(view: SegmentView) ?SegmentView {
    stats.rx_segments += 1;
    recordView(view);
    if ((view.flags & FLAG_RST) != 0) {
        stats.rst_rx += 1;
        if (matchConnection(view)) |c| {
            if (!resetSeqAcceptable(c, view)) {
                c.rx_drops += 1;
                stats.rx_drops += 1;
                stats.last_error = "rst-stale";
                return view;
            }
            c.state = .closed;
            c.used = false;
        }
        stats.last_error = "rst";
        return view;
    }
    if (matchConnection(view)) |c| {
        if (c.state == .syn_sent and (view.flags & (FLAG_SYN | FLAG_ACK)) == (FLAG_SYN | FLAG_ACK) and view.ack == c.seq +% 1) {
            c.seq +%= 1;
            c.ack = view.seq +% 1;
            c.state = .established;
            updateTxWindow(c, view);
            stats.synack_rx += 1;
            stats.last_error = "synack";
        } else if (c.state == .syn_received and (view.flags & FLAG_ACK) != 0 and view.ack == c.seq +% 1) {
            c.seq +%= 1;
            c.state = .established;
            updateTxWindow(c, view);
            stats.accepts += 1;
            stats.last_error = "accepted";
        } else if (c.state == .established) {
            if ((view.payload.len != 0 or (view.flags & FLAG_FIN) != 0) and !segmentSeqAcceptable(c, view)) {
                c.rx_drops += 1;
                stats.rx_drops += 1;
                stats.last_error = "seq-stale";
                return view;
            }
            updateTxWindow(c, view);
            if (view.payload.len != 0) {
                acceptPayload(c, view.seq, view.payload);
            }
            if ((view.flags & FLAG_FIN) != 0 and view.seq +% @as(u32, @intCast(view.payload.len)) == c.ack) {
                // 0.56.20: FIN nur in-order annehmen (c.ack+1 statt
                // view.seq+1 - Letzteres haette bei Payload+FIN die
                // angenommenen Bytes wieder un-ackt). Der last_ack/
                // time_wait-Ausbau ist ZURUECKGESTELLT: er brach die
                // FTP-Close-Vertraege der Konsumenten (zwei Gate-
                // Regressionen); Wiederaufnahme mit den Event-Umbauten.
                c.ack +%= 1;
                c.state = .closed;
                stats.last_error = "fin-rx";
            }
        } else if (c.state == .fin_wait and (view.flags & FLAG_ACK) != 0) {
            updateTxWindow(c, view);
            if ((view.flags & FLAG_FIN) != 0 and view.seq == c.ack) c.ack +%= 1;
            c.state = .closed;
            c.used = false;
            stats.last_error = "closed";
        }
    }
    return view;
}

pub fn applyTxView(view: SegmentView) ?SegmentView {
    recordView(view);
    stats.tx_segments += 1;
    return view;
}

fn recordView(view: SegmentView) void {
    stats.last_flags = view.flags;
    stats.last_source_port = view.source_port;
    stats.last_dest_port = view.dest_port;
    stats.last_seq = view.seq;
    stats.last_ack = view.ack;
    stats.last_payload_len = @intCast(view.payload.len);
    stats.last_error = "none";
}

fn acceptPayload(c: *Connection, seq: u32, payload: []const u8) void {
    if (payload.len == 0) return;
    if (seq == c.ack) {
        acceptInOrderPayload(c, seq, payload);
        flushOutOfOrderPayload(c);
        return;
    }
    if (seqAfter(seq, c.ack)) {
        storeOutOfOrderPayload(c, seq, payload);
        return;
    }

    const overlap = c.ack -% seq;
    if (overlap < payload.len) {
        const start: usize = @intCast(overlap);
        acceptInOrderPayload(c, c.ack, payload[start..]);
        flushOutOfOrderPayload(c);
    } else {
        stats.last_error = "seq-dup";
    }
}

fn acceptInOrderPayload(c: *Connection, seq: u32, payload: []const u8) void {
    const copy_len = writeRx(c, payload);
    if (copy_len < payload.len) {
        const dropped = payload.len - copy_len;
        c.rx_drops += @intCast(dropped);
        stats.rx_drops += @intCast(dropped);
    }
    c.rx_bytes += @intCast(copy_len);
    c.ack = seq +% @as(u32, @intCast(copy_len));
    if (copy_len != 0) {
        stats.data_rx += 1;
        stats.last_error = if (copy_len == payload.len) "data-rx" else "rx-full";
    } else {
        stats.last_error = "rx-full";
    }
}

fn storeOutOfOrderPayload(c: *Connection, seq: u32, payload: []const u8) void {
    if (payload.len > OUT_OF_ORDER_SLOT_SIZE) {
        c.rx_drops += @intCast(payload.len);
        stats.rx_drops += @intCast(payload.len);
        stats.last_error = "ooo-large";
        return;
    }

    const payload_end = seq +% @as(u32, @intCast(payload.len));
    if (!seqAfter(payload_end, c.ack)) {
        stats.last_error = "seq-dup";
        return;
    }

    if (findOutOfOrderCovering(c, seq, payload.len)) |idx| {
        const slot = &c.out_of_order[idx];
        if (payload.len > slot.len) {
            @memcpy(slot.payload[0..payload.len], payload);
            slot.len = payload.len;
        }
        stats.last_error = "ooo-dup";
        return;
    }

    const idx = findOutOfOrderFree(c) orelse selectOutOfOrderReplacement(c, seq, payload.len) orelse {
        c.rx_drops += @intCast(payload.len);
        stats.rx_drops += @intCast(payload.len);
        stats.last_error = "ooo-full";
        return;
    };
    if (c.out_of_order[idx].valid) {
        c.rx_drops += @intCast(c.out_of_order[idx].len);
        stats.rx_drops += @intCast(c.out_of_order[idx].len);
    }
    if (payload.len != 0) @memcpy(c.out_of_order[idx].payload[0..payload.len], payload);
    c.out_of_order[idx].seq = seq;
    c.out_of_order[idx].len = payload.len;
    c.out_of_order[idx].valid = true;
    stats.last_error = "ooo-store";
}

fn flushOutOfOrderPayload(c: *Connection) void {
    while (findFlushableOutOfOrder(c)) |idx| {
        const slot = &c.out_of_order[idx];
        const offset_u32 = c.ack -% slot.seq;
        const offset: usize = @intCast(offset_u32);
        if (offset >= slot.len) {
            slot.* = .{};
            continue;
        }
        const remaining = slot.len - offset;
        if (rxFree(c) < remaining) {
            stats.last_error = "ooo-wait";
            return;
        }
        var payload: [OUT_OF_ORDER_SLOT_SIZE]u8 = undefined;
        @memcpy(payload[0..remaining], slot.payload[offset..slot.len]);
        const seq = c.ack;
        slot.* = .{};
        acceptInOrderPayload(c, seq, payload[0..remaining]);
        if (stats.last_error.len == 7 and memEql(stats.last_error, "data-rx")) stats.last_error = "ooo-flush";
    }
}

fn findOutOfOrderFree(c: *const Connection) ?usize {
    var i: usize = 0;
    while (i < c.out_of_order.len) : (i += 1) {
        if (!c.out_of_order[i].valid) return i;
    }
    return null;
}

fn findOutOfOrderCovering(c: *const Connection, seq: u32, len: usize) ?usize {
    const end = seq +% @as(u32, @intCast(len));
    var i: usize = 0;
    while (i < c.out_of_order.len) : (i += 1) {
        const slot = &c.out_of_order[i];
        if (!slot.valid) continue;
        const slot_end = slot.seq +% @as(u32, @intCast(slot.len));
        const seq_covered = slot.seq == seq or (!seqAfter(slot.seq, seq) and seqAfter(slot_end, seq));
        const end_covered = slot_end == end or seqAfter(slot_end, end);
        if (seq_covered and end_covered) return i;
    }
    return null;
}

fn selectOutOfOrderReplacement(c: *const Connection, seq: u32, len: usize) ?usize {
    _ = len;
    const new_distance = seq -% c.ack;
    var farthest_idx: ?usize = null;
    var farthest_distance: u32 = 0;
    var i: usize = 0;
    while (i < c.out_of_order.len) : (i += 1) {
        const slot = &c.out_of_order[i];
        if (!slot.valid) continue;
        const distance = slot.seq -% c.ack;
        if (farthest_idx == null or distance > farthest_distance) {
            farthest_idx = i;
            farthest_distance = distance;
        }
    }
    if (farthest_idx) |idx| {
        if (new_distance < farthest_distance) return idx;
    }
    return null;
}

fn findFlushableOutOfOrder(c: *const Connection) ?usize {
    var best_idx: ?usize = null;
    var best_offset: u32 = 0;
    var i: usize = 0;
    while (i < c.out_of_order.len) : (i += 1) {
        const slot = &c.out_of_order[i];
        if (!slot.valid) continue;
        const slot_end = slot.seq +% @as(u32, @intCast(slot.len));
        const begins_at_ack = slot.seq == c.ack;
        const overlaps_ack = !seqAfter(slot.seq, c.ack) and seqAfter(slot_end, c.ack);
        if (!begins_at_ack and !overlaps_ack) continue;
        const offset = c.ack -% slot.seq;
        if (best_idx == null or offset < best_offset) {
            best_idx = i;
            best_offset = offset;
        }
    }
    return best_idx;
}

fn outOfOrderCount(c: *const Connection) usize {
    var count_slots: usize = 0;
    for (c.out_of_order) |slot| {
        if (slot.valid) count_slots += 1;
    }
    return count_slots;
}

fn allocConnection() ?usize {
    var i: usize = 0;
    while (i < connections.len) : (i += 1) if (!connections[i].used) return i;
    _ = reapClosedConnectionSlots();
    i = 0;
    while (i < connections.len) : (i += 1) if (!connections[i].used) return i;
    return null;
}

fn reapClosedConnectionSlots() u32 {
    var count: u32 = 0;
    var i: usize = 0;
    while (i < connections.len) : (i += 1) {
        if (!connections[i].used or connections[i].state != .closed) continue;
        connections[i] = .{};
        count += 1;
    }
    if (count != 0) stats.last_error = "closed-reap";
    return count;
}

fn byId(id: u32) ?*Connection {
    var i: usize = 0;
    while (i < connections.len) : (i += 1) if (connections[i].used and connections[i].id == id) return &connections[i];
    return null;
}

fn matchConnection(view: SegmentView) ?*Connection {
    var i: usize = 0;
    while (i < connections.len) : (i += 1) {
        const c = &connections[i];
        if (!c.used or c.state == .closed) continue;
        if (c.local_port == view.dest_port and c.remote_port == view.source_port and sameIp(c.remote_ip, view.source_ip)) return c;
    }
    return null;
}

fn activeCount() u32 {
    var count: u32 = 0;
    for (&connections) |*c| {
        if (c.used and c.state != .closed) count += 1;
    }
    return count;
}

fn rememberSent(c: *Connection, flags: u16, payload: []const u8) void {
    if ((flags & (FLAG_SYN | FLAG_FIN)) == 0 and payload.len == 0) return;
    c.last_valid = true;
    c.last_seq = c.seq;
    c.last_ack = c.ack;
    c.last_flags = flags;
    const copy_len = if (payload.len > c.last_payload.len) c.last_payload.len else payload.len;
    if (copy_len != 0) @memcpy(c.last_payload[0..copy_len], payload[0..copy_len]);
    c.last_len = copy_len;
}

fn updateTxWindow(c: *Connection, view: SegmentView) void {
    if ((view.flags & FLAG_ACK) == 0) {
        c.tx_window = view.window;
        return;
    }
    if (c.tx_ack == 0 or seqAfter(view.ack, c.tx_ack)) {
        c.tx_ack = view.ack;
    }
    const right_edge = view.ack +% @as(u32, view.window);
    c.tx_window = if (right_edge >= c.seq) right_edge - c.seq else 0;
}

fn writeRx(c: *Connection, data: []const u8) usize {
    const copy_len = @min(data.len, rxFree(c));
    if (copy_len == 0) return 0;
    var copied: usize = 0;
    while (copied < copy_len) {
        const tail = (c.rx_head + c.rx_len) % c.rx.len;
        const run = @min(copy_len - copied, c.rx.len - tail);
        @memcpy(c.rx[tail .. tail + run], data[copied .. copied + run]);
        c.rx_len += run;
        copied += run;
    }
    return copy_len;
}

fn rxFree(c: *const Connection) usize {
    return c.rx.len - c.rx_len;
}

fn seqAfter(a: u32, b: u32) bool {
    const diff: i32 = @bitCast(a -% b);
    return diff > 0;
}

fn resetSeqAcceptable(c: *const Connection, view: SegmentView) bool {
    return switch (c.state) {
        .syn_sent => (view.flags & FLAG_ACK) != 0 and view.ack == c.seq +% 1,
        .syn_received, .established, .fin_wait, .last_ack, .time_wait => segmentSeqAcceptable(c, view),
        .closed => false,
    };
}

fn segmentSeqAcceptable(c: *const Connection, view: SegmentView) bool {
    if (view.seq == c.ack) return true;
    // 0.56.20 (Befund 13.1.2): Ueberlappender Retransmit - Anfang liegt vor
    // c.ack, das ENDE dahinter. Ohne diese Ausnahme wurde das Segment als
    // "seq-stale" verworfen und der Overlap-Zweig in acceptPayload war
    // unerreichbar (Haenger-Kandidat bei verlorenen ACKs).
    if (view.payload.len != 0) {
        const seg_end = view.seq +% @as(u32, @intCast(view.payload.len));
        if (!seqAfter(view.seq, c.ack) and seqAfter(seg_end, c.ack)) return true;
    }
    const window: u32 = @intCast(@max(@as(usize, 1), rxFree(c)));
    const distance = view.seq -% c.ack;
    return distance < window;
}

// 0.56.20 (Befund 13.1.4): gezielte Feldruecksetzung statt c.* = .{} -
// das Default-Init memsettet sonst rx + out_of_order (>350 KB pro Slot).
// Die Pufferinhalte sind ohne used/rx_len/valid-Flags bedeutungslos.
fn resetConnectionSlot(c: *Connection) void {
    c.used = false;
    c.id = 0;
    c.state = .closed;
    c.local_port = 0;
    c.remote_port = 0;
    c.remote_ip = .{0} ** 4;
    c.seq = 0;
    c.ack = 0;
    c.tx_bytes = 0;
    c.rx_bytes = 0;
    c.retransmits = 0;
    c.tx_window = 0;
    c.tx_ack = 0;
    c.rx_drops = 0;
    c.accepted_claimed = false;
    c.time_wait_until = 0;
    c.rx_head = 0;
    c.rx_len = 0;
    c.last_valid = false;
    c.last_seq = 0;
    c.last_ack = 0;
    c.last_flags = 0;
    c.last_len = 0;
    for (&c.out_of_order) |*slot| slot.valid = false;
}

// 0.56.20 (Befund 13.1.3): TIME_WAIT-/LAST_ACK-/FIN_WAIT-Slots tick-basiert
// freigeben. tcp.zig bleibt bewusst ohne Timer-Import (reine State-Machine);
// der net-rx-Task liefert die Tick-Zeit. Die Frist wird beim ersten
// Reap-Besuch gesetzt, weil der Zustandseintritt keine Zeit kennt.
pub fn reapTimeWait(now: u64) u32 {
    var freed: u32 = 0;
    var i: usize = 0;
    while (i < connections.len) : (i += 1) {
        const c = &connections[i];
        if (!c.used) continue;
        // 0.56.39: Seit read() den Slot nicht mehr im daten-liefernden
        // Aufruf freigibt (Drain-then-close), koennte ein vollstaendig
        // gelesener .closed-Slot leaken, wenn ein Konsument den finalen
        // EOF-Read auslaesst und nicht explizit schliesst. Diese
        // Slot-Hygiene faengt das nach derselben TIME_WAIT-Frist ab
        // (Grace deckt spaete Retransmits ab; ein Konsument, der EOF
        // holt, gibt den Slot ohnehin frueher frei).
        if (c.state == .closed and c.rx_len == 0) {
            if (c.time_wait_until == 0) {
                c.time_wait_until = now +% TIME_WAIT_TICKS;
            } else if (now >= c.time_wait_until) {
                c.used = false;
                freed += 1;
            }
            continue;
        }
        if (c.state != .fin_wait) continue;
        if (c.time_wait_until == 0) {
            c.time_wait_until = now +% TIME_WAIT_TICKS;
            continue;
        }
        if (now >= c.time_wait_until) {
            c.state = .closed;
            c.used = false;
            freed += 1;
        }
    }
    return freed;
}

fn advertisedWindow(c: *const Connection) u16 {
    const free = rxFree(c);
    return if (free > MAX_ADVERTISED_WINDOW) MAX_ADVERTISED_WINDOW else @intCast(free);
}

fn listenerCount() u32 {
    var count: u32 = 0;
    for (&listeners) |*listener| {
        if (listener.used) count += 1;
    }
    return count;
}

fn fillOutOfOrderProbePayload(out: []u8, segment_index: usize) void {
    var i: usize = 0;
    while (i < out.len) : (i += 1) {
        out[i] = @truncate((segment_index * 17 + i) & 0xFF);
    }
}

fn verifyOutOfOrderProbePayload(data: []const u8, segment_index: usize) bool {
    var i: usize = 0;
    while (i < data.len) : (i += 1) {
        const expected: u8 = @truncate((segment_index * 17 + i) & 0xFF);
        if (data[i] != expected) return false;
    }
    return true;
}

fn memEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) if (a[i] != b[i]) return false;
    return true;
}

fn sameIp(a: [4]u8, b: [4]u8) bool {
    return a[0] == b[0] and a[1] == b[1] and a[2] == b[2] and a[3] == b[3];
}

// --- 0.56.20: Neue Selbsttest-Probes (Befunde 13.1.1/13.1.2) ---

// Ueberlappender Retransmit: Anfang vor c.ack, Ende dahinter -> nur der
// neue Teil (ab ack) wird angenommen, ack rueckt auf das Segmentende.
pub fn overlapRetransmitProbe() bool {
    const remote: [4]u8 = .{ 198, 51, 100, 92 };
    const local: [4]u8 = .{ 192, 0, 2, 2 };
    const conn = selfTestConnect(remote, 65092);
    if (conn <= 0) return false;
    const conn_id: u32 = @intCast(conn);
    defer _ = close(conn_id);
    const c = byId(conn_id) orelse return false;
    const base = c.ack;

    const first = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = base,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "ABCDE",
    };
    _ = applyRxView(first) orelse return false;
    if (c.ack != base +% 5) return false;
    const rx_before = c.rx_bytes;

    const overlap = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = base,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "ABCDEFGH",
    };
    _ = applyRxView(overlap) orelse return false;
    return c.ack == base +% 8 and c.rx_bytes == rx_before + 3;
}

// Sequenz-Wraparound: Peer-Sequenz laeuft ueber die 2^32-Grenze; Annahme,
// ack-Fortschritt und FIN-Behandlung muessen wrap-sicher sein (mit "+ 1"
// statt "+% 1" waere das in ReleaseSafe ein Panic-Kandidat).
pub fn wraparoundProbe() bool {
    const remote: [4]u8 = .{ 198, 51, 100, 93 };
    const local: [4]u8 = .{ 192, 0, 2, 2 };
    const conn = selfTestConnect(remote, 65093);
    if (conn <= 0) return false;
    const conn_id: u32 = @intCast(conn);
    defer _ = close(conn_id);
    const c = byId(conn_id) orelse return false;

    c.ack = 0xFFFF_FFFE;
    const rx_before = c.rx_bytes;
    const seg1 = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = 0xFFFF_FFFE,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "AB",
    };
    _ = applyRxView(seg1) orelse return false;
    if (c.ack != 0) return false;

    const seg2 = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = 0,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_PSH,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "CD",
    };
    _ = applyRxView(seg2) orelse return false;
    if (c.ack != 2 or c.rx_bytes != rx_before + 4) return false;

    const fin = SegmentView{
        .source_ip = remote,
        .dest_ip = local,
        .source_port = c.remote_port,
        .dest_port = c.local_port,
        .seq = 2,
        .ack = c.seq,
        .flags = FLAG_ACK | FLAG_FIN,
        .window = MAX_ADVERTISED_WINDOW,
        .payload = "",
    };
    _ = applyRxView(fin) orelse return false;
    return c.ack == 3 and c.state == .closed;
}
const r4x_api = @import("../program/r4x_api.zig");
