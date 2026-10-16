// Native TCP state owns tables, sequence/catalog policy and original probes.
const Text = @import("../driver/usb/core.zig").Text;
const tcp_runtime = @import("tcp_runtime.zig");
const r4x_api = @import("../program/r4x_api.zig");
pub const IPV4_PROTOCOL: u8 = 6;
pub const HEADER_SIZE: usize = 20;
pub const MAX_CONNECTIONS: usize = tcp_runtime.CONNECTION_CAPACITY;
pub const MAX_LISTENERS: usize = 4;
pub const BUFFER_SIZE: usize = 256 * 1024;
pub const MAX_ADVERTISED_WINDOW: u16 = 0xFFFF;
pub const LOCAL_MSS: u16 = 1460;
pub const DEFAULT_PEER_MSS: u16 = 536;
pub const LOCAL_WINDOW_SCALE: u8 = 2;
pub const MAX_WINDOW_SCALE: u8 = 14;
pub const OUT_OF_ORDER_SLOT_COUNT: usize = 48;
pub const OUT_OF_ORDER_SLOT_SIZE: usize = 2048;
pub const SENT_SEGMENT_CAPACITY: usize = tcp_runtime.SENT_SEGMENT_CAPACITY;
pub const SENT_PAYLOAD_CAPACITY: usize = tcp_runtime.SENT_PAYLOAD_CAPACITY;
// 0.56.20: Frist fuer TIME_WAIT-/LAST_ACK-/FIN_WAIT-Slots (Befund 13.1.3);
// gleicher Wert wie timing.DEFAULT_TCP_TIME_WAIT_TICKS.
pub const TIME_WAIT_TICKS: u64 = @import("timing.zig").msToTicks(3_000);
// 0.56.20: Groessen-Nachweis fuer Boot-Log/Diagnose (Befund 13.1.4).

pub const FLAG_FIN: u16 = tcp_runtime.FLAG_FIN;
pub const FLAG_SYN: u16 = tcp_runtime.FLAG_SYN;
pub const FLAG_RST: u16 = 0x004;
pub const FLAG_PSH: u16 = 0x008;
pub const FLAG_ACK: u16 = 0x010;

// TcpOp.index transports compact parser/builder metadata across the private
// kernel <-> NETTCP.R4P dispatch without changing the fixed TcpOp layout.
const OPTION_MSS_MASK: u32 = 0x0000_FFFF;
const OPTION_WINDOW_SCALE_PRESENT: u32 = 1 << 16;
const OPTION_WINDOW_SCALE_SHIFT: u5 = 17;

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
    mss: u16 = 0,
    window_scale: u8 = 0,
    window_scale_present: bool = false,
    payload: []const u8,
};

pub const AckRequest = enum(u8) {
    none,
    delayed,
    immediate,
};

pub const WriteBlockReason = enum(u8) {
    none,
    remote_window,
    catalog,
    state,
};

pub const ConnectionInfo = r4x_api.TcpConnectionInfo;
pub const Summary = r4x_api.TcpSummary;
pub const ConnectionIdentity = tcp_runtime.ConnectionIdentity;
pub const RetransmitKind = tcp_runtime.SegmentKind;

pub const CleanupResult = extern struct {
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
    write_calls: u64 = 0,
    write_requested_bytes: u64 = 0,
    write_completed_bytes: u64 = 0,
    write_segments: u64 = 0,
    write_partial: u64 = 0,
    remote_window_stalls: u64 = 0,
    catalog_stalls: u64 = 0,
    backend_busy_stalls: u64 = 0,
    pure_ack_tx: u64 = 0,
    delayed_ack_requests: u64 = 0,
    delayed_ack_tx: u64 = 0,
    immediate_ack_tx: u64 = 0,
    ack_coalesced: u64 = 0,
    ack_piggybacked: u64 = 0,
    window_update_tx: u64 = 0,
    outstanding_segments_peak: u32 = 0,
    outstanding_bytes_peak: u32 = 0,
    outstanding_segments_current: u32 = 0,
    outstanding_bytes_current: u32 = 0,
    mss_negotiated: u64 = 0,
    window_scale_negotiated: u64 = 0,
    last_flags: u16 = 0,
    last_source_port: u16 = 0,
    last_dest_port: u16 = 0,
    last_seq: u32 = 0,
    last_ack: u32 = 0,
    last_payload_len: u32 = 0,
    last_error: []const u8 = "none",
};

pub const SendPlan = extern struct {
    local_port: u16,
    remote_port: u16,
    remote_ip: [4]u8,
    seq: u32,
    ack: u32,
    rx_window: u16,
    options: u32,
};

