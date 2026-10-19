//! Original NIPC network service coordination and text/binary replies.
//! Request scratch belongs to this activation; native metadata admission ends
//! before TCP, IPC, R4D/R4P, console or filesystem calls. Wire codecs never read
//! Rust padding. Queue admission retains typed resident native handlers.
use super::{cleanup, ipc_net as net, ipc_state as state, ipc_types::*, timing};
use crate::{
    boot::log,
    fs::vfs_types::Bytes,
    sched::sync_types::Text,
    service::{ipc, ipc_types as channel},
};
use core::{ptr, slice};
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Idle,
    Pending,
    Ok,
    Timeout,
    Failed,
    Cancelled,
    WouldBlock,
}
trait View {
    fn view(&self) -> &[u8];
}
impl View for Text {
    fn view(&self) -> &[u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.pointer, self.len) }
        }
    }
}
impl View for &[u8] {
    fn view(&self) -> &[u8] {
        self
    }
}
impl<const N: usize> View for &[u8; N] {
    fn view(&self) -> &[u8] {
        &self[..]
    }
}
impl<const N: usize> View for [u8; N] {
    fn view(&self) -> &[u8] {
        &self[..]
    }
}
struct Writer<'a> {
    out: &'a mut [u8],
    pos: usize,
}
impl<'a> Writer<'a> {
    fn new(out: &'a mut [u8]) -> Self {
        Self { out, pos: 0 }
    }
    fn put(&mut self, ch: u8) {
        if self.pos >= self.out.len() {
            return;
        }
        self.out[self.pos] = ch;
        self.pos += 1;
    }
    fn text(&mut self, value: impl View) {
        for &ch in value.view() {
            if ch != 0 {
                self.put(ch);
            }
        }
    }
    fn bytes(&mut self, value: &[u8]) {
        for &ch in value {
            self.put(ch);
        }
    }
    fn slice(&self) -> &[u8] {
        &self.out[..self.pos]
    }
    fn num(&mut self, value: impl TryInto<u64>) {
        let mut number = value
            .try_into()
            .ok()
            .expect("Nonnegative bounded IPC number");
        let mut buf = [0; 20];
        let mut pos = buf.len();
        if number == 0 {
            self.put(b'0');
            return;
        }
        while number > 0 {
            pos -= 1;
            buf[pos] = b'0' + (number % 10) as u8;
            number /= 10;
        }
        self.text(&buf[pos..]);
    }
    fn ip(&mut self, value: [u8; 4]) {
        for (index, byte) in value.into_iter().enumerate() {
            if index != 0 {
                self.put(b'.');
            }
            self.num(byte);
        }
    }
    fn signed(&mut self, value: i32) {
        if value < 0 {
            self.put(b'-');
            self.num(value.checked_neg().expect("Original signed IPC range") as u32);
        } else {
            self.num(value as u32);
        }
    }
}
fn int<T: TryFrom<S>, S>(value: S) -> T {
    T::try_from(value)
        .ok()
        .expect("Original checked IPC integer conversion")
}
fn copy_bounded(out: &mut [u8], value: impl View) {
    out.fill(0);
    if out.is_empty() {
        return;
    }
    let value = value.view();
    let len = value.len().min(out.len() - 1);
    out[..len].copy_from_slice(&value[..len]);
}
fn string_len_z(value: &[u8]) -> usize {
    value.iter().position(|&ch| ch == 0).unwrap_or(value.len())
}
fn fixed_text(value: &[u8]) -> &[u8] {
    &value[..string_len_z(value)]
}
fn text_equals(a: impl View, b: impl View) -> bool {
    a.view() == b.view()
}
fn text_contains(a: impl View, b: impl View) -> bool {
    super::core_bytes::mem_contains(a.view(), b.view())
}
fn net_tx_result_code(result: net::TxResult) -> i32 {
    result as i32
}
fn service_semantic_flags(flags: u32) -> u32 {
    flags & !SERVICE_STATUS_MASK
}
fn with_service_status(flags: u32, status: Status) -> u32 {
    service_semantic_flags(flags) | ((status as u32) << SERVICE_STATUS_SHIFT)
}
fn service_status_encoded(flags: u32) -> bool {
    flags & SERVICE_STATUS_MASK != 0
}
fn service_operation_status(flags: u32) -> Status {
    match (flags & SERVICE_STATUS_MASK) >> SERVICE_STATUS_SHIFT {
        0 => Status::Idle,
        1 => Status::Pending,
        2 => Status::Ok,
        3 => Status::Timeout,
        4 => Status::Failed,
        5 => Status::Cancelled,
        6 => Status::WouldBlock,
        _ => Status::Failed,
    }
}
fn service_operation_status_code(flags: u32) -> u32 {
    service_operation_status(flags) as u32
}
fn service_status_from_tx_result(result: net::TxResult, last_error: impl View) -> Status {
    match result {
        net::TxResult::Ok => Status::Ok,
        net::TxResult::Busy => Status::WouldBlock,
        net::TxResult::BackendError => {
            if text_contains(last_error, b"timeout") {
                Status::Timeout
            } else {
                Status::Failed
            }
        }
        _ => Status::Failed,
    }
}

