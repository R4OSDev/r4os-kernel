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
fn service_status_from_dns_result(result: i32) -> Status {
    match result {
        super::dns::RESULT_OK => Status::Ok,
        super::dns::RESULT_TIMEOUT => Status::Timeout,
        _ => Status::Failed,
    }
}
fn set_tcp_service_status(out: &mut TcpServiceResult, status: Status) {
    out.flags = with_service_status(out.flags, status);
    out.service_status = status as u32;
}
fn set_udp_service_status(out: &mut UdpServiceResult, status: Status) {
    out.flags = with_service_status(out.flags, status);
    out.service_status = status as u32;
}
fn record_service_close(status: Status) {
    state::record_close(status as u8);
}
fn set_tcp_lifecycle(out: &mut TcpServiceResult, cause: u32) {
    if cause == 0 || out.lifecycle_cause == cause {
        return;
    }
    out.lifecycle_cause = cause;
    out.flags |= TCP_FLAG_LIFECYCLE_VALID;
    state::record_tcp(cause);
}
fn set_udp_lifecycle(out: &mut UdpServiceResult, cause: u32) {
    if cause == 0 || out.lifecycle_cause == cause {
        return;
    }
    out.lifecycle_cause = cause;
    out.flags |= UDP_FLAG_LIFECYCLE_VALID;
    state::record_udp(cause);
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
fn tcp_state_name(state: u8) -> &'static [u8] {
    match state {
        0 => b"closed",
        1 => b"syn-sent",
        2 => b"established",
        3 => b"fin-wait",
        4 => b"syn-received",
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
fn udp_last_error() -> Text {
    net::udp_error()
}
#[inline(never)]
fn handle_tcp_result_operation(
    response: &mut [u8],
    channel: u32,
    op: u16,
    id: u32,
    payload: &[u8],
    client: u16,
) -> i32 {
    let mut data_buf = [0; TCP_RESULT_DATA_MAX];
    let result = make_tcp_result(op, payload, &mut data_buf, client);
    let mut payload_buf = [0; TCP_MESSAGE_PAYLOAD_MAX];
    let base = result.result.encode(&mut payload_buf);
    payload_buf[base..base + result.data.len()].copy_from_slice(result.data);
    write_response(
        response,
        channel,
        op,
        id,
        RESULT_OK,
        &payload_buf[..base + result.data.len()],
    )
}
#[inline(never)]
fn handle_udp_result_operation(
    response: &mut [u8],
    channel: u32,
    op: u16,
    id: u32,
    payload: &[u8],
) -> i32 {
    let mut data_buf = [0; UDP_RESULT_DATA_MAX];
    let result = make_udp_result(op, payload, &mut data_buf);
    let mut payload_buf = [0; TCP_MESSAGE_PAYLOAD_MAX];
    let base = result.result.encode(&mut payload_buf);
    payload_buf[base..base + result.data.len()].copy_from_slice(result.data);
    write_response(
        response,
        channel,
        op,
        id,
        RESULT_OK,
        &payload_buf[..base + result.data.len()],
    )
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
fn write_dhcp_status(w: &mut Writer<'_>) {
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    w.text(b"state=".as_slice());
    w.text(if s.lease.bound {
        b"bound".as_slice()
    } else {
        b"released".as_slice()
    });
    w.text(b" pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" op=".as_slice());
    w.text(s.pending_label);
    w.text(b" ip=".as_slice());
    w.ip(s.lease.offered_ip);
    w.text(b" server=".as_slice());
    w.ip(s.lease.server_ip);
    w.text(b" lease=".as_slice());
    w.num(s.lease.lease_seconds);
    w.text(b" elapsed=".as_slice());
    w.num(timing.elapsed_seconds);
    w.text(b" remaining=".as_slice());
    w.num(timing.remaining_seconds);
    w.text(b" renew_in=".as_slice());
    w.num(timing.renew_in_seconds);
    w.text(b" rebind_in=".as_slice());
    w.num(timing.rebind_in_seconds);
    w.text(b" discover=".as_slice());
    w.num(s.discover_tx);
    w.text(b" offer=".as_slice());
    w.num(s.offer_rx);
    w.text(b" request=".as_slice());
    w.num(s.request_tx);
    w.text(b" ack=".as_slice());
    w.num(s.ack_rx);
    w.text(b" nak=".as_slice());
    w.num(s.nak_rx);
    w.text(b" release=".as_slice());
    w.num(s.release_tx);
    w.text(b" retry=".as_slice());
    w.num(s.retries);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" relerr=".as_slice());
    w.num(s.release_errors);
    w.text(b" malformed=".as_slice());
    w.num(s.malformed);
    w.text(b" inactive=".as_slice());
    w.num(s.inactive_rx);
    w.text(b" foreign=".as_slice());
    w.num(s.foreign_rx);
    w.text(b" phase=".as_slice());
    w.num(s.out_of_phase_rx);
    w.text(b" attempt=".as_slice());
    w.num(s.last_attempt);
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}
fn make_dhcp_status_result() -> DhcpServiceStatus {
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    let runtime = net::dhcp_runtime_status();
    let mut flags: u32 = 0;
    if s.lease.bound {
        flags |= DHCP_FLAG_BOUND;
    }
    if s.operation_pending {
        flags |= DHCP_FLAG_PENDING;
    }
    if s.lease.dns_configured {
        flags |= DHCP_FLAG_DNS_CONFIGURED;
    }
    if runtime.desired_dhcp {
        flags |= DHCP_FLAG_DESIRED;
    }
    if runtime.task_started {
        flags |= DHCP_FLAG_TASK_STARTED;
    }
    if runtime.link_up {
        flags |= DHCP_FLAG_LINK_UP;
    }
    if runtime.state == super::dhcp_runtime::State::RetryWait {
        flags |= DHCP_FLAG_RETRY_WAIT;
    }
    let mut out = DhcpServiceStatus {
        runtime_state: runtime.state as u16,
        flags: int(flags),
        xid: int(s.lease.xid),
        offered_ip: s.lease.offered_ip,
        server_ip: s.lease.server_ip,
        netmask: s.lease.netmask,
        gateway_ip: s.lease.gateway_ip,
        dns_ip: s.lease.dns_ip,
        lease_seconds: int(s.lease.lease_seconds),
        renew_seconds: int(s.lease.renew_seconds),
        rebind_seconds: int(s.lease.rebind_seconds),
        elapsed_seconds: int(timing.elapsed_seconds),
        remaining_seconds: int(timing.remaining_seconds),
        renew_in_seconds: int(timing.renew_in_seconds),
        rebind_in_seconds: int(timing.rebind_in_seconds),
        last_attempt: int(s.last_attempt),
        last_type: int(s.last_type),
        discover_tx: int(s.discover_tx),
        offer_rx: int(s.offer_rx),
        request_tx: int(s.request_tx),
        ack_rx: int(s.ack_rx),
        nak_rx: int(s.nak_rx),
        release_tx: int(s.release_tx),
        retries: int(s.retries),
        timeouts: int(s.timeouts),
        release_errors: int(s.release_errors),
        malformed: int(s.malformed),
        self_tests: int(s.self_tests),
        ..DhcpServiceStatus::EMPTY
    };
    copy_bounded(&mut out.pending_label[0..], s.pending_label);
    copy_bounded(&mut out.last_error[0..], s.last_error);
    out
}
fn write_dhcp_operation(w: &mut Writer<'_>, op: u16, payload: &[u8]) {
    let request = split_service_deadline(payload);
    let result = match op {
        OP_DHCP_ACQUIRE => net::dhcp_acquire_default_until(request.deadline_tick),
        OP_DHCP_RENEW => net::dhcp_renew_default_until(request.deadline_tick),
        OP_DHCP_RELEASE => net::dhcp_release_default(),
        _ => unreachable!(),
    };
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    w.text(b"action=".as_slice());
    w.text(match op {
        OP_DHCP_ACQUIRE => b"acquire".as_slice(),
        OP_DHCP_RENEW => b"renew".as_slice(),
        OP_DHCP_RELEASE => b"release".as_slice(),
        _ => b"unknown".as_slice(),
    });
    w.text(b" result=".as_slice());
    w.text(net::tx_result_name(result));
    w.text(b" code=".as_slice());
    w.signed(net_tx_result_code(result));
    w.text(b" state=".as_slice());
    w.text(if s.lease.bound {
        b"bound".as_slice()
    } else {
        b"released".as_slice()
    });
    w.text(b" pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" ip=".as_slice());
    w.ip(s.lease.offered_ip);
    w.text(b" server=".as_slice());
    w.ip(s.lease.server_ip);
    w.text(b" remaining=".as_slice());
    w.num(timing.remaining_seconds);
    w.text(b" renew_in=".as_slice());
    w.num(timing.renew_in_seconds);
    w.text(b" rebind_in=".as_slice());
    w.num(timing.rebind_in_seconds);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" retry=".as_slice());
    w.num(s.retries);
    w.text(b" relerr=".as_slice());
    w.num(s.release_errors);
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}
fn make_dhcp_operation_result(op: u16, payload: &[u8]) -> DhcpServiceResult {
    let request = split_service_deadline(payload);
    let tx = match op {
        OP_DHCP_ACQUIRE_RESULT => net::dhcp_acquire_default_until(request.deadline_tick),
        OP_DHCP_RENEW_RESULT => net::dhcp_renew_default_until(request.deadline_tick),
        OP_DHCP_RELEASE_RESULT => net::dhcp_release_default(),
        _ => unreachable!(),
    };
    let s = net::dhcp_stats();
    let timing = net::dhcp_lease_timing();
    let runtime = net::dhcp_runtime_status();
    let mut flags: u32 = 0;
    if s.lease.bound {
        flags |= DHCP_FLAG_BOUND;
    }
    if s.operation_pending {
        flags |= DHCP_FLAG_PENDING;
    }
    if s.lease.dns_configured {
        flags |= DHCP_FLAG_DNS_CONFIGURED;
    }
    if runtime.desired_dhcp {
        flags |= DHCP_FLAG_DESIRED;
    }
    if runtime.task_started {
        flags |= DHCP_FLAG_TASK_STARTED;
    }
    if runtime.link_up {
        flags |= DHCP_FLAG_LINK_UP;
    }
    if runtime.state == super::dhcp_runtime::State::RetryWait {
        flags |= DHCP_FLAG_RETRY_WAIT;
    }
    flags = with_service_status(flags, service_status_from_tx_result(tx, s.last_error));
    DhcpServiceResult {
        action: int(match op {
            OP_DHCP_ACQUIRE_RESULT => DHCP_ACTION_ACQUIRE,
            OP_DHCP_RENEW_RESULT => DHCP_ACTION_RENEW,
            OP_DHCP_RELEASE_RESULT => DHCP_ACTION_RELEASE,
            _ => 0,
        }),
        result: int(net_tx_result_code(tx)),
        flags: int(flags),
        offered_ip: s.lease.offered_ip,
        server_ip: s.lease.server_ip,
        netmask: s.lease.netmask,
        gateway_ip: s.lease.gateway_ip,
        dns_ip: s.lease.dns_ip,
        lease_seconds: int(s.lease.lease_seconds),
        elapsed_seconds: int(timing.elapsed_seconds),
        remaining_seconds: int(timing.remaining_seconds),
        renew_in_seconds: int(timing.renew_in_seconds),
        rebind_in_seconds: int(timing.rebind_in_seconds),
        last_attempt: int(s.last_attempt),
        last_type: int(s.last_type),
        discover_tx: int(s.discover_tx),
        offer_rx: int(s.offer_rx),
        request_tx: int(s.request_tx),
        ack_rx: int(s.ack_rx),
        nak_rx: int(s.nak_rx),
        release_tx: int(s.release_tx),
        retries: int(s.retries),
        timeouts: int(s.timeouts),
        release_errors: int(s.release_errors),
        malformed: int(s.malformed),
        ..DhcpServiceResult::EMPTY
    }
}
fn write_dns_status(w: &mut Writer<'_>) {
    let s = net::dns_stats();
    let cache = net::dns_cache_status();
    w.text(b"pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" name=".as_slice());
    w.text(if s.operation_pending {
        &s.pending_name[0..]
    } else {
        &s.last_name[0..]
    });
    w.text(b" queries=".as_slice());
    w.num(s.queries_tx);
    w.text(b" resolve=".as_slice());
    w.num(s.resolve_requests);
    w.text(b" responses=".as_slice());
    w.num(s.responses_rx);
    w.text(b" a=".as_slice());
    w.num(s.a_records);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" nxdomain=".as_slice());
    w.num(s.nxdomain);
    w.text(b" txerr=".as_slice());
    w.num(s.tx_errors);
    w.text(b" malformed=".as_slice());
    w.num(s.malformed);
    w.text(b" server=".as_slice());
    w.ip(s.last_server);
    w.text(b" id=".as_slice());
    w.num(s.last_id);
    w.text(b" answer=".as_slice());
    w.ip(s.last_answer);
    w.text(b" result=".as_slice());
    w.signed(s.last_result);
    w.text(b" cache=".as_slice());
    w.text(if cache.valid {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" cache_name=".as_slice());
    w.text(&s.cache_name[0..]);
    w.text(b" cache_answer=".as_slice());
    w.ip(s.cache_answer);
    w.text(b" cache_hits=".as_slice());
    w.num(s.cache_hits);
    w.text(b" cache_stores=".as_slice());
    w.num(s.cache_stores);
    w.text(b" cache_age=".as_slice());
    w.num(cache.age_seconds);
    w.text(b"/".as_slice());
    w.num(cache.ttl_seconds);
    w.text(b" cache_remaining=".as_slice());
    w.num(cache.remaining_seconds);
    w.text(b" cache_entries=".as_slice());
    w.num(net::dns_cache_entry_count());
    w.text(b" cache_negative=".as_slice());
    w.num(net::dns_cache_negative_count());
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}
fn make_dns_status_result() -> DnsServiceStatus {
    let s = net::dns_stats();
    let cache = net::dns_cache_status();
    let mut flags: u32 = 0;
    if s.last_result == 0 && (s.responses_rx != 0 || s.a_records != 0) {
        flags |= DNS_FLAG_OK;
    }
    if s.operation_pending {
        flags |= DNS_FLAG_PENDING;
    }
    if cache.valid {
        flags |= DNS_FLAG_CACHE_VALID;
    }
    let mut out = DnsServiceStatus {
        flags: int(flags),
        last_result: int(s.last_result),
        last_id: int(s.last_id),
        last_server: s.last_server,
        last_answer: s.last_answer,
        cache_server: s.cache_server,
        cache_answer: s.cache_answer,
        cache_age_seconds: int(cache.age_seconds),
        cache_ttl_seconds: int(cache.ttl_seconds),
        cache_remaining_seconds: int(cache.remaining_seconds),
        queries_tx: int(s.queries_tx),
        resolve_requests: int(s.resolve_requests),
        responses_rx: int(s.responses_rx),
        a_records: int(s.a_records),
        timeouts: int(s.timeouts),
        nxdomain: int(s.nxdomain),
        tx_errors: int(s.tx_errors),
        malformed: int(s.malformed),
        self_tests: int(s.self_tests),
        cache_hits: int(s.cache_hits),
        cache_stores: int(s.cache_stores),
        ..DnsServiceStatus::EMPTY
    };
    copy_bounded(&mut out.name[0..], &s.last_name[0..]);
    out.name_len = int(string_len_z(&out.name[0..]));
    copy_bounded(&mut out.pending_name[0..], &s.pending_name[0..]);
    out.pending_name_len = int(string_len_z(&out.pending_name[0..]));
    copy_bounded(&mut out.cache_name[0..], &s.cache_name[0..]);
    out.cache_name_len = int(string_len_z(&out.cache_name[0..]));
    copy_bounded(&mut out.last_error[0..], s.last_error);
    out
}
fn write_dns_resolve(w: &mut Writer<'_>, op: u16, payload: &[u8]) {
    let mut server: [u8; 4] = [0; 4];
    let mut name = payload;
    let explicit_server = op == OP_DNS_RESOLVE_A_SERVER;
    if explicit_server {
        if payload.len() < 5 {
            w.text(b"name= result=name code=-9".as_slice());
            w.text(b" ip=0.0.0.0 server=0.0.0.0 pending=no cache=no cache_hits=0 cache_stores=0 timeout=0 last=bad-request".as_slice());
            return;
        }
        server = [payload[0], payload[1], payload[2], payload[3]];
        name = &payload[4..];
    }
    let mut answer: [u8; 4] = [0; 4];
    let result = if explicit_server {
        net::dns_resolve_a_with_server(name, server, &mut answer)
    } else {
        net::dns_resolve_a(name, &mut answer)
    };
    w.text(b"name=".as_slice());
    w.text(name);
    w.text(b" result=".as_slice());
    w.text(net::dns_result_name(result));
    w.text(b" code=".as_slice());
    w.signed(result);
    w.text(b" ip=".as_slice());
    w.ip(answer);
    let s = net::dns_stats();
    let cache = net::dns_cache_status();
    w.text(b" server=".as_slice());
    w.ip(if explicit_server {
        server
    } else {
        s.last_server
    });
    w.text(b" pending=".as_slice());
    w.text(if s.operation_pending {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" cache=".as_slice());
    w.text(if cache.valid {
        b"yes".as_slice()
    } else {
        b"no".as_slice()
    });
    w.text(b" cache_hits=".as_slice());
    w.num(s.cache_hits);
    w.text(b" cache_stores=".as_slice());
    w.num(s.cache_stores);
    w.text(b" timeout=".as_slice());
    w.num(s.timeouts);
    w.text(b" cache_entries=".as_slice());
    w.num(net::dns_cache_entry_count());
    w.text(b" cache_negative=".as_slice());
    w.num(net::dns_cache_negative_count());
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}
fn make_dns_resolve_result(op: u16, payload: &[u8]) -> DnsServiceResult {
    let mut server: [u8; 4] = [0; 4];
    let mut name = payload;
    let explicit_server = op == OP_DNS_RESOLVE_A_SERVER_RESULT;
    if explicit_server {
        if payload.len() < 5 {
            return dns_bad_request_result(op, b"".as_slice());
        }
        server = [payload[0], payload[1], payload[2], payload[3]];
        name = &payload[4..];
    }
    let mut answer: [u8; 4] = [0; 4];
    let before = net::dns_stats();
    let result = if explicit_server {
        net::dns_resolve_a_with_server(name, server, &mut answer)
    } else {
        net::dns_resolve_a(name, &mut answer)
    };
    let after = net::dns_stats();
    let cache = net::dns_cache_status();
    let mut flags: u32 = 0;
    if result == 0 {
        flags |= DNS_FLAG_OK;
    }
    if after.operation_pending {
        flags |= DNS_FLAG_PENDING;
    }
    if cache.valid {
        flags |= DNS_FLAG_CACHE_VALID;
    }
    if after.cache_hits > before.cache_hits {
        flags |= DNS_FLAG_CACHE_HIT;
    }
    if explicit_server {
        flags |= DNS_FLAG_EXPLICIT_SERVER;
    }
    flags = with_service_status(flags, service_status_from_dns_result(result));
    let mut out = DnsServiceResult {
        action: int(if explicit_server {
            DNS_ACTION_RESOLVE_A_SERVER
        } else {
            DNS_ACTION_RESOLVE_A
        }),
        result: int(result),
        flags: int(flags),
        answer,
        server: if explicit_server {
            server
        } else {
            after.last_server
        },
        cache_answer: after.cache_answer,
        cache_age_seconds: int(cache.age_seconds),
        cache_ttl_seconds: int(cache.ttl_seconds),
        cache_remaining_seconds: int(cache.remaining_seconds),
        queries_tx: int(after.queries_tx),
        resolve_requests: int(after.resolve_requests),
        responses_rx: int(after.responses_rx),
        a_records: int(after.a_records),
        timeouts: int(after.timeouts),
        nxdomain: int(after.nxdomain),
        tx_errors: int(after.tx_errors),
        malformed: int(after.malformed),
        cache_hits: int(after.cache_hits),
        cache_stores: int(after.cache_stores),
        last_id: int(after.last_id),
        ..DnsServiceResult::EMPTY
    };
    copy_bounded(&mut out.name[0..], name);
    out.name_len = int(core::cmp::min(name.len(), out.name.len()));
    copy_bounded(&mut out.last_error[0..], after.last_error);
    out
}
fn dns_bad_request_result(op: u16, name: &[u8]) -> DnsServiceResult {
    let mut out = DnsServiceResult {
        action: int(if op == OP_DNS_RESOLVE_A_SERVER_RESULT {
            DNS_ACTION_RESOLVE_A_SERVER
        } else {
            DNS_ACTION_RESOLVE_A
        }),
        result: int(-9),
        flags: int(with_service_status(
            if op == OP_DNS_RESOLVE_A_SERVER_RESULT {
                DNS_FLAG_EXPLICIT_SERVER
            } else {
                0
            },
            Status::Failed,
        )),
        ..DnsServiceResult::EMPTY
    };
    copy_bounded(&mut out.name[0..], name);
    out.name_len = int(core::cmp::min(name.len(), out.name.len()));
    copy_bounded(&mut out.last_error[0..], b"bad-request".as_slice());
    out
}
fn write_udp_status(w: &mut Writer<'_>) {
    let s = net::udp_status();
    w.text(b"active=".as_slice());
    w.num(s.active_sockets);
    w.text(b"/".as_slice());
    w.num(s.max_sockets);
    w.text(b" queue=".as_slice());
    w.num(s.queued_packets);
    w.text(b"/".as_slice());
    w.num(s.queue_limit);
    w.text(b" payload_max=".as_slice());
    w.num(s.payload_max);
    w.text(b" message_payload=".as_slice());
    w.num(TCP_MESSAGE_PAYLOAD_MAX);
    w.text(b" send_max=".as_slice());
    w.num(UDP_SEND_PAYLOAD_MAX);
    w.text(b" recv_max=".as_slice());
    w.num(UDP_RESULT_DATA_MAX);
    w.text(b" delivered=".as_slice());
    w.num(s.delivered);
    w.text(b" drops=".as_slice());
    w.num(s.drops);
    w.text(b" lifecycle_last=".as_slice());
    w.text(socket_lifecycle_name(
        state::snapshot().udp_lifecycle.last_cause,
    ));
    w.text(b" would_block=".as_slice());
    w.num(state::snapshot().udp_lifecycle.would_block);
    w.text(b" last=".as_slice());
    w.text(s.last_error);
}
fn make_udp_status_result() -> UdpServiceStatus {
    let s = net::udp_status();
    let mut out = UdpServiceStatus {
        active_sockets: int(s.active_sockets),
        max_sockets: int(s.max_sockets),
        queued_packets: int(s.queued_packets),
        queue_limit: int(s.queue_limit),
        payload_max: int(s.payload_max),
        message_payload_max: int(TCP_MESSAGE_PAYLOAD_MAX),
        send_max: int(UDP_SEND_PAYLOAD_MAX),
        recv_max: int(UDP_RESULT_DATA_MAX),
        delivered: int(s.delivered),
        drops: int(s.drops),
        last_lifecycle_cause: int(state::snapshot().udp_lifecycle.last_cause),
        lifecycle_closed: int(state::snapshot().udp_lifecycle.closed),
        lifecycle_timeout: int(state::snapshot().udp_lifecycle.timeout),
        lifecycle_local_close: int(state::snapshot().udp_lifecycle.local_close),
        lifecycle_would_block: int(state::snapshot().udp_lifecycle.would_block),
        lifecycle_bad_handle: int(state::snapshot().udp_lifecycle.bad_handle),
        lifecycle_dropped: int(state::snapshot().udp_lifecycle.dropped),
        ..UdpServiceStatus::EMPTY
    };
    if state::snapshot().udp_lifecycle.last_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
        out.flags |= UDP_STATUS_FLAG_LIFECYCLE_VALID;
    }
    copy_bounded(&mut out.last_error[0..], s.last_error);
    out
}
fn write_udp_operation(w: &mut Writer<'_>, op: u16, payload: &[u8]) {
    match op {
        OP_UDP_BIND => write_udp_bind(w, payload),
        OP_UDP_SENDTO => write_udp_send_to(w, payload),
        OP_UDP_RECV => write_udp_recv(w, payload),
        OP_UDP_CLOSE => write_udp_close(w, payload),
        _ => unreachable!(),
    };
}
fn is_udp_operation(op: u16) -> bool {
    op == OP_UDP_BIND || op == OP_UDP_SENDTO || op == OP_UDP_RECV || op == OP_UDP_CLOSE
}
fn is_udp_result_operation(op: u16) -> bool {
    op == OP_UDP_BIND_RESULT
        || op == OP_UDP_SENDTO_RESULT
        || op == OP_UDP_RECV_RESULT
        || op == OP_UDP_CLOSE_RESULT
}
fn make_udp_result<'a>(op: u16, payload: &[u8], data_out: &'a mut [u8]) -> UdpResultPayload<'a> {
    let mut out = UdpServiceResult {
        action: int(udp_result_action(op)),
        ..UdpServiceResult::EMPTY
    };
    let data = match op {
        OP_UDP_BIND_RESULT => udp_bind_result(&mut out, payload),
        OP_UDP_SENDTO_RESULT => udp_send_to_result(&mut out, payload),
        OP_UDP_RECV_RESULT => udp_recv_result(&mut out, payload, data_out),
        OP_UDP_CLOSE_RESULT => udp_close_result(&mut out, payload),
        _ => b"".as_slice(),
    };
    fill_udp_result_status(&mut out);
    set_udp_operation_status(&mut out);
    UdpResultPayload { result: out, data }
}
fn udp_result_action(op: u16) -> u16 {
    match op {
        OP_UDP_BIND_RESULT => UDP_ACTION_BIND,
        OP_UDP_SENDTO_RESULT => UDP_ACTION_SENDTO,
        OP_UDP_RECV_RESULT => UDP_ACTION_RECV,
        OP_UDP_CLOSE_RESULT => UDP_ACTION_CLOSE,
        _ => 0,
    }
}
fn udp_bind_result(out: &mut UdpServiceResult, payload: &[u8]) -> &'static [u8] {
    if payload.len() < 2 {
        return udp_bad_request_result(out);
    }
    out.dest_port = read_u16(payload, 0);
    if out.dest_port == 0 {
        return udp_bad_request_result(out);
    }
    let svc_handle = net::udp_bind(out.dest_port);
    out.result = svc_handle;
    if svc_handle > 0 {
        out.result = 0;
        out.handle = int(svc_handle);
        out.flags |= UDP_FLAG_OK | UDP_FLAG_HANDLE_VALID;
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    }
    b"".as_slice()
}
fn udp_send_to_result(out: &mut UdpServiceResult, payload: &[u8]) -> &'static [u8] {
    if payload.len() < 10 {
        return udp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    out.dest_ip = [payload[4], payload[5], payload[6], payload[7]];
    out.dest_port = read_u16(payload, 8);
    out.requested_bytes = int(payload.len() - 10);
    out.flags |= UDP_FLAG_HANDLE_VALID | UDP_FLAG_REMOTE_VALID;
    let tx = net::udp_send_to(out.handle, out.dest_ip, out.dest_port, &payload[10..]);
    out.result = net_tx_result_code(tx);
    if tx == net::TxResult::Ok {
        out.flags |= UDP_FLAG_OK;
        out.bytes = out.requested_bytes;
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    } else if tx == net::TxResult::Busy {
        set_udp_service_status(out, Status::WouldBlock);
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
    } else {
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
    }
    b"".as_slice()
}
fn udp_recv_result<'a>(
    out: &mut UdpServiceResult,
    payload: &[u8],
    data_out: &'a mut [u8],
) -> &'a [u8] {
    if payload.len() < 6 {
        return udp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    let max_len = core::cmp::min(read_u16(payload, 4) as usize, data_out.len());
    if max_len == 0 {
        return udp_bad_request_result(out);
    }
    out.requested_bytes = int(max_len);
    out.flags |= UDP_FLAG_HANDLE_VALID;
    let mut info: super::udp_sockets::Info =
        unsafe { core::mem::zeroed::<super::udp_sockets::Info>() };
    let got = net::udp_recv_from(out.handle, &mut info, &mut data_out[0..max_len]);
    out.result = got;
    if got >= 0 {
        out.result = 0;
        out.bytes = int(got);
        out.source_ip = info.source_ip;
        out.dest_ip = info.dest_ip;
        out.source_port = info.source_port;
        out.dest_port = info.dest_port;
        out.flags |= UDP_FLAG_OK;
        if got > 0 {
            out.flags |= UDP_FLAG_DATA | UDP_FLAG_REMOTE_VALID;
            set_udp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
            return &data_out
                [0..usize::try_from(got).expect("Original bounded positive IPC length")];
        }
        set_udp_service_status(out, Status::WouldBlock);
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
    }
    b"".as_slice()
}
fn udp_close_result(out: &mut UdpServiceResult, payload: &[u8]) -> &'static [u8] {
    if payload.len() < 4 {
        return udp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    out.flags |= UDP_FLAG_HANDLE_VALID;
    let result = net::udp_close(out.handle);
    out.result = result;
    if result == 0 {
        out.flags |= UDP_FLAG_OK;
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC);
        record_service_close(Status::Ok);
    } else {
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
        record_service_close(Status::Failed);
    }
    b"".as_slice()
}
fn udp_bad_request_result(out: &mut UdpServiceResult) -> &'static [u8] {
    out.result = -1;
    set_udp_service_status(out, Status::Failed);
    copy_bounded(&mut out.last_error[0..], b"bad-request".as_slice());
    b"".as_slice()
}
fn fill_udp_result_status(out: &mut UdpServiceResult) {
    let s = net::udp_status();
    out.active_sockets = s.active_sockets;
    out.max_sockets = s.max_sockets;
    out.queued_packets = s.queued_packets;
    out.queue_limit = s.queue_limit;
    out.payload_max = s.payload_max;
    out.message_payload_max = int(TCP_MESSAGE_PAYLOAD_MAX);
    out.send_max = int(UDP_SEND_PAYLOAD_MAX);
    out.recv_max = int(UDP_RESULT_DATA_MAX);
    out.delivered = s.delivered;
    out.drops = s.drops;
    if out.last_error[0] == 0 {
        copy_bounded(&mut out.last_error[0..], s.last_error);
    }
}
fn set_udp_operation_status(out: &mut UdpServiceResult) {
    if service_status_encoded(out.flags) {
        out.service_status = service_operation_status_code(out.flags);
        return;
    }
    let last_error = fixed_text(&out.last_error[0..]);
    if out.result == 0 {
        if text_equals(last_error, operation_status_name(Status::WouldBlock)) {
            set_udp_service_status(out, Status::WouldBlock);
            set_udp_lifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        } else {
            set_udp_service_status(out, Status::Ok);
        }
        return;
    }
    if text_contains(last_error, b"timeout".as_slice()) {
        set_udp_service_status(out, Status::Timeout);
        set_udp_lifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
    } else {
        set_udp_service_status(out, Status::Failed);
        if out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
            set_udp_lifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
        }
    }
}
fn write_udp_bind(w: &mut Writer<'_>, payload: &[u8]) {
    if payload.len() < 2 {
        w.text(b"op=bind result=bad-request code=-1".as_slice());
        return;
    }
    let port = read_u16(payload, 0);
    let svc_handle = net::udp_bind(port);
    w.text(b"op=bind result=".as_slice());
    if svc_handle > 0 {
        w.text(b"ok code=0 handle=".as_slice());
        w.num(u32::try_from(svc_handle).expect("Original bounded positive IPC length"));
    } else {
        w.text(b"failed code=".as_slice());
        w.signed(svc_handle);
        w.text(b" handle=0".as_slice());
    }
    w.text(b" port=".as_slice());
    w.num(port);
    w.text(b" last=".as_slice());
    w.text(udp_last_error());
}
fn write_udp_send_to(w: &mut Writer<'_>, payload: &[u8]) {
    if payload.len() < 10 {
        w.text(b"op=sendto result=bad-request code=-1".as_slice());
        return;
    }
    let svc_handle = read_u32(payload, 0);
    let dest_ip: [u8; 4] = [payload[4], payload[5], payload[6], payload[7]];
    let dest_port = read_u16(payload, 8);
    let data = &payload[10..];
    let tx = net::udp_send_to(svc_handle, dest_ip, dest_port, data);
    w.text(b"op=sendto result=".as_slice());
    w.text(net::tx_result_name(tx));
    w.text(b" code=".as_slice());
    w.signed(net_tx_result_code(tx));
    w.text(b" handle=".as_slice());
    w.num(svc_handle);
    w.text(b" remote=".as_slice());
    w.ip(dest_ip);
    w.text(b":".as_slice());
    w.num(dest_port);
    w.text(b" bytes=".as_slice());
    w.num(data.len());
    w.text(b" last=".as_slice());
    w.text(udp_last_error());
}
fn write_udp_recv(w: &mut Writer<'_>, payload: &[u8]) {
    if payload.len() < 6 {
        w.text(b"op=recv result=bad-request code=-1".as_slice());
        return;
    }
    let svc_handle = read_u32(payload, 0);
    let max_len = core::cmp::min(read_u16(payload, 4) as usize, UDP_RESULT_DATA_MAX);
    if max_len == 0 {
        w.text(b"op=recv result=bad-request code=-1".as_slice());
        return;
    }
    let mut buf: [u8; UDP_RESULT_DATA_MAX] = [0; UDP_RESULT_DATA_MAX];
    let mut info: super::udp_sockets::Info =
        unsafe { core::mem::zeroed::<super::udp_sockets::Info>() };
    let got = net::udp_recv_from(svc_handle, &mut info, &mut buf[0..max_len]);
    w.text(b"op=recv result=".as_slice());
    if got >= 0 {
        w.text(b"ok code=0 bytes=".as_slice());
        w.num(u32::try_from(got).expect("Original bounded positive IPC length"));
    } else {
        w.text(b"failed code=".as_slice());
        w.signed(got);
        w.text(b" bytes=0".as_slice());
    }
    w.text(b" handle=".as_slice());
    w.num(svc_handle);
    if got > 0 {
        w.text(b" source=".as_slice());
        w.ip(info.source_ip);
        w.text(b":".as_slice());
        w.num(info.source_port);
        w.text(b" dest=".as_slice());
        w.ip(info.dest_ip);
        w.text(b":".as_slice());
        w.num(info.dest_port);
        w.text(b" data=".as_slice());
        w.text(&buf[0..usize::try_from(got).expect("Original bounded positive IPC length")]);
    }
    w.text(b" last=".as_slice());
    w.text(udp_last_error());
}
fn write_udp_close(w: &mut Writer<'_>, payload: &[u8]) {
    if payload.len() < 4 {
        w.text(b"op=close result=bad-request code=-1".as_slice());
        record_service_close(Status::Failed);
        return;
    }
    let svc_handle = read_u32(payload, 0);
    let result = net::udp_close(svc_handle);
    let status: Status = if result == 0 {
        Status::Ok
    } else {
        Status::Failed
    };
    record_service_close(status);
    w.text(b"op=close result=".as_slice());
    w.text(if result == 0 {
        b"ok".as_slice()
    } else {
        b"failed".as_slice()
    });
    w.text(b" code=".as_slice());
    w.signed(result);
    w.text(b" handle=".as_slice());
    w.num(svc_handle);
    w.text(b" last=".as_slice());
    w.text(udp_last_error());
    w.text(b" status=".as_slice());
    w.text(operation_status_name(status));
}
fn write_tcp_status(w: &mut Writer<'_>) {
    state::reap();
    let mut summary: crate::contract::TcpSummary = crate::contract::TcpSummary::default();
    net::tcp_summary(&mut summary);
    let stats = net::tcp_stats();
    w.text(b"active=".as_slice());
    w.num(summary.active_connections);
    w.text(b"/".as_slice());
    w.num(summary.max_connections);
    w.text(b" handles=".as_slice());
    w.num(state::count());
    w.text(b"/".as_slice());
    w.num(TCP_SERVICE_HANDLES);
    w.text(b" listen=".as_slice());
    w.num(summary.active_listeners);
    w.text(b" tcp_buffer=".as_slice());
    w.num(summary.buffer_size);
    w.text(b" message_payload=".as_slice());
    w.num(TCP_MESSAGE_PAYLOAD_MAX);
    w.text(b" write_max=".as_slice());
    w.num(TCP_WRITE_PAYLOAD_MAX);
    w.text(b" tx=".as_slice());
    w.num(summary.data_tx);
    w.text(b" rx=".as_slice());
    w.num(summary.data_rx);
    w.text(b" retrans=".as_slice());
    w.num(summary.retransmits);
    w.text(b" drops=".as_slice());
    w.num(summary.rx_drops);
    w.text(b" stale_handles=".as_slice());
    w.num(state::snapshot().stale_handles_reaped);
    w.text(b" tombstones=".as_slice());
    w.num(state::tomb_count());
    w.text(b" lifecycle_last=".as_slice());
    w.text(socket_lifecycle_name(
        state::snapshot().tcp_lifecycle.last_cause,
    ));
    w.text(b" read_timeout=".as_slice());
    w.num(state::snapshot().read_wait_timeouts);
    w.text(b" accept_timeout=".as_slice());
    w.num(state::snapshot().accept_wait_timeouts);
    w.text(b" last=".as_slice());
    w.text(stats.last_error);
}
fn make_tcp_status_result(client_id: u16) -> TcpServiceStatus {
    state::reap();
    let mut summary: crate::contract::TcpSummary = crate::contract::TcpSummary::default();
    net::tcp_summary(&mut summary);
    let stats = net::tcp_stats();
    let mut out = TcpServiceStatus {
        request_owner_id: int(client_id),
        owned_handles: int(state::owned_count(client_id)),
        legacy_handles: int(state::legacy_count()),
        active_connections: int(summary.active_connections),
        max_connections: int(summary.max_connections),
        active_listeners: int(summary.active_listeners),
        handle_count: int(state::count()),
        max_handles: int(TCP_SERVICE_HANDLES),
        tcp_buffer_size: int(summary.buffer_size),
        message_payload_max: int(TCP_MESSAGE_PAYLOAD_MAX),
        write_max: int(TCP_WRITE_PAYLOAD_MAX),
        read_max: int(TCP_RESULT_DATA_MAX),
        rx_segments: int(stats.rx_segments),
        tx_segments: int(stats.tx_segments),
        syn_tx: int(summary.syn_tx),
        synack_rx: int(summary.synack_rx),
        ack_tx: int(summary.ack_tx),
        data_tx: int(summary.data_tx),
        data_rx: int(summary.data_rx),
        fin_tx: int(summary.fin_tx),
        rst_rx: int(summary.rst_rx),
        checksum_errors: int(summary.checksum_errors),
        timeouts: int(summary.timeouts),
        self_tests: int(summary.self_tests),
        synack_tx: int(summary.synack_tx),
        listen_syn_rx: int(summary.listen_syn_rx),
        accepts: int(summary.accepts),
        retransmits: int(summary.retransmits),
        rx_drops: int(summary.rx_drops),
        last_source_port: int(stats.last_source_port),
        last_dest_port: int(stats.last_dest_port),
        last_flags: int(stats.last_flags),
        last_seq: int(stats.last_seq),
        last_ack: int(stats.last_ack),
        last_payload_len: int(stats.last_payload_len),
        owner_mismatches: int(state::snapshot().owner_mismatches),
        stale_handles_reaped: int(state::snapshot().stale_handles_reaped),
        stale_tombstones: int(state::tomb_count()),
        last_lifecycle_cause: int(state::snapshot().tcp_lifecycle.last_cause),
        lifecycle_closed: int(state::snapshot().tcp_lifecycle.closed),
        lifecycle_reset: int(state::snapshot().tcp_lifecycle.reset),
        lifecycle_timeout: int(state::snapshot().tcp_lifecycle.timeout),
        lifecycle_peer_gone: int(state::snapshot().tcp_lifecycle.peer_gone),
        lifecycle_local_abort: int(state::snapshot().tcp_lifecycle.local_abort),
        lifecycle_local_close: int(state::snapshot().tcp_lifecycle.local_close),
        lifecycle_pending_close: int(state::snapshot().tcp_lifecycle.pending_close),
        lifecycle_would_block: int(state::snapshot().tcp_lifecycle.would_block),
        lifecycle_bad_handle: int(state::snapshot().tcp_lifecycle.bad_handle),
        lifecycle_owner_mismatch: int(state::snapshot().tcp_lifecycle.owner_mismatch),
        read_wait_timeouts: int(state::snapshot().read_wait_timeouts),
        accept_wait_timeouts: int(state::snapshot().accept_wait_timeouts),
        write_would_block: int(state::snapshot().write_would_block),
        close_cancelled: int(state::snapshot().lifecycle.close_cancelled),
        ..TcpServiceStatus::EMPTY
    };
    if summary.active_listeners != 0 {
        out.flags |= TCP_STATUS_FLAG_LISTENER_ACTIVE;
    }
    if stats.last_flags != 0
        || stats.last_source_port != 0
        || stats.last_dest_port != 0
        || stats.last_payload_len != 0
    {
        out.flags |= TCP_STATUS_FLAG_LAST_SEGMENT;
    }
    if state::snapshot().tcp_lifecycle.last_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
        out.flags |= TCP_STATUS_FLAG_LIFECYCLE_VALID;
    }
    copy_bounded(&mut out.last_error[0..], stats.last_error);
    out
}
fn write_tcp_operation(w: &mut Writer<'_>, op: u16, payload: &[u8], client_id: u16) {
    match op {
        OP_TCP_CONNECT => write_tcp_connect(w, payload, client_id),
        OP_TCP_WRITE => write_tcp_write(w, payload, client_id),
        OP_TCP_READ => write_tcp_read(w, payload, client_id),
        OP_TCP_CLOSE => write_tcp_close(w, payload, client_id),
        OP_TCP_LISTEN => write_tcp_listen(w, payload),
        OP_TCP_ACCEPT_READ => write_tcp_accept_read(w, payload, client_id),
        OP_TCP_CLOSE_LISTEN => write_tcp_close_listen(w, payload),
        OP_TCP_CONNECTIONS => write_tcp_connections(w),
        OP_TCP_POLL => write_tcp_poll(w, payload, client_id),
        OP_TCP_ACCEPT => write_tcp_accept(w, payload, client_id),
        _ => unreachable!(),
    };
}
fn is_tcp_operation(op: u16) -> bool {
    op == OP_TCP_CONNECT
        || op == OP_TCP_WRITE
        || op == OP_TCP_READ
        || op == OP_TCP_CLOSE
        || op == OP_TCP_LISTEN
        || op == OP_TCP_ACCEPT_READ
        || op == OP_TCP_CLOSE_LISTEN
        || op == OP_TCP_CONNECTIONS
        || op == OP_TCP_POLL
        || op == OP_TCP_ACCEPT
}
fn is_tcp_result_operation(op: u16) -> bool {
    op == OP_TCP_CONNECT_RESULT
        || op == OP_TCP_WRITE_RESULT
        || op == OP_TCP_READ_RESULT
        || op == OP_TCP_CLOSE_RESULT
        || op == OP_TCP_LISTEN_RESULT
        || op == OP_TCP_ACCEPT_READ_RESULT
        || op == OP_TCP_CLOSE_LISTEN_RESULT
        || op == OP_TCP_POLL_RESULT
        || op == OP_TCP_ACCEPT_RESULT
        || op == OP_TCP_ABORT_RESULT
        || op == OP_TCP_ACCEPT_POLL_RESULT
        || op == OP_TCP_RETRANSMIT_RESULT
}
fn make_tcp_result<'a>(
    op: u16,
    payload: &[u8],
    data_out: &'a mut [u8],
    client_id: u16,
) -> TcpResultPayload<'a> {
    let request = if op == OP_TCP_CONNECT_RESULT {
        split_service_deadline(payload)
    } else {
        DeadlineRequest {
            payload,
            ..DeadlineRequest::EMPTY
        }
    };
    let mut out = TcpServiceResult {
        action: int(tcp_result_action(op)),
        read_max: int(TCP_RESULT_DATA_MAX),
        owner_id: int(client_id),
        ..TcpServiceResult::EMPTY
    };
    let data = match op {
        OP_TCP_CONNECT_RESULT => {
            tcp_connect_result(&mut out, request.payload, request.deadline_tick, client_id)
        }
        OP_TCP_WRITE_RESULT => tcp_write_result(&mut out, request.payload, client_id),
        OP_TCP_READ_RESULT => tcp_read_result(&mut out, request.payload, data_out, client_id),
        OP_TCP_CLOSE_RESULT => tcp_close_result(&mut out, request.payload, client_id),
        OP_TCP_LISTEN_RESULT => tcp_listen_result(&mut out, request.payload),
        OP_TCP_ACCEPT_READ_RESULT => {
            tcp_accept_read_result(&mut out, request.payload, data_out, client_id)
        }
        OP_TCP_CLOSE_LISTEN_RESULT => tcp_close_listen_result(&mut out, request.payload),
        OP_TCP_POLL_RESULT => tcp_poll_result(&mut out, request.payload, client_id),
        OP_TCP_ACCEPT_RESULT => tcp_accept_result(&mut out, request.payload, client_id),
        OP_TCP_ABORT_RESULT => tcp_abort_result(&mut out, request.payload, client_id),
        OP_TCP_ACCEPT_POLL_RESULT => tcp_accept_poll_result(&mut out, request.payload, client_id),
        OP_TCP_RETRANSMIT_RESULT => tcp_retransmit_result(&mut out, request.payload, client_id),
        _ => b"".as_slice(),
    };
    fill_tcp_result_status(&mut out);
    set_tcp_operation_status(&mut out);
    TcpResultPayload { result: out, data }
}
fn tcp_result_action(op: u16) -> u16 {
    match op {
        OP_TCP_CONNECT_RESULT => TCP_ACTION_CONNECT,
        OP_TCP_WRITE_RESULT => TCP_ACTION_WRITE,
        OP_TCP_READ_RESULT => TCP_ACTION_READ,
        OP_TCP_CLOSE_RESULT => TCP_ACTION_CLOSE,
        OP_TCP_LISTEN_RESULT => TCP_ACTION_LISTEN,
        OP_TCP_ACCEPT_READ_RESULT => TCP_ACTION_ACCEPT_READ,
        OP_TCP_CLOSE_LISTEN_RESULT => TCP_ACTION_CLOSE_LISTEN,
        OP_TCP_POLL_RESULT => TCP_ACTION_POLL,
        OP_TCP_ACCEPT_RESULT => TCP_ACTION_ACCEPT,
        OP_TCP_ABORT_RESULT => TCP_ACTION_ABORT,
        OP_TCP_ACCEPT_POLL_RESULT => TCP_ACTION_ACCEPT_POLL,
        OP_TCP_RETRANSMIT_RESULT => TCP_ACTION_RETRANSMIT,
        _ => 0,
    }
}
fn tcp_connect_result(
    out: &mut TcpServiceResult,
    payload: &[u8],
    deadline_tick: Option<u64>,
    client_id: u16,
) -> &'static [u8] {
    if payload.len() < 6 {
        return tcp_bad_request_result(out);
    }
    out.remote_ip = [payload[0], payload[1], payload[2], payload[3]];
    out.port = read_u16(payload, 4);
    out.remote_port = out.port;
    out.flags |= TCP_FLAG_REMOTE_VALID;
    let conn = net::tcp_connect_until(out.remote_ip, out.port, deadline_tick);
    if conn <= 0 {
        out.result = conn;
        set_tcp_lifecycle(
            out,
            state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
        );
        return b"".as_slice();
    }
    out.conn_id = int(conn);
    out.flags |= TCP_FLAG_CONN_VALID;
    let svc_handle = match state::allocate(out.conn_id, client_id) {
        Some(value) => value,
        None => {
            let _ = net::tcp_close(out.conn_id);
            out.result = -2;
            copy_bounded(&mut out.last_error[0..], b"handle-full".as_slice());
            return b"".as_slice();
        }
    };
    out.handle = svc_handle;
    out.result = 0;
    out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID;
    set_tcp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    b"".as_slice()
}
fn tcp_write_result(out: &mut TcpServiceResult, payload: &[u8], client_id: u16) -> &'static [u8] {
    if payload.len() < 4 {
        return tcp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    let lookup = state::resolve(out.handle, client_id);
    if lookup.result != 0 {
        out.result = lookup.result;
        copy_bounded(&mut out.last_error[0..], lookup.last_error);
        set_tcp_lifecycle(out, lookup.lifecycle_cause);
        return b"".as_slice();
    }
    let conn_id = lookup.conn_id;
    out.conn_id = conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    let data = &payload[4..];
    out.requested_bytes = int(data.len());
    let written = net::tcp_write(conn_id, data);
    let total: usize = if written > 0 { int(written) } else { 0 };
    let write_error: i32 = if written < 0 { written } else { 0 };
    if total > 0 || data.is_empty() {
        out.result = 0;
        out.bytes = int(total);
        out.flags |= TCP_FLAG_OK;
        set_tcp_lifecycle(
            out,
            if total == 0 {
                SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC
            } else {
                SOCKET_LIFECYCLE_ACTIVE_PUBLIC
            },
        );
        if total == 0 {
            state::note_write_would_block();
        }
    } else if write_error == 0 {
        out.result = 0;
        out.bytes = 0;
        out.flags |= TCP_FLAG_OK;
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        state::note_write_would_block();
    } else {
        out.result = write_error;
        set_tcp_lifecycle(
            out,
            state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
        );
    }
    b"".as_slice()
}
fn tcp_read_result<'a>(
    out: &mut TcpServiceResult,
    payload: &[u8],
    data_out: &'a mut [u8],
    client_id: u16,
) -> &'a [u8] {
    if payload.len() < 6 {
        return tcp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    let lookup = state::resolve(out.handle, client_id);
    if lookup.result != 0 {
        out.result = lookup.result;
        copy_bounded(&mut out.last_error[0..], lookup.last_error);
        set_tcp_lifecycle(out, lookup.lifecycle_cause);
        return b"".as_slice();
    }
    let conn_id = lookup.conn_id;
    out.conn_id = conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    let max_len = core::cmp::min(read_u16(payload, 4) as usize, data_out.len());
    out.requested_bytes = int(max_len);
    let got = net::tcp_read_available(conn_id, &mut data_out[0..max_len]);
    out.result = got;
    if got >= 0 {
        out.result = 0;
        out.bytes = int(got);
        out.flags |= TCP_FLAG_OK;
        if got > 0 {
            out.flags |= TCP_FLAG_DATA;
            set_tcp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        } else {
            copy_bounded(
                &mut out.last_error[0..],
                operation_status_name(Status::WouldBlock),
            );
            set_tcp_service_status(out, Status::WouldBlock);
            set_tcp_lifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        }
        return &data_out[0..usize::try_from(got).expect("Original bounded positive IPC length")];
    }
    set_tcp_lifecycle(
        out,
        state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
    );
    b"".as_slice()
}
fn tcp_close_result(out: &mut TcpServiceResult, payload: &[u8], client_id: u16) -> &'static [u8] {
    if payload.len() < 4 {
        return tcp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    let lookup = state::resolve_close(out.handle, client_id);
    if lookup.result != 0 {
        out.result = lookup.result;
        copy_bounded(&mut out.last_error[0..], lookup.last_error);
        set_tcp_lifecycle(out, lookup.lifecycle_cause);
        record_service_close(Status::Failed);
        return b"".as_slice();
    }
    let conn_id = lookup.conn_id;
    out.conn_id = conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    let result = net::tcp_close(conn_id);
    out.result = result;
    if result >= 0 {
        state::release(out.handle);
        out.result = 0;
        out.flags |= TCP_FLAG_OK;
        set_tcp_service_status(out, Status::Ok);
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC);
        record_service_close(Status::Ok);
    } else if !state::usable(conn_id) {
        state::release(out.handle);
        out.result = 0;
        out.flags |= TCP_FLAG_OK;
        set_tcp_service_status(out, Status::Cancelled);
        set_tcp_lifecycle(
            out,
            if lookup.lifecycle_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
                lookup.lifecycle_cause
            } else {
                state::cause(conn_id)
            },
        );
        record_service_close(Status::Cancelled);
    } else {
        set_tcp_lifecycle(
            out,
            state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
        );
        record_service_close(Status::Failed);
    }
    b"".as_slice()
}
fn tcp_abort_result(out: &mut TcpServiceResult, payload: &[u8], client_id: u16) -> &'static [u8] {
    if payload.len() < 4 {
        return tcp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    let lookup = state::resolve_close(out.handle, client_id);
    if lookup.result != 0 {
        out.result = lookup.result;
        copy_bounded(&mut out.last_error[0..], lookup.last_error);
        set_tcp_lifecycle(out, lookup.lifecycle_cause);
        record_service_close(Status::Failed);
        return b"".as_slice();
    }
    out.conn_id = lookup.conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    fill_tcp_connection_fields(out);
    let result = net::tcp_abort(out.conn_id);
    state::release(out.handle);
    out.result = result;
    if result == 0 {
        out.flags |= TCP_FLAG_OK;
        set_tcp_service_status(out, Status::Cancelled);
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC);
        record_service_close(Status::Cancelled);
    } else if !state::usable(out.conn_id) {
        out.result = 0;
        out.flags |= TCP_FLAG_OK;
        set_tcp_service_status(out, Status::Cancelled);
        set_tcp_lifecycle(
            out,
            if lookup.lifecycle_cause != SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
                lookup.lifecycle_cause
            } else {
                state::cause(out.conn_id)
            },
        );
        record_service_close(Status::Cancelled);
    } else {
        set_tcp_lifecycle(
            out,
            state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
        );
        record_service_close(Status::Failed);
    }
    b"".as_slice()
}
fn tcp_retransmit_result(
    out: &mut TcpServiceResult,
    payload: &[u8],
    client_id: u16,
) -> &'static [u8] {
    if payload.len() < 4 {
        return tcp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    let lookup = state::resolve(out.handle, client_id);
    if lookup.result != 0 {
        out.result = lookup.result;
        copy_bounded(&mut out.last_error[0..], lookup.last_error);
        set_tcp_lifecycle(out, lookup.lifecycle_cause);
        return b"".as_slice();
    }
    out.conn_id = lookup.conn_id;
    out.flags |= TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    let result = net::tcp_retransmit_last(out.conn_id);
    out.result = result;
    if result == 0 {
        out.flags |= TCP_FLAG_OK;
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
    } else {
        set_tcp_lifecycle(
            out,
            state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
        );
    }
    fill_tcp_connection_fields(out);
    b"".as_slice()
}
fn tcp_listen_result(out: &mut TcpServiceResult, payload: &[u8]) -> &'static [u8] {
    if payload.len() < 2 {
        return tcp_bad_request_result(out);
    }
    out.port = read_u16(payload, 0);
    out.local_port = out.port;
    out.local_ip = net::local_ip();
    if out.port == 0 {
        return tcp_bad_request_result(out);
    }
    if net::tcp_listen(out.port) {
        out.result = 0;
        out.flags |= TCP_FLAG_OK | TCP_FLAG_LISTENER;
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_LISTENER_PUBLIC);
    } else {
        out.result = -1;
        set_tcp_lifecycle(
            out,
            state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
        );
    }
    b"".as_slice()
}
fn tcp_accept_read_result<'a>(
    out: &mut TcpServiceResult,
    payload: &[u8],
    data_out: &'a mut [u8],
    client_id: u16,
) -> &'a [u8] {
    if payload.len() < 4 {
        return tcp_bad_request_result(out);
    }
    out.port = read_u16(payload, 0);
    let max_len = core::cmp::min(read_u16(payload, 2) as usize, data_out.len());
    out.requested_bytes = int(max_len);
    if out.port == 0 || max_len == 0 {
        return tcp_bad_request_result(out);
    }
    let mut conn_id: u32 = 0;
    let persistent_listener = net::tcp_has_listener(out.port);
    let got = if persistent_listener {
        net::tcp_accept_read_on_listener(out.port, &mut data_out[0..max_len], &mut conn_id)
    } else {
        net::tcp_accept_read_once(out.port, &mut data_out[0..max_len], &mut conn_id)
    };
    out.conn_id = conn_id;
    if got > 0 {
        let svc_handle = match state::allocate(conn_id, client_id) {
            Some(value) => value,
            None => {
                let _ = net::tcp_close(conn_id);
                out.result = -2;
                copy_bounded(&mut out.last_error[0..], b"handle-full".as_slice());
                return b"".as_slice();
            }
        };
        out.handle = svc_handle;
        out.result = 0;
        out.bytes = int(got);
        out.flags |= TCP_FLAG_OK | TCP_FLAG_DATA | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        return &data_out[0..usize::try_from(got).expect("Original bounded positive IPC length")];
    }
    if got == 0 {
        out.result = 0;
        out.flags |= TCP_FLAG_TIMEOUT;
        state::note_accept_timeout();
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
        return b"".as_slice();
    }
    out.result = got;
    if conn_id != 0 {
        out.flags |= TCP_FLAG_CONN_VALID;
    }
    set_tcp_lifecycle(
        out,
        state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
    );
    b"".as_slice()
}
fn tcp_accept_result(out: &mut TcpServiceResult, payload: &[u8], client_id: u16) -> &'static [u8] {
    if payload.len() < 2 {
        return tcp_bad_request_result(out);
    }
    out.port = read_u16(payload, 0);
    if out.port == 0 {
        return tcp_bad_request_result(out);
    }
    let mut conn_id: u32 = 0;
    let got = net::tcp_accept_on_listener(out.port, &mut conn_id);
    out.conn_id = conn_id;
    if got > 0 {
        let svc_handle = match state::allocate(conn_id, client_id) {
            Some(value) => value,
            None => {
                let _ = net::tcp_close(conn_id);
                out.result = -2;
                copy_bounded(&mut out.last_error[0..], b"handle-full".as_slice());
                return b"".as_slice();
            }
        };
        out.handle = svc_handle;
        out.result = 0;
        out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        return b"".as_slice();
    }
    if got == 0 {
        out.result = 0;
        out.flags |= TCP_FLAG_TIMEOUT;
        state::note_accept_timeout();
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
        return b"".as_slice();
    }
    out.result = got;
    if conn_id != 0 {
        out.flags |= TCP_FLAG_CONN_VALID;
    }
    set_tcp_lifecycle(
        out,
        state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
    );
    b"".as_slice()
}
fn tcp_accept_poll_result(
    out: &mut TcpServiceResult,
    payload: &[u8],
    client_id: u16,
) -> &'static [u8] {
    if payload.len() < 2 {
        return tcp_bad_request_result(out);
    }
    out.port = read_u16(payload, 0);
    if out.port == 0 {
        return tcp_bad_request_result(out);
    }
    let mut conn_id: u32 = 0;
    let got = net::tcp_accept_poll_on_listener(out.port, &mut conn_id);
    out.conn_id = conn_id;
    if got > 0 {
        let svc_handle = match state::allocate(conn_id, client_id) {
            Some(value) => value,
            None => {
                let _ = net::tcp_close(conn_id);
                out.result = -2;
                copy_bounded(&mut out.last_error[0..], b"handle-full".as_slice());
                return b"".as_slice();
            }
        };
        out.handle = svc_handle;
        out.result = 0;
        out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_ACTIVE_PUBLIC);
        return b"".as_slice();
    }
    if got == 0 {
        out.result = 0;
        out.flags |= TCP_FLAG_TIMEOUT;
        copy_bounded(&mut out.last_error[0..], b"accept-poll-empty".as_slice());
        set_tcp_service_status(out, Status::WouldBlock);
        set_tcp_lifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        return b"".as_slice();
    }
    out.result = got;
    if conn_id != 0 {
        out.flags |= TCP_FLAG_CONN_VALID;
    }
    set_tcp_lifecycle(
        out,
        state::lifecycle_from_error(state::text(net::tcp_stats().last_error)),
    );
    b"".as_slice()
}
fn tcp_close_listen_result(out: &mut TcpServiceResult, payload: &[u8]) -> &'static [u8] {
    if payload.len() < 2 {
        return tcp_bad_request_result(out);
    }
    out.port = read_u16(payload, 0);
    if out.port == 0 {
        return tcp_bad_request_result(out);
    }
    net::tcp_close_listener(out.port);
    out.result = 0;
    out.flags |= TCP_FLAG_OK | TCP_FLAG_LISTENER;
    set_tcp_lifecycle(out, SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC);
    b"".as_slice()
}
fn tcp_poll_result(out: &mut TcpServiceResult, payload: &[u8], client_id: u16) -> &'static [u8] {
    if payload.len() < 4 {
        return tcp_bad_request_result(out);
    }
    out.handle = read_u32(payload, 0);
    let lookup = state::resolve(out.handle, client_id);
    if lookup.result != 0 {
        out.result = lookup.result;
        copy_bounded(&mut out.last_error[0..], lookup.last_error);
        set_tcp_lifecycle(out, lookup.lifecycle_cause);
        return b"".as_slice();
    }
    out.conn_id = lookup.conn_id;
    net::tcp_poll_service();
    out.result = 0;
    out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    fill_tcp_connection_fields(out);
    if out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
        set_tcp_lifecycle(out, state::cause(out.conn_id));
    }
    b"".as_slice()
}
fn tcp_bad_request_result(out: &mut TcpServiceResult) -> &'static [u8] {
    out.result = -1;
    set_tcp_service_status(out, Status::Failed);
    copy_bounded(&mut out.last_error[0..], b"bad-request".as_slice());
    b"".as_slice()
}
fn fill_tcp_result_status(out: &mut TcpServiceResult) {
    state::reap();
    let mut summary: crate::contract::TcpSummary = crate::contract::TcpSummary::default();
    net::tcp_summary(&mut summary);
    out.active_connections = summary.active_connections;
    out.max_connections = summary.max_connections;
    out.active_listeners = summary.active_listeners;
    out.handle_count = state::count();
    out.max_handles = int(TCP_SERVICE_HANDLES);
    out.tcp_buffer_size = summary.buffer_size;
    out.message_payload_max = int(TCP_MESSAGE_PAYLOAD_MAX);
    out.write_max = int(TCP_WRITE_PAYLOAD_MAX);
    out.read_max = int(TCP_RESULT_DATA_MAX);
    if out.local_ip[0] == 0 && out.local_ip[1] == 0 && out.local_ip[2] == 0 && out.local_ip[3] == 0
    {
        out.local_ip = net::local_ip();
    }
    if out.conn_id != 0 {
        fill_tcp_connection_fields(out);
    }
    if out.last_error[0] == 0 {
        copy_bounded(&mut out.last_error[0..], net::tcp_stats().last_error);
    }
    if out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
        set_tcp_lifecycle(
            out,
            state::lifecycle_from_error(fixed_text(&out.last_error[0..])),
        );
    }
}
fn set_tcp_operation_status(out: &mut TcpServiceResult) {
    if service_status_encoded(out.flags) {
        out.service_status = service_operation_status_code(out.flags);
        return;
    }
    let last_error = fixed_text(&out.last_error[0..]);
    if (out.flags & TCP_FLAG_TIMEOUT) != 0 || text_contains(last_error, b"timeout".as_slice()) {
        set_tcp_service_status(out, Status::Timeout);
        if out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
            set_tcp_lifecycle(out, SOCKET_LIFECYCLE_TIMEOUT_PUBLIC);
        }
        return;
    }
    if out.result == 0 && (out.flags & TCP_FLAG_OK) != 0 {
        set_tcp_service_status(out, Status::Ok);
        return;
    }
    if text_equals(last_error, operation_status_name(Status::WouldBlock)) {
        set_tcp_service_status(out, Status::WouldBlock);
        if out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
            set_tcp_lifecycle(out, SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC);
        }
    } else {
        set_tcp_service_status(out, Status::Failed);
        if out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
            set_tcp_lifecycle(out, SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC);
        }
    }
}
fn fill_tcp_connection_fields(out: &mut TcpServiceResult) {
    let mut summary: crate::contract::TcpSummary = crate::contract::TcpSummary::default();
    net::tcp_summary(&mut summary);
    for index in 0..summary.max_connections {
        let mut info: crate::contract::TcpConnectionInfo =
            crate::contract::TcpConnectionInfo::default();
        if net::tcp_connection_info(index, &mut info) <= 0 {
            continue;
        }
        if info.id != out.conn_id {
            continue;
        }
        out.pending_rx = info.pending_rx;
        out.rx_window = info.rx_window;
        out.tx_window = info.tx_window;
        out.tx_seq = info.seq;
        out.tx_ack = info.tx_ack;
        out.retransmits = info.retransmits;
        out.rx_drops = info.rx_drops;
        out.local_ip = net::local_ip();
        out.local_port = info.local_port;
        if out.remote_ip[0] == 0
            && out.remote_ip[1] == 0
            && out.remote_ip[2] == 0
            && out.remote_ip[3] == 0
        {
            out.remote_ip = info.remote_ip;
        }
        if out.remote_port == 0 {
            out.remote_port = info.remote_port;
        }
        out.flags |= TCP_FLAG_CONN_VALID;
        return;
    }
}
fn write_tcp_connect(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
    if payload.len() < 6 {
        w.text(b"op=connect result=bad-request code=-1".as_slice());
        return;
    }
    let remote_ip: [u8; 4] = [payload[0], payload[1], payload[2], payload[3]];
    let port = read_u16(payload, 4);
    let conn = net::tcp_connect(remote_ip, port);
    w.text(b"op=connect result=".as_slice());
    if conn > 0 {
        let conn_id: u32 = int(conn);
        let svc_handle = match state::allocate(conn_id, client_id) {
            Some(value) => value,
            None => {
                let _ = net::tcp_close(conn_id);
                w.text(b"failed code=-2".as_slice());
                w.text(b" conn=".as_slice());
                w.num(conn_id);
                w.text(b" handle=0".as_slice());
                w.text(b" remote=".as_slice());
                w.ip(remote_ip);
                w.text(b":".as_slice());
                w.num(port);
                w.text(b" last=handle-full".as_slice());
                return;
            }
        };
        w.text(b"ok code=0 handle=".as_slice());
        w.num(svc_handle);
        w.text(b" conn=".as_slice());
        w.num(conn_id);
    } else {
        w.text(b"failed code=".as_slice());
        w.signed(conn);
        w.text(b" handle=0 conn=0".as_slice());
    }
    w.text(b" remote=".as_slice());
    w.ip(remote_ip);
    w.text(b":".as_slice());
    w.num(port);
    w.text(b" last=".as_slice());
    w.text(net::tcp_stats().last_error);
}
fn write_tcp_write(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
    if payload.len() < 4 {
        w.text(b"op=write result=bad-request code=-1".as_slice());
        return;
    }
    let svc_handle = read_u32(payload, 0);
    let lookup = state::resolve(svc_handle, client_id);
    if lookup.result != 0 {
        w.text(b"op=write result=failed code=".as_slice());
        w.signed(lookup.result);
        w.text(b" handle=".as_slice());
        w.num(svc_handle);
        w.text(b" last=".as_slice());
        w.text(lookup.last_error);
        w.text(b" handles=".as_slice());
        write_tcp_handle_usage(w);
        return;
    }
    let conn_id = lookup.conn_id;
    let data = &payload[4..];
    let written = net::tcp_write(conn_id, data);
    let total: usize = if written > 0 { int(written) } else { 0 };
    let write_error: i32 = if written < 0 { written } else { 0 };
    w.text(b"op=write result=".as_slice());
    if total > 0 || data.is_empty() {
        w.text(b"ok code=0 bytes=".as_slice());
        w.num(u32::try_from(total).expect("Original bounded positive IPC length"));
    } else {
        w.text(b"failed code=".as_slice());
        w.signed(write_error);
    }
    w.text(b" handle=".as_slice());
    w.num(svc_handle);
    w.text(b" conn=".as_slice());
    w.num(conn_id);
    w.text(b" last=".as_slice());
    w.text(net::tcp_stats().last_error);
}
#[inline(never)]
fn write_tcp_read(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
    if payload.len() < 6 {
        w.text(b"op=read result=bad-request code=-1".as_slice());
        return;
    }
    let svc_handle = read_u32(payload, 0);
    let lookup = state::resolve(svc_handle, client_id);
    if lookup.result != 0 {
        w.text(b"op=read result=failed code=".as_slice());
        w.signed(lookup.result);
        w.text(b" handle=".as_slice());
        w.num(svc_handle);
        w.text(b" last=".as_slice());
        w.text(lookup.last_error);
        w.text(b" handles=".as_slice());
        write_tcp_handle_usage(w);
        return;
    }
    let conn_id = lookup.conn_id;
    let max_len = core::cmp::min(
        read_u16(payload, 4) as usize,
        core::cmp::min(
            TCP_TEXT_DATA_MAX,
            w.out
                .len()
                .saturating_sub(w.pos)
                .saturating_sub(TCP_TEXT_METADATA_MAX),
        ),
    );
    let mut buf: [u8; TCP_TEXT_DATA_MAX] = [0; TCP_TEXT_DATA_MAX];
    let got = net::tcp_read(conn_id, &mut buf[0..max_len]);
    w.text(b"op=read result=".as_slice());
    if got >= 0 {
        w.text(b"ok code=0 bytes=".as_slice());
        w.num(u32::try_from(got).expect("Original bounded positive IPC length"));
        if got > 0 {
            w.text(b" data=".as_slice());
            w.bytes(&buf[0..usize::try_from(got).expect("Original bounded positive IPC length")]);
        }
    } else {
        w.text(b"failed code=".as_slice());
        w.signed(got);
    }
    w.text(b" handle=".as_slice());
    w.num(svc_handle);
    w.text(b" conn=".as_slice());
    w.num(conn_id);
    w.text(b" last=".as_slice());
    let last_error = net::tcp_stats().last_error;
    w.text(&state::text(last_error)[0..core::cmp::min(last_error.len, 32)]);
}
fn write_tcp_close(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
    if payload.len() < 4 {
        w.text(b"op=close result=bad-request code=-1".as_slice());
        record_service_close(Status::Failed);
        return;
    }
    let svc_handle = read_u32(payload, 0);
    let lookup = state::resolve_close(svc_handle, client_id);
    if lookup.result != 0 {
        record_service_close(Status::Failed);
        w.text(b"op=close result=failed code=".as_slice());
        w.signed(lookup.result);
        w.text(b" handle=".as_slice());
        w.num(svc_handle);
        w.text(b" last=".as_slice());
        w.text(lookup.last_error);
        w.text(b" handles=".as_slice());
        write_tcp_handle_usage(w);
        return;
    }
    let conn_id = lookup.conn_id;
    let result = net::tcp_close(conn_id);
    state::release(svc_handle);
    let mut status: Status = Status::Failed;
    w.text(b"op=close result=".as_slice());
    if result >= 0 {
        status = Status::Ok;
        record_service_close(status);
        w.text(b"ok code=".as_slice());
        w.num(0);
    } else if !state::usable(conn_id) {
        status = Status::Cancelled;
        record_service_close(status);
        w.text(b"ok code=".as_slice());
        w.num(0);
    } else {
        record_service_close(status);
        w.text(b"failed code=".as_slice());
        w.signed(result);
    }
    w.text(b" handle=".as_slice());
    w.num(svc_handle);
    w.text(b" conn=".as_slice());
    w.num(conn_id);
    w.text(b" last=".as_slice());
    w.text(net::tcp_stats().last_error);
    w.text(b" status=".as_slice());
    w.text(operation_status_name(status));
}
fn write_tcp_listen(w: &mut Writer<'_>, payload: &[u8]) {
    if payload.len() < 2 {
        w.text(b"op=listen result=bad-request code=-1".as_slice());
        return;
    }
    let port = read_u16(payload, 0);
    if port == 0 {
        w.text(b"op=listen result=bad-request code=-1 port=0".as_slice());
        return;
    }
    let ok = net::tcp_listen(port);
    w.text(b"op=listen result=".as_slice());
    w.text(if ok {
        b"ok".as_slice()
    } else {
        b"failed".as_slice()
    });
    w.text(b" port=".as_slice());
    w.num(port);
    w.text(b" code=".as_slice());
    w.signed(if ok { 0 } else { -1 });
    w.text(b" last=".as_slice());
    w.text(net::tcp_stats().last_error);
}
#[inline(never)]
fn write_tcp_accept_read(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
    if payload.len() < 4 {
        w.text(b"op=acceptread result=bad-request code=-1".as_slice());
        return;
    }
    let port = read_u16(payload, 0);
    let max_len = core::cmp::min(
        read_u16(payload, 2) as usize,
        core::cmp::min(
            TCP_TEXT_DATA_MAX,
            w.out
                .len()
                .saturating_sub(w.pos)
                .saturating_sub(TCP_TEXT_METADATA_MAX),
        ),
    );
    if port == 0 || max_len == 0 {
        w.text(b"op=acceptread result=bad-request code=-1".as_slice());
        return;
    }
    let mut conn_id: u32 = 0;
    let mut buf: [u8; TCP_TEXT_DATA_MAX] = [0; TCP_TEXT_DATA_MAX];
    let persistent_listener = net::tcp_has_listener(port);
    let got = if persistent_listener {
        net::tcp_accept_read_on_listener(port, &mut buf[0..max_len], &mut conn_id)
    } else {
        net::tcp_accept_read_once(port, &mut buf[0..max_len], &mut conn_id)
    };
    w.text(b"op=acceptread result=".as_slice());
    if got > 0 {
        let svc_handle = match state::allocate(conn_id, client_id) {
            Some(value) => value,
            None => {
                let _ = net::tcp_close(conn_id);
                w.text(b"failed code=-2 handle=0 conn=".as_slice());
                w.num(conn_id);
                w.text(b" port=".as_slice());
                w.num(port);
                w.text(b" last=handle-full".as_slice());
                return;
            }
        };
        w.text(b"ok code=0 bytes=".as_slice());
        w.num(u32::try_from(got).expect("Original bounded positive IPC length"));
        w.text(b" handle=".as_slice());
        w.num(svc_handle);
        w.text(b" conn=".as_slice());
        w.num(conn_id);
        w.text(b" data=".as_slice());
        w.bytes(&buf[0..usize::try_from(got).expect("Original bounded positive IPC length")]);
    } else if got == 0 {
        w.text(b"timeout code=0 bytes=0 conn=0".as_slice());
    } else {
        w.text(b"failed code=".as_slice());
        w.signed(got);
        w.text(b" conn=".as_slice());
        w.num(conn_id);
    }
    w.text(b" port=".as_slice());
    w.num(port);
    w.text(b" last=".as_slice());
    let last_error = net::tcp_stats().last_error;
    w.text(&state::text(last_error)[0..core::cmp::min(last_error.len, 32)]);
}
fn write_tcp_accept(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
    if payload.len() < 2 {
        w.text(b"op=accept result=bad-request code=-1".as_slice());
        return;
    }
    let port = read_u16(payload, 0);
    if port == 0 {
        w.text(b"op=accept result=bad-request code=-1".as_slice());
        return;
    }
    let mut conn_id: u32 = 0;
    let got = net::tcp_accept_on_listener(port, &mut conn_id);
    w.text(b"op=accept result=".as_slice());
    if got > 0 {
        let svc_handle = match state::allocate(conn_id, client_id) {
            Some(value) => value,
            None => {
                let _ = net::tcp_close(conn_id);
                w.text(b"failed port=".as_slice());
                w.num(port);
                w.text(b" code=-2 handle=0 conn=".as_slice());
                w.num(conn_id);
                w.text(b" last=handle-full".as_slice());
                return;
            }
        };
        w.text(b"ok port=".as_slice());
        w.num(port);
        w.text(b" code=0 handle=".as_slice());
        w.num(svc_handle);
        w.text(b" conn=".as_slice());
        w.num(conn_id);
    } else if got == 0 {
        w.text(b"timeout port=".as_slice());
        w.num(port);
        w.text(b" code=0 handle=0 conn=0".as_slice());
    } else {
        w.text(b"failed port=".as_slice());
        w.num(port);
        w.text(b" code=".as_slice());
        w.signed(got);
        w.text(b" handle=0 conn=".as_slice());
        w.num(conn_id);
    }
    w.text(b" last=".as_slice());
    w.text(net::tcp_stats().last_error);
}
fn write_tcp_close_listen(w: &mut Writer<'_>, payload: &[u8]) {
    if payload.len() < 2 {
        w.text(b"op=closelisten result=bad-request code=-1".as_slice());
        return;
    }
    let port = read_u16(payload, 0);
    if port == 0 {
        w.text(b"op=closelisten result=bad-request code=-1 port=0".as_slice());
        return;
    }
    net::tcp_close_listener(port);
    w.text(b"op=closelisten result=ok port=".as_slice());
    w.num(port);
    w.text(b" code=0".as_slice());
    w.text(b" last=".as_slice());
    w.text(net::tcp_stats().last_error);
}
fn write_tcp_poll(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
    if payload.len() < 4 {
        w.text(b"op=poll result=bad-request code=-1".as_slice());
        return;
    }
    let svc_handle = read_u32(payload, 0);
    let lookup = state::resolve(svc_handle, client_id);
    if lookup.result != 0 {
        w.text(b"op=poll result=failed code=".as_slice());
        w.signed(lookup.result);
        w.text(b" handle=".as_slice());
        w.num(svc_handle);
        w.text(b" last=".as_slice());
        w.text(lookup.last_error);
        w.text(b" handles=".as_slice());
        write_tcp_handle_usage(w);
        return;
    }
    let mut result = TcpServiceResult {
        handle: int(svc_handle),
        conn_id: int(lookup.conn_id),
        ..TcpServiceResult::EMPTY
    };
    fill_tcp_connection_fields(&mut result);
    w.text(b"op=poll result=ok code=0 handle=".as_slice());
    w.num(svc_handle);
    w.text(b" conn=".as_slice());
    w.num(lookup.conn_id);
    w.text(b" pending=".as_slice());
    w.num(result.pending_rx);
    w.text(b" win=".as_slice());
    w.num(result.rx_window);
    w.text(b" txwin=".as_slice());
    w.num(result.tx_window);
    w.text(b" txseq=".as_slice());
    w.num(result.tx_seq);
    w.text(b" txack=".as_slice());
    w.num(result.tx_ack);
    w.text(b" retrans=".as_slice());
    w.num(result.retransmits);
    w.text(b" drops=".as_slice());
    w.num(result.rx_drops);
    w.text(b" last=".as_slice());
    w.text(net::tcp_stats().last_error);
}
fn write_tcp_connections(w: &mut Writer<'_>) {
    state::reap();
    let mut summary: crate::contract::TcpSummary = crate::contract::TcpSummary::default();
    net::tcp_summary(&mut summary);
    w.text(b"connections=".as_slice());
    w.num(summary.active_connections);
    w.text(b"/".as_slice());
    w.num(summary.max_connections);
    w.text(b" handles=".as_slice());
    w.num(state::count());
    w.text(b"/".as_slice());
    w.num(TCP_SERVICE_HANDLES);
    let _index: u32 = 0;
    let mut printed: u32 = 0;
    for index in 0..summary.max_connections {
        let mut info: crate::contract::TcpConnectionInfo =
            crate::contract::TcpConnectionInfo::default();
        if net::tcp_connection_info(index, &mut info) <= 0 {
            continue;
        }
        printed += 1;
        w.text(b" #".as_slice());
        w.num(index);
        w.text(b" id=".as_slice());
        w.num(info.id);
        w.text(b" handle=".as_slice());
        w.num(state::for_connection_any(info.id).unwrap_or_default());
        w.text(b" owner=".as_slice());
        w.num(state::owner_for_connection(info.id));
        w.text(b" ".as_slice());
        w.text(tcp_state_name(info.state));
        w.text(b" l=".as_slice());
        w.num(info.local_port);
        w.text(b" r=".as_slice());
        w.ip(info.remote_ip);
        w.text(b":".as_slice());
        w.num(info.remote_port);
        w.text(b" rx=".as_slice());
        w.num(info.rx_bytes);
        w.text(b" tx=".as_slice());
        w.num(info.tx_bytes);
        w.text(b" pending=".as_slice());
        w.num(info.pending_rx);
        w.text(b" win=".as_slice());
        w.num(info.rx_window);
        w.text(b" txwin=".as_slice());
        w.num(info.tx_window);
        w.text(b" txack=".as_slice());
        w.num(info.tx_ack);
        w.text(b" retrans=".as_slice());
        w.num(info.retransmits);
        w.text(b" drops=".as_slice());
        w.num(info.rx_drops);
        w.text(b" seq=".as_slice());
        w.num(info.seq);
        w.text(b" ack=".as_slice());
        w.num(info.ack);
        w.text(b" last_flags=".as_slice());
        w.num(info.last_flags);
        w.text(b" last_payload=".as_slice());
        w.num(info.last_payload_len);
    }
    if printed == 0 {
        w.text(b" none".as_slice());
    }
}
fn write_tcp_handle_usage(w: &mut Writer<'_>) {
    w.num(state::count());
    w.text(b"/".as_slice());
    w.num(TCP_SERVICE_HANDLES);
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
        );
    }
}

unsafe fn native_handler(id: u32, data: Bytes, output: *mut u8, capacity: usize) -> i32 {
    unsafe { r4os_rust_net_ipc_handle(id, data.data, data.len, output, capacity) }
}