pub const RetransmitPlan = struct {
    local_port: u16,
    remote_port: u16,
    remote_ip: [4]u8,
    seq: u32,
    ack: u32,
    flags: u16,
    rx_window: u16,
    options: u32,
    payload_storage: [SENT_PAYLOAD_CAPACITY]u8,
    payload_len: u16,
    token: u64,
    sent_tick: u64,
    retransmits: u8,
    pub fn payloadSlice(self: *const RetransmitPlan) []const u8 {
        return self.payload_storage[0..self.payload_len];
    }
};

pub const OutstandingInfo = extern struct {
    sent_tick: u64,
    retransmits: u8,
};

const NativeSegmentView = extern struct {
    source_ip: [4]u8,
    dest_ip: [4]u8,
    source_port: u16,
    dest_port: u16,
    seq: u32,
    ack: u32,
    flags: u16,
    window: u16,
    mss: u16,
    window_scale: u8,
    window_scale_present: bool,
    payload: Text,
};
fn toNativeSegmentView(value: SegmentView) NativeSegmentView {
    return .{ .source_ip = value.source_ip, .dest_ip = value.dest_ip, .source_port = value.source_port, .dest_port = value.dest_port, .seq = value.seq, .ack = value.ack, .flags = value.flags, .window = value.window, .mss = value.mss, .window_scale = value.window_scale, .window_scale_present = value.window_scale_present, .payload = .{ .pointer = value.payload.ptr, .len = value.payload.len } };
}
fn fromNativeSegmentView(value: NativeSegmentView) SegmentView {
    return .{ .source_ip = value.source_ip, .dest_ip = value.dest_ip, .source_port = value.source_port, .dest_port = value.dest_port, .seq = value.seq, .ack = value.ack, .flags = value.flags, .window = value.window, .mss = value.mss, .window_scale = value.window_scale, .window_scale_present = value.window_scale_present, .payload = value.payload.slice() };
}