fn record_service_close(status: Status) {
    state::record_close(status as u8);
}

fn socket_lifecycle_name(cause: u32) -> &'static [u8] {
    match cause {
        1 => b"active",
        2 => b"closed",
        3 => b"reset",
        4 => b"timeout",
        5 => b"peer-gone",
        6 => b"local-abort",
        7 => b"local-close",
        8 => b"pending-close",
        9 => b"would-block",
        10 => b"bad-handle",
        11 => b"owner-mismatch",
        12 => b"listener",
        13 => b"dropped",
        _ => b"unknown",
    }
}

fn read_u16(data: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(data[at..at + 2].try_into().unwrap())
}
fn read_u32(data: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
}
fn read_u64(data: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(data[at..at + 8].try_into().unwrap())
}
fn read_i32(data: &[u8], at: usize) -> i32 {
    read_u32(data, at) as i32
}
fn write_u16(data: &mut [u8], at: usize, value: u16) {
    data[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn write_u32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn write_header(
    out: &mut [u8],
    channel: u32,
    op: u16,
    id: u32,
    status: i32,
    len: u16,
) -> Option<()> {
    if out.len() < HEADER_SIZE {
        return None;
    }
    write_u32(out, 0, MAGIC);
    write_u16(out, 4, VERSION);
    write_u16(out, 6, int(channel));
    write_u16(out, 8, op);
    write_u16(out, 10, 0);
    write_u32(out, 12, id);
    write_u16(out, 16, 0);
    write_u16(out, 18, len);
    write_u32(out, 20, status as u32);
    Some(())
}
fn write_response(
    out: &mut [u8],
    channel: u32,
    op: u16,
    id: u32,
    status: i32,
    payload: &[u8],
) -> i32 {
    if out.len() < HEADER_SIZE {
        return RESULT_BAD_REQUEST;
    }
    let len = payload.len().min(out.len() - HEADER_SIZE);
    if write_header(out, channel, op, id, status, int(len)).is_none() {
        return RESULT_BAD_REQUEST;
    }
    if len != 0 {
        out[HEADER_SIZE..HEADER_SIZE + len].copy_from_slice(&payload[..len]);
    }
    int(HEADER_SIZE + len)
}
fn write_error(out: &mut [u8], channel: u32, op: u16, id: u32, status: i32) -> i32 {
    write_response(out, channel, op, id, status, b"")
}
struct DeadlineRequest<'a> {
    payload: &'a [u8],
    deadline_tick: Option<u64>,
}
fn split_service_deadline(payload: &[u8]) -> DeadlineRequest<'_> {
    let size = core::mem::size_of::<crate::contract::ServiceDeadlineFooter>();
    if payload.len() < size {
        return DeadlineRequest {
            payload,
            deadline_tick: None,
        };
    }
    let footer = &payload[payload.len() - size..];
    let expected = crate::contract::ServiceDeadlineFooter::default();
    if read_u32(footer, 0) != expected.magic
        || read_u16(footer, 4) != expected.version
        || usize::from(read_u16(footer, 6)) != size
        || read_u32(footer, 8) != int(payload.len() - size)
        || read_u32(footer, 12) != 0
    {
        return DeadlineRequest {
            payload,
            deadline_tick: None,
        };
    }
    DeadlineRequest {
        payload: &payload[..payload.len() - size],
        deadline_tick: Some(read_u64(footer, 16)),
    }
}
fn service_name(id: u32) -> &'static [u8] {
    match id {
        2 => b"net.dhcp",
        3 => b"net.dns",
        4 => b"net.tcp",
        5 => b"net.udp",
        _ => b"unknown",
    }
}
struct QueueStatus {
    channels: u32,
    handlers: u32,
    queued: u32,
    limit: u32,
    drops: u64,
}
fn queue_status() -> QueueStatus {
    let mut result = QueueStatus {
        channels: 0,
        handlers: 0,
        queued: 0,
        limit: 0,
        drops: 0,
    };
    for id in 2..=5 {
        let mut info = channel::ChannelInfo::EMPTY;
        unsafe {
            ipc::r4os_rust_ipc_channel(id, &mut info);
        }
        result.channels += 1;
        result.limit += info.queue_depth;
        result.queued += info.queued;
        result.drops += info.drops;
        if info.has_handler != 0 {
            result.handlers += 1;
        }
    }
    result
}
fn drain_service_queues() {
    let mut scratch = [0; ipc::MAX_MESSAGE_SIZE];
    for id in 2..=5 {
        while unsafe { ipc::recv(id, scratch.as_mut_ptr(), scratch.len()) } > 0 {}
    }
}

fn init() {
    for id in 2..=5 {
        unsafe {
            net_ipc_register(id);
        }
    }
}
fn restart_services() -> RestartStatus {
    let before = queue_status();
    state::record_restart(before.queued);
    let cleanup = unsafe { cleanup::all(Text::literal(b"service-restart")) };
    state::note_cleanup(
        b"service-restart",
        cleanup.last_tcp_connections,
        cleanup.last_tcp_listeners,
    );
    state::clear_handles();
    drain_service_queues();
    init();
    state::snapshot().restart
}
fn service_error_count() -> u64 {
    let status = queue_status();
    let bp = state::backpressure();
    let lifecycle = state::snapshot().lifecycle;
    status.drops
        + bp.payload_too_large
        + bp.response_buffer_small
        + bp.send_failures
        + lifecycle.close_failed
        + lifecycle.close_cancelled
        + lifecycle.cleanup_aborts
}
struct TcpResultPayload<'a> {
    result: TcpServiceResult,
    data: &'a [u8],
}

struct UdpResultPayload<'a> {
    result: UdpServiceResult,
    data: &'a [u8],
}

impl<'a> DeadlineRequest<'a> {
    const EMPTY: Self = Self {
        payload: &[],
        deadline_tick: None,
    };
}
fn operation_status_name(status: Status) -> Text {
    timing::operation_status_name(status as u8)
}