const NativeStats = extern struct {
    rx_segments: u64,
    tx_segments: u64,
    syn_tx: u64,
    synack_rx: u64,
    synack_tx: u64,
    ack_tx: u64,
    data_tx: u64,
    data_rx: u64,
    fin_tx: u64,
    rst_rx: u64,
    listen_syn_rx: u64,
    accepts: u64,
    retransmits: u64,
    rx_drops: u64,
    malformed: u64,
    checksum_errors: u64,
    timeouts: u64,
    self_tests: u64,
    write_calls: u64,
    write_requested_bytes: u64,
    write_completed_bytes: u64,
    write_segments: u64,
    write_partial: u64,
    remote_window_stalls: u64,
    catalog_stalls: u64,
    backend_busy_stalls: u64,
    pure_ack_tx: u64,
    delayed_ack_requests: u64,
    delayed_ack_tx: u64,
    immediate_ack_tx: u64,
    ack_coalesced: u64,
    ack_piggybacked: u64,
    window_update_tx: u64,
    outstanding_segments_peak: u32,
    outstanding_bytes_peak: u32,
    outstanding_segments_current: u32,
    outstanding_bytes_current: u32,
    mss_negotiated: u64,
    window_scale_negotiated: u64,
    last_flags: u16,
    last_source_port: u16,
    last_dest_port: u16,
    last_seq: u32,
    last_ack: u32,
    last_payload_len: u32,
    last_error: Text,
};
fn toNativeStats(value: Stats) NativeStats {
    return .{ .rx_segments = value.rx_segments, .tx_segments = value.tx_segments, .syn_tx = value.syn_tx, .synack_rx = value.synack_rx, .synack_tx = value.synack_tx, .ack_tx = value.ack_tx, .data_tx = value.data_tx, .data_rx = value.data_rx, .fin_tx = value.fin_tx, .rst_rx = value.rst_rx, .listen_syn_rx = value.listen_syn_rx, .accepts = value.accepts, .retransmits = value.retransmits, .rx_drops = value.rx_drops, .malformed = value.malformed, .checksum_errors = value.checksum_errors, .timeouts = value.timeouts, .self_tests = value.self_tests, .write_calls = value.write_calls, .write_requested_bytes = value.write_requested_bytes, .write_completed_bytes = value.write_completed_bytes, .write_segments = value.write_segments, .write_partial = value.write_partial, .remote_window_stalls = value.remote_window_stalls, .catalog_stalls = value.catalog_stalls, .backend_busy_stalls = value.backend_busy_stalls, .pure_ack_tx = value.pure_ack_tx, .delayed_ack_requests = value.delayed_ack_requests, .delayed_ack_tx = value.delayed_ack_tx, .immediate_ack_tx = value.immediate_ack_tx, .ack_coalesced = value.ack_coalesced, .ack_piggybacked = value.ack_piggybacked, .window_update_tx = value.window_update_tx, .outstanding_segments_peak = value.outstanding_segments_peak, .outstanding_bytes_peak = value.outstanding_bytes_peak, .outstanding_segments_current = value.outstanding_segments_current, .outstanding_bytes_current = value.outstanding_bytes_current, .mss_negotiated = value.mss_negotiated, .window_scale_negotiated = value.window_scale_negotiated, .last_flags = value.last_flags, .last_source_port = value.last_source_port, .last_dest_port = value.last_dest_port, .last_seq = value.last_seq, .last_ack = value.last_ack, .last_payload_len = value.last_payload_len, .last_error = .{ .pointer = value.last_error.ptr, .len = value.last_error.len } };
}
fn fromNativeStats(value: NativeStats) Stats {
    return .{ .rx_segments = value.rx_segments, .tx_segments = value.tx_segments, .syn_tx = value.syn_tx, .synack_rx = value.synack_rx, .synack_tx = value.synack_tx, .ack_tx = value.ack_tx, .data_tx = value.data_tx, .data_rx = value.data_rx, .fin_tx = value.fin_tx, .rst_rx = value.rst_rx, .listen_syn_rx = value.listen_syn_rx, .accepts = value.accepts, .retransmits = value.retransmits, .rx_drops = value.rx_drops, .malformed = value.malformed, .checksum_errors = value.checksum_errors, .timeouts = value.timeouts, .self_tests = value.self_tests, .write_calls = value.write_calls, .write_requested_bytes = value.write_requested_bytes, .write_completed_bytes = value.write_completed_bytes, .write_segments = value.write_segments, .write_partial = value.write_partial, .remote_window_stalls = value.remote_window_stalls, .catalog_stalls = value.catalog_stalls, .backend_busy_stalls = value.backend_busy_stalls, .pure_ack_tx = value.pure_ack_tx, .delayed_ack_requests = value.delayed_ack_requests, .delayed_ack_tx = value.delayed_ack_tx, .immediate_ack_tx = value.immediate_ack_tx, .ack_coalesced = value.ack_coalesced, .ack_piggybacked = value.ack_piggybacked, .window_update_tx = value.window_update_tx, .outstanding_segments_peak = value.outstanding_segments_peak, .outstanding_bytes_peak = value.outstanding_bytes_peak, .outstanding_segments_current = value.outstanding_segments_current, .outstanding_bytes_current = value.outstanding_bytes_current, .mss_negotiated = value.mss_negotiated, .window_scale_negotiated = value.window_scale_negotiated, .last_flags = value.last_flags, .last_source_port = value.last_source_port, .last_dest_port = value.last_dest_port, .last_seq = value.last_seq, .last_ack = value.last_ack, .last_payload_len = value.last_payload_len, .last_error = value.last_error.slice() };
}

const NativeRetransmitPlan = extern struct {
    local_port: u16,
    remote_port: u16,
    remote_ip: [4]u8,
    seq: u32,
    ack: u32,
    flags: u16,
    rx_window: u16,
    options: u32,
    payload_storage: [SENT_PAYLOAD_CAPACITY]u8,
    payload_len: u16,
    token: u64,
    sent_tick: u64,
    retransmits: u8,
};
fn toNativeRetransmitPlan(value: RetransmitPlan) NativeRetransmitPlan {
    return .{ .local_port = value.local_port, .remote_port = value.remote_port, .remote_ip = value.remote_ip, .seq = value.seq, .ack = value.ack, .flags = value.flags, .rx_window = value.rx_window, .options = value.options, .payload_storage = value.payload_storage, .payload_len = value.payload_len, .token = value.token, .sent_tick = value.sent_tick, .retransmits = value.retransmits };
}
fn fromNativeRetransmitPlan(value: NativeRetransmitPlan) RetransmitPlan {
    return .{ .local_port = value.local_port, .remote_port = value.remote_port, .remote_ip = value.remote_ip, .seq = value.seq, .ack = value.ack, .flags = value.flags, .rx_window = value.rx_window, .options = value.options, .payload_storage = value.payload_storage, .payload_len = value.payload_len, .token = value.token, .sent_tick = value.sent_tick, .retransmits = value.retransmits };
}