fn wire_reply<T: Wire>(
    response: &mut [u8],
    scratch: &mut [u8],
    channel: u32,
    op: u16,
    id: u32,
    value: T,
) -> i32 {
    let len = value.encode(scratch);
    write_response(response, channel, op, id, RESULT_OK, &scratch[..len])
}
fn handle(channel: u32, request: &[u8], response: &mut [u8]) -> i32 {
    if request.len() < HEADER_SIZE {
        return write_error(response, channel, 0, 0, RESULT_BAD_REQUEST);
    }
    if read_u32(request, 0) != MAGIC || read_u16(request, 4) != VERSION {
        return write_error(response, channel, 0, 0, RESULT_BAD_REQUEST);
    }
    let service = read_u16(request, 6);
    let op = read_u16(request, 8);
    let id = read_u32(request, 12);
    let client = read_u16(request, 16);
    let payload_len = usize::from(read_u16(request, 18));
    if HEADER_SIZE + payload_len > request.len() {
        return write_error(response, channel, op, id, RESULT_BAD_REQUEST);
    }
    let payload = &request[HEADER_SIZE..HEADER_SIZE + payload_len];
    if u32::from(service) != channel {
        return write_error(response, channel, op, id, RESULT_BAD_SERVICE);
    }
    let mut scratch = [0; TCP_MESSAGE_PAYLOAD_MAX];
    let mut w = Writer::new(&mut scratch);
    if op == OP_SERVICE_RESTART {
        let status = restart_services();
        write_restart_status(&mut w, status);
        write_restart_diagnostics(&mut w);
        return write_response(response, channel, op, id, RESULT_OK, w.slice());
    }
    if channel == 2 && op == OP_DHCP_STATUS_RESULT {
        return wire_reply(
            response,
            &mut scratch,
            channel,
            op,
            id,
            make_dhcp_status_result(),
        );
    }
    if channel == 2 && matches!(op, OP_DHCP_ACQUIRE | OP_DHCP_RENEW | OP_DHCP_RELEASE) {
        write_dhcp_operation(&mut w, op, payload);
        return write_response(response, channel, op, id, RESULT_OK, w.slice());
    }
    if channel == 2
        && matches!(
            op,
            OP_DHCP_ACQUIRE_RESULT | OP_DHCP_RENEW_RESULT | OP_DHCP_RELEASE_RESULT
        )
    {
        return wire_reply(
            response,
            &mut scratch,
            channel,
            op,
            id,
            make_dhcp_operation_result(op, payload),
        );
    }
    if channel == 3 && matches!(op, OP_DNS_RESOLVE_A | OP_DNS_RESOLVE_A_SERVER) {
        write_dns_resolve(&mut w, op, payload);
        return write_response(response, channel, op, id, RESULT_OK, w.slice());
    }
    if channel == 3 && op == OP_DNS_STATUS_RESULT {
        return wire_reply(
            response,
            &mut scratch,
            channel,
            op,
            id,
            make_dns_status_result(),
        );
    }
    if channel == 3 && matches!(op, OP_DNS_RESOLVE_A_RESULT | OP_DNS_RESOLVE_A_SERVER_RESULT) {
        return wire_reply(
            response,
            &mut scratch,
            channel,
            op,
            id,
            make_dns_resolve_result(op, payload),
        );
    }
    if channel == 4 && op == OP_TCP_STATUS_RESULT {
        return wire_reply(
            response,
            &mut scratch,
            channel,
            op,
            id,
            make_tcp_status_result(client),
        );
    }
    if channel == 4 && is_tcp_operation(op) {
        write_tcp_operation(&mut w, op, payload, client);
        return write_response(response, channel, op, id, RESULT_OK, w.slice());
    }
    if channel == 4 && is_tcp_result_operation(op) {
        return handle_tcp_result_operation(response, channel, op, id, payload, client);
    }
    if channel == 5 && op == OP_UDP_STATUS_RESULT {
        return wire_reply(
            response,
            &mut scratch,
            channel,
            op,
            id,
            make_udp_status_result(),
        );
    }
    if channel == 5 && is_udp_operation(op) {
        write_udp_operation(&mut w, op, payload);
        return write_response(response, channel, op, id, RESULT_OK, w.slice());
    }
    if channel == 5 && is_udp_result_operation(op) {
        return handle_udp_result_operation(response, channel, op, id, payload);
    }
    if op != OP_STATUS {
        return write_error(response, channel, op, id, RESULT_BAD_OP);
    }
    match channel {
        2 => write_dhcp_status(&mut w),
        3 => write_dns_status(&mut w),
        4 => write_tcp_status(&mut w),
        5 => write_udp_status(&mut w),
        _ => return write_error(response, channel, op, id, RESULT_BAD_SERVICE),
    }
    write_response(response, channel, op, id, RESULT_OK, w.slice())
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_ipc_init() {
    init();
}
/// # Safety
/// Registration's Zig invoker lends separate immutable request and exclusive
/// response storage, retained by the exact native IPC running ticket until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_handle(
    channel: u32,
    request: *const u8,
    len: usize,
    response: *mut u8,
    capacity: usize,
) -> i32 {
    let request = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(request, len) }
    };
    let response = if capacity == 0 {
        &mut []
    } else {
        unsafe { slice::from_raw_parts_mut(response, capacity) }
    };
    handle(channel, request, response)
}
/// # Safety
/// Readable foreign payload and exclusive foreign response remain retained by
/// the caller through IPC completion. No foreign reference spans the wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_request(
    channel: u32,
    op: u16,
    id: u32,
    client: u16,
    data: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> i32 {
    if len > TCP_MESSAGE_PAYLOAD_MAX {
        state::note_payload_too_large();
        return RESULT_BAD_REQUEST;
    }
    if capacity < HEADER_SIZE {
        state::note_response_small();
        return RESULT_BAD_REQUEST;
    }
    let mut message = [0; ipc::MAX_MESSAGE_SIZE];
    if write_header(&mut message, channel, op, id, RESULT_OK, int(len)).is_none() {
        return RESULT_BAD_REQUEST;
    }
    write_u16(&mut message, 16, client);
    if len != 0 {
        unsafe {
            ptr::copy_nonoverlapping(data, message.as_mut_ptr().add(HEADER_SIZE), len);
        }
    }
    let got = unsafe {
        ipc::request(
            channel,
            Bytes {
                data: message.as_ptr(),
                len: HEADER_SIZE + len,
            },
            out,
            capacity,
            u64::MAX,
        )
    };
    if got < 0 {
        if capacity < ipc::MAX_MESSAGE_SIZE {
            state::note_response_small();
        }
        state::note_send_failure();
        return RESULT_BAD_SERVICE;
    }
    got
}
/// # Safety
/// Exact paired resident output; the original reason argument was unused.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_restart(out: *mut RestartStatus) {
    let result = restart_services();
    unsafe {
        out.write(result);
    }
}
/// # Safety
/// Exclusive paired output; metadata copied before touching the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_restart_status(out: *mut RestartStatus) {
    let result = state::snapshot().restart;
    unsafe {
        out.write(result);
    }
}
/// # Safety
/// Exclusive paired output; metadata copied before touching the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_backpressure(out: *mut ServiceBackpressureStatus) {
    let result = state::backpressure();
    unsafe {
        out.write(result);
    }
}
/// # Safety
/// Exclusive paired output; metadata copied before touching the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_lifecycle(out: *mut ServiceLifecycleStatus) {
    let result = state::snapshot().lifecycle;
    unsafe {
        out.write(result);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_ipc_stale_skips() -> u64 {
    state::snapshot().stale_responses_skipped
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_ipc_error_count() -> u64 {
    service_error_count()
}
/// # Safety
/// Stable kernel reason bytes through this call; counts already copied from
/// the exact completed network cleanup. No resource is retired here a second time.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_note_cleanup(
    reason: Text,
    connections: u32,
    listeners: u32,
) {
    state::note_cleanup(state::text(reason), connections, listeners);
}
/// # Safety
/// Writable paired output for the static registered service name.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_service_name(id: u32, out: *mut Text) {
    let name = service_name(id);
    unsafe {
        out.write(Text::literal(name));
    }
}
/// # Safety
/// Readable complete response, exclusive status/length outputs. Returned view
/// belongs to the same retained response and is never stored by the kernel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_parse(
    data: *const u8,
    len: usize,
    status: *mut i32,
    length: *mut usize,
) -> *const u8 {
    if len < HEADER_SIZE {
        return ptr::null();
    }
    let response = unsafe { slice::from_raw_parts(data, len) };
    if read_u32(response, 0) != MAGIC || read_u16(response, 4) != VERSION {
        return ptr::null();
    }
    let payload_len = usize::from(read_u16(response, 18));
    if HEADER_SIZE + payload_len > len {
        return ptr::null();
    }
    unsafe {
        status.write(read_i32(response, 20));
        length.write(payload_len);
        data.add(HEADER_SIZE)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_ipc_semantic_flags(flags: u32) -> u32 {
    service_semantic_flags(flags)
}
/// # Safety
/// Exclusive paired output for a static operation status label.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_status_name(flags: u32, out: *mut Text) {
    unsafe {
        out.write(operation_status_name(service_operation_status(flags)));
    }
}
fn boot_text(value: impl View) {
    let value = value.view();
    unsafe {
        log::r4os_rust_bootlog_write(value.as_ptr(), value.len());
    }
}
/// # Safety
/// Readable stable kernel reason label through logging, without a held owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipc_log_errors(reason: Text) {
    let status = queue_status();
    let bp = state::backpressure();
    let lifecycle = state::snapshot().lifecycle;
    boot_text(b"[NETSVC][ERR] ");
    boot_text(reason);
    boot_text(b" total=");
    log::r4os_rust_bootlog_dec(service_error_count());
    boot_text(b" drops=");
    log::r4os_rust_bootlog_dec(status.drops);
    boot_text(b" req_large=");
    log::r4os_rust_bootlog_dec(bp.payload_too_large);
    boot_text(b" resp_small=");
    log::r4os_rust_bootlog_dec(bp.response_buffer_small);
    boot_text(b" send_fail=");
    log::r4os_rust_bootlog_dec(bp.send_failures);
    boot_text(b" close_fail=");
    log::r4os_rust_bootlog_dec(lifecycle.close_failed);
    boot_text(b" close_cancel=");
    log::r4os_rust_bootlog_dec(lifecycle.close_cancelled);
    boot_text(b" cleanup_abort=");
    log::r4os_rust_bootlog_dec(lifecycle.cleanup_aborts);
    boot_text(b"\r\n");
}
// Ported original coordinators/formatters. Text labels and branch order remain
// bound to ipc-before-native.zig in the migration evidence.
fn write_restart_status(w: &mut Writer<'_>, status: RestartStatus) {
    w.text(b"ok restarts=".as_slice());
    w.num(status.restarts);
    w.text(b" last_queue=".as_slice());
    w.num(status.last_queued);
    w.text(b" last_handles=".as_slice());
    w.num(status.last_handles);
    w.text(b" last_tomb=".as_slice());
    w.num(status.last_tombstones);
}
fn write_service_status_line(w: &mut Writer<'_>) {
    let status = state::snapshot().restart;
    w.text(b"  response_match=channel/op/request stale_skips=".as_slice());
    w.num(state::snapshot().stale_responses_skipped);
    w.text(b" restarts=".as_slice());
    w.num(status.restarts);
    w.text(b" last_queue=".as_slice());
    w.num(status.last_queued);
    w.text(b" last_handles=".as_slice());
    w.num(status.last_handles);
    w.text(b" last_tomb=".as_slice());
    w.num(status.last_tombstones);
}
fn write_cleanup_status_line(w: &mut Writer<'_>) {
    let s = net::cleanup_status();
    w.text(b"  Cleanup: runs=".as_slice());
    w.num(s.runs);
    w.text(b" link_down=".as_slice());
    w.num(s.link_down_cleanups);
    w.text(b" reset=".as_slice());
    w.num(s.adapter_reset_cleanups);
    w.text(b" unreg=".as_slice());
    w.num(s.adapter_unregister_cleanups);
    w.text(b" svc_restart=".as_slice());
    w.num(s.service_restart_cleanups);
    w.text(b" poweroff=".as_slice());
    w.num(s.poweroff_cleanups);
    w.text(b" reboot=".as_slice());
    w.num(s.reboot_cleanups);
    w.text(b" udp_closed=".as_slice());
    w.num(s.udp_sockets_closed);
    w.text(b" tcp_abort=".as_slice());
    w.num(s.tcp_connections_aborted);
    w.text(b" tcp_listen=".as_slice());
    w.num(s.tcp_listeners_closed);
    w.text(b" dhcp_cancel=".as_slice());
    w.num(s.dhcp_operations_cancelled);
    w.text(b" dns_cancel=".as_slice());
    w.num(s.dns_operations_cancelled);
    w.text(b" last_udp=".as_slice());
    w.num(s.last_udp_closed);
    w.text(b" last_tcp=".as_slice());
    w.num(s.last_tcp_connections);
    w.text(b"/".as_slice());
    w.num(s.last_tcp_listeners);
    w.text(b" last=".as_slice());
    w.text(s.last_reason);
}
fn write_restart_diagnostics(w: &mut Writer<'_>) {
    w.text(b"\r\nIPC network services\r\n".as_slice());
    write_service_status_line(w);
    w.text(b"\r\n".as_slice());
    write_cleanup_status_line(w);
}