extern fn r4os_rust_net_tcp_connection_slot_bytes() usize;
extern fn r4os_rust_net_tcp_option_metadata(u16, u8) u32;
extern fn r4os_rust_net_tcp_option_mss(u32) u16;
extern fn r4os_rust_net_tcp_option_window_scale(u32, *u8) bool;
extern fn r4os_rust_net_tcp_reset() void;
extern fn r4os_rust_net_tcp_get_stats(*NativeStats) void;
extern fn r4os_rust_net_tcp_record_write_start(usize) void;
extern fn r4os_rust_net_tcp_record_write_finish(usize, usize, usize, u8) void;
extern fn r4os_rust_net_tcp_record_backend_busy() void;
extern fn r4os_rust_net_tcp_outstanding_totals(*u32, *u32) void;
extern fn r4os_rust_net_tcp_summary(*Summary) void;
extern fn r4os_rust_net_tcp_connection_info(u32, *ConnectionInfo) i32;
extern fn r4os_rust_net_tcp_self_test_connect(*const [4]u8, u16) i32;
extern fn r4os_rust_net_tcp_begin_live_connect(*const [4]u8, u16, u32) i32;
extern fn r4os_rust_net_tcp_listen(u16) bool;
extern fn r4os_rust_net_tcp_close_listener(u16) void;
extern fn r4os_rust_net_tcp_has_listener(u16) bool;
extern fn r4os_rust_net_tcp_accept_inbound(*const NativeSegmentView, u32, *u32) bool;
extern fn r4os_rust_net_tcp_abort(u32, [*]const u8, usize) void;
extern fn r4os_rust_net_tcp_mark_timeout([*]const u8, usize) void;
extern fn r4os_rust_net_tcp_record_malformed([*]const u8, usize) void;
extern fn r4os_rust_net_tcp_record_checksum_error() void;
extern fn r4os_rust_net_tcp_set_error([*]const u8, usize) void;
extern fn r4os_rust_net_tcp_established(u32) bool;
extern fn r4os_rust_net_tcp_request_close(u32) bool;
extern fn r4os_rust_net_tcp_close_ready(u32) bool;
extern fn r4os_rust_net_tcp_fin_waiting(u32) bool;
extern fn r4os_rust_net_tcp_connection_identity(u32, *ConnectionIdentity) bool;
extern fn r4os_rust_net_tcp_connection_identity_at(usize, *ConnectionIdentity) bool;
extern fn r4os_rust_net_tcp_identity_active(*const ConnectionIdentity) bool;
extern fn r4os_rust_net_tcp_connection_with_data_on_port(u16, *u32) bool;
extern fn r4os_rust_net_tcp_connection_on_port(u16, *u32) bool;
extern fn r4os_rust_net_tcp_claim_accepted(u32) bool;
extern fn r4os_rust_net_tcp_connection_id_for_segment(*const NativeSegmentView, *u32) bool;
extern fn r4os_rust_net_tcp_connection_identity_for_segment(*const NativeSegmentView, *ConnectionIdentity) bool;
extern fn r4os_rust_net_tcp_abort_connections_on_port(u16, [*]const u8, usize) void;
extern fn r4os_rust_net_tcp_abort_all([*]const u8, usize, *CleanupResult) void;
extern fn r4os_rust_net_tcp_plan_send(u32, u16, *SendPlan) bool;
extern fn r4os_rust_net_tcp_send_allowance(u32, usize) usize;
extern fn r4os_rust_net_tcp_write_block_reason(u32) u8;
extern fn r4os_rust_net_tcp_can_track_send(u32, u16, usize) bool;
extern fn r4os_rust_net_tcp_plan_retransmit(u32, u8, *NativeRetransmitPlan) bool;
extern fn r4os_rust_net_tcp_outstanding_info(u32, u8, *OutstandingInfo) bool;
extern fn r4os_rust_net_tcp_outstanding_count(u32) usize;
extern fn r4os_rust_net_tcp_outstanding_bytes(u32) usize;
extern fn r4os_rust_net_tcp_commit_sent(u32, u16, [*]const u8, usize, u64) bool;
extern fn r4os_rust_net_tcp_record_retransmit(u32, u64, u64, [*]const u8, usize) bool;
extern fn r4os_rust_net_tcp_tx_ack_of(u32) u32;
extern fn r4os_rust_net_tcp_rx_ack_of(u32) u32;
extern fn r4os_rust_net_tcp_request_ack(u32, u32, *const NativeSegmentView, u64, u64) u8;
extern fn r4os_rust_net_tcp_delayed_ack_due(usize, u64, *u32) bool;
extern fn r4os_rust_net_tcp_note_ack_sent(u32, bool, u16) void;
extern fn r4os_rust_net_tcp_window_update_required(u32) bool;
extern fn r4os_rust_net_tcp_readable(u32) bool;
extern fn r4os_rust_net_tcp_closed(u32) bool;
extern fn r4os_rust_net_tcp_loopback_write(u32, [*]const u8, usize) i32;
extern fn r4os_rust_net_tcp_read(u32, [*]u8, usize) i32;
extern fn r4os_rust_net_tcp_close(u32) i32;
extern fn r4os_rust_net_tcp_buffer_limit_probe() bool;
extern fn r4os_rust_net_tcp_out_of_order_probe() bool;
extern fn r4os_rust_net_tcp_apply_rx_view(*const NativeSegmentView, *NativeSegmentView) bool;
extern fn r4os_rust_net_tcp_apply_tx_view(*const NativeSegmentView, *NativeSegmentView) bool;
extern fn r4os_rust_net_tcp_reap_time_wait(u64) u32;
extern fn r4os_rust_net_tcp_overlap_retransmit_probe() bool;
extern fn r4os_rust_net_tcp_wraparound_probe() bool;
extern fn r4os_rust_net_tcp_graceful_close_probe() bool;
extern fn r4os_rust_net_tcp_performance_contract_probe() bool;
pub fn connectionSlotBytes() usize {
    return r4os_rust_net_tcp_connection_slot_bytes();
}
pub fn optionMetadata(mss: u16, window_scale: ?u8) u32 {
    return r4os_rust_net_tcp_option_metadata(mss, window_scale orelse 255);
}
pub fn optionMss(metadata: u32) u16 {
    return r4os_rust_net_tcp_option_mss(metadata);
}
pub fn optionWindowScale(metadata: u32) ?u8 {
    var out: u8 = undefined;
    return if (r4os_rust_net_tcp_option_window_scale(metadata, &out)) out else null;
}
pub fn reset() void {
    r4os_rust_net_tcp_reset();
}
pub fn getStats() Stats {
    var out: NativeStats = undefined;
    r4os_rust_net_tcp_get_stats(&out);
    return fromNativeStats(out);
}
pub fn recordWriteStart(requested: usize) void {
    r4os_rust_net_tcp_record_write_start(requested);
}
pub fn recordWriteFinish(requested: usize, written: usize, segments: usize, block: WriteBlockReason) void {
    r4os_rust_net_tcp_record_write_finish(requested, written, segments, @intFromEnum(block));
}
pub fn recordBackendBusy() void {
    r4os_rust_net_tcp_record_backend_busy();
}
pub fn outstandingTotals(segment_count: *u32, byte_count: *u32) void {
    r4os_rust_net_tcp_outstanding_totals(segment_count, byte_count);
}
pub fn summary(out: *Summary) void {
    r4os_rust_net_tcp_summary(out);
}
pub fn connectionInfo(index: u32, out: *ConnectionInfo) i32 {
    return r4os_rust_net_tcp_connection_info(index, out);
}
pub fn selfTestConnect(remote_ip: [4]u8, remote_port: u16) i32 {
    return r4os_rust_net_tcp_self_test_connect(&remote_ip, remote_port);
}
pub fn beginLiveConnect(remote_ip: [4]u8, remote_port: u16, initial_seq: u32) i32 {
    return r4os_rust_net_tcp_begin_live_connect(&remote_ip, remote_port, initial_seq);
}
pub fn listen(port: u16) bool {
    return r4os_rust_net_tcp_listen(port);
}
pub fn closeListener(port: u16) void {
    r4os_rust_net_tcp_close_listener(port);
}
pub fn hasListener(port: u16) bool {
    return r4os_rust_net_tcp_has_listener(port);
}
pub fn acceptInbound(view: SegmentView, initial_seq: u32) ?u32 {
    const native_view = toNativeSegmentView(view);
    var out: u32 = undefined;
    return if (r4os_rust_net_tcp_accept_inbound(&native_view, initial_seq, &out)) out else null;
}
pub fn abort(conn_id: u32, reason: []const u8) void {
    r4os_rust_net_tcp_abort(conn_id, reason.ptr, reason.len);
}
pub fn markTimeout(reason: []const u8) void {
    r4os_rust_net_tcp_mark_timeout(reason.ptr, reason.len);
}
pub fn recordMalformed(reason: []const u8) void {
    r4os_rust_net_tcp_record_malformed(reason.ptr, reason.len);
}
pub fn recordChecksumError() void {
    r4os_rust_net_tcp_record_checksum_error();
}
pub fn setError(reason: []const u8) void {
    r4os_rust_net_tcp_set_error(reason.ptr, reason.len);
}
pub fn established(conn_id: u32) bool {
    return r4os_rust_net_tcp_established(conn_id);
}
pub fn requestClose(conn_id: u32) bool {
    return r4os_rust_net_tcp_request_close(conn_id);
}
pub fn closeReady(conn_id: u32) bool {
    return r4os_rust_net_tcp_close_ready(conn_id);
}
pub fn finWaiting(conn_id: u32) bool {
    return r4os_rust_net_tcp_fin_waiting(conn_id);
}
pub fn connectionIdentity(conn_id: u32) ?ConnectionIdentity {
    var out: ConnectionIdentity = undefined;
    return if (r4os_rust_net_tcp_connection_identity(conn_id, &out)) out else null;
}
pub fn connectionIdentityAt(slot: usize) ?ConnectionIdentity {
    var out: ConnectionIdentity = undefined;
    return if (r4os_rust_net_tcp_connection_identity_at(slot, &out)) out else null;
}
pub fn identityActive(identity: ConnectionIdentity) bool {
    return r4os_rust_net_tcp_identity_active(&identity);
}
pub fn connectionWithDataOnPort(port: u16) ?u32 {
    var out: u32 = undefined;
    return if (r4os_rust_net_tcp_connection_with_data_on_port(port, &out)) out else null;
}
pub fn connectionOnPort(port: u16) ?u32 {
    var out: u32 = undefined;
    return if (r4os_rust_net_tcp_connection_on_port(port, &out)) out else null;
}
pub fn claimAccepted(conn_id: u32) bool {
    return r4os_rust_net_tcp_claim_accepted(conn_id);
}
pub fn connectionIdForSegment(view: SegmentView) ?u32 {
    const native_view = toNativeSegmentView(view);
    var out: u32 = undefined;
    return if (r4os_rust_net_tcp_connection_id_for_segment(&native_view, &out)) out else null;
}
pub fn connectionIdentityForSegment(view: SegmentView) ?ConnectionIdentity {
    const native_view = toNativeSegmentView(view);
    var out: ConnectionIdentity = undefined;
    return if (r4os_rust_net_tcp_connection_identity_for_segment(&native_view, &out)) out else null;
}
pub fn abortConnectionsOnPort(port: u16, reason: []const u8) void {
    r4os_rust_net_tcp_abort_connections_on_port(port, reason.ptr, reason.len);
}
pub fn abortAll(reason: []const u8) CleanupResult {
    var out: CleanupResult = undefined;
    r4os_rust_net_tcp_abort_all(reason.ptr, reason.len, &out);
    return out;
}
pub fn planSend(conn_id: u32, flags: u16) ?SendPlan {
    var out: SendPlan = undefined;
    return if (r4os_rust_net_tcp_plan_send(conn_id, flags, &out)) out else null;
}
pub fn sendAllowance(conn_id: u32, requested: usize) usize {
    return r4os_rust_net_tcp_send_allowance(conn_id, requested);
}
pub fn writeBlockReason(conn_id: u32) WriteBlockReason {
    return @enumFromInt(r4os_rust_net_tcp_write_block_reason(conn_id));
}
pub fn canTrackSend(conn_id: u32, flags: u16, payload_len: usize) bool {
    return r4os_rust_net_tcp_can_track_send(conn_id, flags, payload_len);
}
pub fn planRetransmit(conn_id: u32, kind: RetransmitKind) ?RetransmitPlan {
    var out: NativeRetransmitPlan = undefined;
    return if (r4os_rust_net_tcp_plan_retransmit(conn_id, @intFromEnum(kind), &out)) fromNativeRetransmitPlan(out) else null;
}
pub fn outstandingInfo(conn_id: u32, kind: RetransmitKind) ?OutstandingInfo {
    var out: OutstandingInfo = undefined;
    return if (r4os_rust_net_tcp_outstanding_info(conn_id, @intFromEnum(kind), &out)) out else null;
}
pub fn outstandingCount(conn_id: u32) usize {
    return r4os_rust_net_tcp_outstanding_count(conn_id);
}
pub fn outstandingBytes(conn_id: u32) usize {
    return r4os_rust_net_tcp_outstanding_bytes(conn_id);
}
pub fn commitSent(conn_id: u32, flags: u16, payload: []const u8, sent_tick: u64) bool {
    return r4os_rust_net_tcp_commit_sent(conn_id, flags, payload.ptr, payload.len, sent_tick);
}
pub fn recordRetransmit(conn_id: u32, token: u64, sent_tick: u64, reason: []const u8) bool {
    return r4os_rust_net_tcp_record_retransmit(conn_id, token, sent_tick, reason.ptr, reason.len);
}
pub fn txAckOf(conn_id: u32) u32 {
    return r4os_rust_net_tcp_tx_ack_of(conn_id);
}
pub fn rxAckOf(conn_id: u32) u32 {
    return r4os_rust_net_tcp_rx_ack_of(conn_id);
}
pub fn requestAck(conn_id: u32, previous_ack: u32, view: SegmentView, now: u64, delay_ticks: u64) AckRequest {
    const native_view = toNativeSegmentView(view);
    return @enumFromInt(r4os_rust_net_tcp_request_ack(conn_id, previous_ack, &native_view, now, delay_ticks));
}
pub fn delayedAckDue(slot: usize, now: u64) ?u32 {
    var out: u32 = undefined;
    return if (r4os_rust_net_tcp_delayed_ack_due(slot, now, &out)) out else null;
}
pub fn noteAckSent(conn_id: u32, piggybacked: bool, advertised_window: u16) void {
    r4os_rust_net_tcp_note_ack_sent(conn_id, piggybacked, advertised_window);
}
pub fn windowUpdateRequired(conn_id: u32) bool {
    return r4os_rust_net_tcp_window_update_required(conn_id);
}
pub fn readable(conn_id: u32) bool {
    return r4os_rust_net_tcp_readable(conn_id);
}
pub fn closed(conn_id: u32) bool {
    return r4os_rust_net_tcp_closed(conn_id);
}
pub fn loopbackWrite(conn_id: u32, data: []const u8) i32 {
    return r4os_rust_net_tcp_loopback_write(conn_id, data.ptr, data.len);
}
pub fn read(conn_id: u32, out: []u8) i32 {
    return r4os_rust_net_tcp_read(conn_id, out.ptr, out.len);
}
pub fn close(conn_id: u32) i32 {
    return r4os_rust_net_tcp_close(conn_id);
}
pub fn bufferLimitProbe() bool {
    return r4os_rust_net_tcp_buffer_limit_probe();
}
pub fn outOfOrderProbe() bool {
    return r4os_rust_net_tcp_out_of_order_probe();
}
pub fn applyRxView(view: SegmentView) ?SegmentView {
    const native_view = toNativeSegmentView(view);
    var out: NativeSegmentView = undefined;
    return if (r4os_rust_net_tcp_apply_rx_view(&native_view, &out)) fromNativeSegmentView(out) else null;
}
pub fn applyTxView(view: SegmentView) ?SegmentView {
    const native_view = toNativeSegmentView(view);
    var out: NativeSegmentView = undefined;
    return if (r4os_rust_net_tcp_apply_tx_view(&native_view, &out)) fromNativeSegmentView(out) else null;
}
pub fn reapTimeWait(now: u64) u32 {
    return r4os_rust_net_tcp_reap_time_wait(now);
}
pub fn overlapRetransmitProbe() bool {
    return r4os_rust_net_tcp_overlap_retransmit_probe();
}
pub fn wraparoundProbe() bool {
    return r4os_rust_net_tcp_wraparound_probe();
}
pub fn gracefulCloseProbe() bool {
    return r4os_rust_net_tcp_graceful_close_probe();
}
pub fn performanceContractProbe() bool {
    return r4os_rust_net_tcp_performance_contract_probe();
}