unsafe fn net_ipc_register(id: u32) {
    let name = service_name(id);
    unsafe {
        crate::service::ipc::register(
            id,
            Bytes {
                data: name.as_ptr(),
                len: name.len(),
            },
            Some(native_handler),
            true,
        );
    }
}

unsafe fn native_handler(id: u32, data: Bytes, output: *mut u8, capacity: usize) -> i32 {
    unsafe { r4os_rust_net_ipc_handle(id, data.data, data.len, output, capacity) }
}

// Private operation modules share the sole state and original typed owners.
mod dhcp;
mod dns;
mod tcp_operations;
mod tcp_text;
mod udp;
use dhcp::{
    make_dhcp_operation_result, make_dhcp_status_result, write_dhcp_operation, write_dhcp_status,
};
use dns::{make_dns_resolve_result, make_dns_status_result, write_dns_resolve, write_dns_status};
use tcp_operations::{
    fill_tcp_connection_fields, handle_tcp_result_operation, is_tcp_operation,
    is_tcp_result_operation, make_tcp_status_result,
};
use tcp_text::{write_tcp_operation, write_tcp_status};
use udp::{
    handle_udp_result_operation, is_udp_operation, is_udp_result_operation, make_udp_status_result,
    write_udp_operation, write_udp_status,
};