comptime {
    if (@sizeOf(NativeSegmentView) != 48 or @alignOf(NativeSegmentView) != 8 or @offsetOf(NativeSegmentView, "source_ip") != 0 or @offsetOf(NativeSegmentView, "dest_ip") != 4 or @offsetOf(NativeSegmentView, "source_port") != 8 or @offsetOf(NativeSegmentView, "dest_port") != 10 or @offsetOf(NativeSegmentView, "seq") != 12 or @offsetOf(NativeSegmentView, "ack") != 16 or @offsetOf(NativeSegmentView, "flags") != 20 or @offsetOf(NativeSegmentView, "window") != 22 or @offsetOf(NativeSegmentView, "mss") != 24 or @offsetOf(NativeSegmentView, "window_scale") != 26 or @offsetOf(NativeSegmentView, "window_scale_present") != 27 or @offsetOf(NativeSegmentView, "payload") != 32) @compileError("Native network NativeSegmentView layout");
}

comptime {
    if (@sizeOf(NativeStats) != 336 or @alignOf(NativeStats) != 8 or @offsetOf(NativeStats, "rx_segments") != 0 or @offsetOf(NativeStats, "tx_segments") != 8 or @offsetOf(NativeStats, "syn_tx") != 16 or @offsetOf(NativeStats, "synack_rx") != 24 or @offsetOf(NativeStats, "synack_tx") != 32 or @offsetOf(NativeStats, "ack_tx") != 40 or @offsetOf(NativeStats, "data_tx") != 48 or @offsetOf(NativeStats, "data_rx") != 56 or @offsetOf(NativeStats, "fin_tx") != 64 or @offsetOf(NativeStats, "rst_rx") != 72 or @offsetOf(NativeStats, "listen_syn_rx") != 80 or @offsetOf(NativeStats, "accepts") != 88 or @offsetOf(NativeStats, "retransmits") != 96 or @offsetOf(NativeStats, "rx_drops") != 104 or @offsetOf(NativeStats, "malformed") != 112 or @offsetOf(NativeStats, "checksum_errors") != 120 or @offsetOf(NativeStats, "timeouts") != 128 or @offsetOf(NativeStats, "self_tests") != 136 or @offsetOf(NativeStats, "write_calls") != 144 or @offsetOf(NativeStats, "write_requested_bytes") != 152 or @offsetOf(NativeStats, "write_completed_bytes") != 160 or @offsetOf(NativeStats, "write_segments") != 168 or @offsetOf(NativeStats, "write_partial") != 176 or @offsetOf(NativeStats, "remote_window_stalls") != 184 or @offsetOf(NativeStats, "catalog_stalls") != 192 or @offsetOf(NativeStats, "backend_busy_stalls") != 200 or @offsetOf(NativeStats, "pure_ack_tx") != 208 or @offsetOf(NativeStats, "delayed_ack_requests") != 216 or @offsetOf(NativeStats, "delayed_ack_tx") != 224 or @offsetOf(NativeStats, "immediate_ack_tx") != 232 or @offsetOf(NativeStats, "ack_coalesced") != 240 or @offsetOf(NativeStats, "ack_piggybacked") != 248 or @offsetOf(NativeStats, "window_update_tx") != 256 or @offsetOf(NativeStats, "outstanding_segments_peak") != 264 or @offsetOf(NativeStats, "outstanding_bytes_peak") != 268 or @offsetOf(NativeStats, "outstanding_segments_current") != 272 or @offsetOf(NativeStats, "outstanding_bytes_current") != 276 or @offsetOf(NativeStats, "mss_negotiated") != 280 or @offsetOf(NativeStats, "window_scale_negotiated") != 288 or @offsetOf(NativeStats, "last_flags") != 296 or @offsetOf(NativeStats, "last_source_port") != 298 or @offsetOf(NativeStats, "last_dest_port") != 300 or @offsetOf(NativeStats, "last_seq") != 304 or @offsetOf(NativeStats, "last_ack") != 308 or @offsetOf(NativeStats, "last_payload_len") != 312 or @offsetOf(NativeStats, "last_error") != 320) @compileError("Native network NativeStats layout");
}

comptime {
    if (@sizeOf(CleanupResult) != 8 or @alignOf(CleanupResult) != 4 or @offsetOf(CleanupResult, "connections") != 0 or @offsetOf(CleanupResult, "listeners") != 4) @compileError("Native network CleanupResult layout");
}

comptime {
    if (@sizeOf(SendPlan) != 24 or @alignOf(SendPlan) != 4 or @offsetOf(SendPlan, "local_port") != 0 or @offsetOf(SendPlan, "remote_port") != 2 or @offsetOf(SendPlan, "remote_ip") != 4 or @offsetOf(SendPlan, "seq") != 8 or @offsetOf(SendPlan, "ack") != 12 or @offsetOf(SendPlan, "rx_window") != 16 or @offsetOf(SendPlan, "options") != 20) @compileError("Native network SendPlan layout");
}

comptime {
    if (@sizeOf(OutstandingInfo) != 16 or @alignOf(OutstandingInfo) != 8 or @offsetOf(OutstandingInfo, "sent_tick") != 0 or @offsetOf(OutstandingInfo, "retransmits") != 8) @compileError("Native network OutstandingInfo layout");
}

comptime {
    if (@sizeOf(NativeRetransmitPlan) != 1512 or @alignOf(NativeRetransmitPlan) != 8 or @offsetOf(NativeRetransmitPlan, "local_port") != 0 or @offsetOf(NativeRetransmitPlan, "remote_port") != 2 or @offsetOf(NativeRetransmitPlan, "remote_ip") != 4 or @offsetOf(NativeRetransmitPlan, "seq") != 8 or @offsetOf(NativeRetransmitPlan, "ack") != 12 or @offsetOf(NativeRetransmitPlan, "flags") != 16 or @offsetOf(NativeRetransmitPlan, "rx_window") != 18 or @offsetOf(NativeRetransmitPlan, "options") != 20 or @offsetOf(NativeRetransmitPlan, "payload_storage") != 24 or @offsetOf(NativeRetransmitPlan, "payload_len") != 1484 or @offsetOf(NativeRetransmitPlan, "token") != 1488 or @offsetOf(NativeRetransmitPlan, "sent_tick") != 1496 or @offsetOf(NativeRetransmitPlan, "retransmits") != 1504) @compileError("Native network NativeRetransmitPlan layout");
}
