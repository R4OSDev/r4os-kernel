//! Original TCP transport/API coordination. Connection/catalog state remains
//! at the exact native TCP owner. Every plan is an owned copy; no TCP guard,
//! runtime owner or resident state reference spans packet I/O or a wait.
//! Foreign payload/output buffers stay raw across those boundaries.
use super::{
    adapters, builders, config, dhcp_client, protocol_runtime, r4p_contract as wire, tcp, tcp_rto,
    tcp_runtime::{self, ConnectionIdentity, SegmentKind},
    tcp_wait, timing, transmit,
};
use crate::{
    contract::TcpConnectionInfo,
    memory::owner::Guard,
    sched::{scheduler, sync_types::Text},
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
const POLL: usize = 32;
const SLICE: u64 = timing::ms_to_ticks(250);
const BAD_STATE: i32 = wire::TCP_RESULT_BAD_STATE;
#[repr(C)]
pub struct Peers {
    pub send_ipv4: unsafe extern "C" fn(*const [u8; 4], u8, *const u8, usize) -> u8,
    pub poll: unsafe extern "C" fn(usize),
}
struct PeerStorage(UnsafeCell<*const Peers>);
// SAFETY: singleton initialization installs an immutable kernel-lifetime table.
unsafe impl Sync for PeerStorage {}
static PEERS: PeerStorage = PeerStorage(UnsafeCell::new(ptr::null()));
fn peers() -> &'static Peers {
    let p = {
        let _guard = Guard::acquire(2);
        unsafe { *PEERS.0.get() }
    };
    assert!(
        !p.is_null(),
        "TCP transport peers installed before admission"
    );
    unsafe { &*p }
}
fn poll() {
    unsafe {
        (peers().poll)(POLL);
    }
}
fn established(id: u32) -> bool {
    unsafe { tcp::r4os_rust_net_tcp_established(id) }
}
fn closed(id: u32) -> bool {
    unsafe { tcp::r4os_rust_net_tcp_closed(id) }
}
fn error(reason: &[u8]) {
    unsafe {
        tcp::r4os_rust_net_tcp_set_error(reason.as_ptr(), reason.len());
    }
}
fn timeout(reason: &[u8]) {
    unsafe {
        tcp::r4os_rust_net_tcp_mark_timeout(reason.as_ptr(), reason.len());
    }
}
fn identity(id: u32) -> Option<ConnectionIdentity> {
    let mut out = MaybeUninit::uninit();
    if unsafe { tcp::r4os_rust_net_tcp_connection_identity(id, out.as_mut_ptr()) } {
        Some(unsafe { out.assume_init() })
    } else {
        None
    }
}
fn abort_connection(id: u32, reason: &[u8]) {
    let identity = identity(id);
    unsafe {
        tcp::r4os_rust_net_tcp_abort(id, reason.as_ptr(), reason.len());
    }
    if let Some(identity) = identity {
        unsafe {
            tcp_rto::r4os_rust_tcp_rto_release(&identity);
        }
    }
}
pub(super) fn next_seq(ip: [u8; 4], port: u16) -> u32 {
    let value = timing::now_ticks() as u32 ^ u32::from_be_bytes(ip) ^ (u32::from(port) << 8);
    if value == 0 { 0x5244_4350 } else { value }
}
pub(super) unsafe fn build(
    params: builders::WireTcpBuild,
    payload: *const u8,
    len: usize,
    out: *mut u8,
    capacity: usize,
) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_tcp(
            &params,
            payload,
            len,
            out,
            capacity,
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    protocol_runtime::r4os_rust_net_note_tcp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            protocol_runtime::r4os_rust_net_tcp_wire_error_set(report.reason);
        }
    }
    if report.length == usize::MAX {
        return None;
    }
    protocol_runtime::r4os_rust_net_note_tcp_r4p_build(1);
    protocol_runtime::r4os_rust_net_tcp_wire_flags_set(report.tcp_flags);
    Some(report.length)
}
unsafe fn send_plan(plan: tcp::SendPlan, flags: u16, payload: *const u8, len: usize) -> u8 {
    let mut segment = [0; 1536];
    let params = builders::WireTcpBuild {
        source_ip: config::settings().local_ip,
        dest_ip: plan.remote_ip,
        source_port: plan.local_port,
        dest_port: plan.remote_port,
        seq: plan.seq,
        ack: plan.ack,
        flags,
        window: plan.rx_window,
        options: plan.options,
    };
    let Some(length) =
        (unsafe { build(params, payload, len, segment.as_mut_ptr(), segment.len()) })
    else {
        return 4;
    };
    unsafe { (peers().send_ipv4)(&plan.remote_ip, 6, segment.as_ptr(), length) }
}
pub(super) unsafe fn send(id: u32, flags: u16, payload: *const u8, len: usize) -> Option<u8> {
    if !unsafe { tcp::r4os_rust_net_tcp_can_track_send(id, flags, len) } {
        return Some(3);
    }
    let mut plan = MaybeUninit::uninit();
    if !unsafe { tcp::r4os_rust_net_tcp_plan_send(id, flags, plan.as_mut_ptr()) } {
        return None;
    }
    let plan = unsafe { plan.assume_init() };
    let mut result = unsafe { send_plan(plan, flags, payload, len) };
    // Preserve bounded backend-busy retries for payload only; handshake/FIN
    // must never hold up the RX drain with this yield loop.
    if result == 3 && len != 0 {
        for _ in 0..64 {
            if result != 3 {
                break;
            }
            scheduler::yield_task();
            result = unsafe { send_plan(plan, flags, payload, len) };
        }
    }
    if result == 0 {
        if !unsafe {
            tcp::r4os_rust_net_tcp_commit_sent(id, flags, payload, len, timing::now_ticks())
        } {
            return Some(6);
        }
        if flags & tcp::FLAG_ACK != 0 {
            unsafe {
                tcp::r4os_rust_net_tcp_note_ack_sent(id, len != 0, plan.rx_window);
            }
        }
        if tcp_runtime::needs_tracking(flags, len) {
            tcp_rto::r4os_rust_tcp_rto_stamp_send(id);
        }
    }
    Some(result)
}
fn send_empty(id: u32, flags: u16) -> Option<u8> {
    unsafe { send(id, flags, b"".as_ptr(), 0) }
}
pub(super) fn retransmit(id: u32, kind: SegmentKind, reason: &[u8]) -> u8 {
    let mut plan = MaybeUninit::<tcp::RetransmitPlan>::uninit();
    if !unsafe { tcp::r4os_rust_net_tcp_plan_retransmit(id, kind as u8, plan.as_mut_ptr()) } {
        return 6;
    }
    let plan = unsafe { plan.assume_init() };
    let mut segment = [0; 1536];
    let params = builders::WireTcpBuild {
        source_ip: config::settings().local_ip,
        dest_ip: plan.remote_ip,
        source_port: plan.local_port,
        dest_port: plan.remote_port,
        seq: plan.seq,
        ack: plan.ack,
        flags: plan.flags,
        window: plan.rx_window,
        options: plan.options,
    };
    let payload = plan.payload_slice();
    let Some(length) = (unsafe {
        build(
            params,
            payload.as_ptr(),
            payload.len(),
            segment.as_mut_ptr(),
            segment.len(),
        )
    }) else {
        return 4;
    };
    let result = unsafe { (peers().send_ipv4)(&plan.remote_ip, 6, segment.as_ptr(), length) };
    if result == 0 {
        if !unsafe {
            tcp::r4os_rust_net_tcp_record_retransmit(
                id,
                plan.token,
                timing::now_ticks(),
                reason.as_ptr(),
                reason.len(),
            )
        } {
            return 6;
        }
        tcp_rto::r4os_rust_tcp_rto_refresh(id);
    }
    result
}
fn wait_established(id: u32, deadline: u64) -> bool {
    poll();
    loop {
        if established(id) {
            return true;
        }
        if closed(id) {
            return false;
        }
        let now = timing::now_ticks();
        if now >= deadline {
            return established(id);
        }
        tcp_wait::r4os_rust_net_tcp_wait_established(id, SLICE.min(deadline - now));
    }
}
unsafe fn wait_read(id: u32, out: *mut u8, len: usize, ticks: u64) -> Option<i32> {
    poll();
    let deadline = timing::Deadline::start(ticks, 1);
    loop {
        let got = unsafe { tcp::r4os_rust_net_tcp_read(id, out, len) };
        if got > 0 {
            window_update(id);
            return Some(got);
        }
        if got != 0 {
            return Some(got);
        }
        if deadline.expired_now() {
            return None;
        }
        tcp_wait::r4os_rust_net_tcp_wait_readable(id, SLICE);
    }
}
fn window_update(id: u32) {
    if unsafe { tcp::r4os_rust_net_tcp_window_update_required(id) } {
        send_empty(id, tcp::FLAG_ACK);
    }
}
fn connect(ip: [u8; 4], port: u16, deadline: Option<u64>) -> i32 {
    if adapters::r4os_rust_net_adapter_count() == 0 {
        error(b"no-adapter");
        return wire::TCP_RESULT_NO_CONNECTION;
    }
    if deadline.is_some_and(|d| timing::now_ticks() >= d) {
        timeout(b"connect-timeout");
        return BAD_STATE;
    }
    let connection =
        unsafe { tcp::r4os_rust_net_tcp_begin_live_connect(&ip, port, next_seq(ip, port)) };
    if connection <= 0 {
        return connection;
    }
    let id = connection as u32;
    let Some(syn) = send_empty(id, tcp::FLAG_SYN) else {
        abort_connection(id, b"syn-tx");
        return BAD_STATE;
    };
    if syn != 0 {
        abort_connection(id, transmit::result_name(syn));
        return BAD_STATE;
    }
    let mut retransmits = 0;
    loop {
        let now = timing::now_ticks();
        if deadline.is_some_and(|d| now >= d) {
            break;
        }
        let attempt = now.saturating_add(tcp_rto::r4os_rust_tcp_rto_backoff(id, retransmits));
        if wait_established(id, deadline.map_or(attempt, |d| d.min(attempt))) {
            break;
        }
        if deadline.is_some_and(|d| timing::now_ticks() >= d) || retransmits >= 5 {
            break;
        }
        if retransmit(id, SegmentKind::Syn, b"syn-retry") != 0 {
            break;
        }
        retransmits += 1;
    }
    if !established(id) {
        timeout(b"connect-timeout");
        abort_connection(id, b"connect-timeout");
        return BAD_STATE;
    }
    send_empty(id, tcp::FLAG_ACK);
    connection
}
unsafe fn write(id: u32, data: *const u8, len: usize) -> i32 {
    if !established(id) {
        return BAD_STATE;
    }
    if len == 0 {
        return 0;
    }
    unsafe {
        tcp::r4os_rust_net_tcp_record_write_start(len);
    }
    let mut total = 0;
    let mut segments = 0;
    let mut hard_error = false;
    while total < len {
        let allowed = unsafe { tcp::r4os_rust_net_tcp_send_allowance(id, len - total) };
        if allowed == 0 {
            break;
        }
        assert!(allowed <= len - total, "Exact TCP send allowance");
        let flags = tcp::FLAG_ACK
            | if total + allowed == len {
                tcp::FLAG_PSH
            } else {
                0
            };
        let Some(result) = (unsafe { send(id, flags, data.add(total), allowed) }) else {
            hard_error = true;
            break;
        };
        if result == 3 {
            unsafe {
                tcp::r4os_rust_net_tcp_record_backend_busy();
            }
            break;
        }
        if result != 0 {
            hard_error = true;
            break;
        }
        total += allowed;
        segments += 1;
    }
    let block = if total < len {
        unsafe { tcp::r4os_rust_net_tcp_write_block_reason(id) }
    } else {
        0
    };
    unsafe {
        tcp::r4os_rust_net_tcp_record_write_finish(len, total, segments, block);
    }
    if total != 0 {
        i32::try_from(total).expect("Original bounded TCP write result")
    } else if hard_error {
        BAD_STATE
    } else {
        0
    }
}
unsafe fn read(id: u32, out: *mut u8, len: usize, available: bool) -> i32 {
    if available && len == 0 {
        return 0;
    }
    poll();
    let immediate = unsafe { tcp::r4os_rust_net_tcp_read(id, out, len) };
    if immediate > 0 {
        window_update(id);
        return immediate;
    }
    if available || immediate != 0 {
        return immediate;
    }
    if let Some(got) = unsafe {
        wait_read(
            id,
            out,
            len,
            timing::DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS,
        )
    } {
        return got;
    }
    timeout(b"read-timeout");
    0
}
fn data_on_port(port: u16) -> Option<u32> {
    let mut id = 0;
    if unsafe { tcp::r4os_rust_net_tcp_connection_with_data_on_port(port, &mut id) } {
        Some(id)
    } else {
        None
    }
}
fn connection_on_port(port: u16) -> Option<u32> {
    let mut id = 0;
    if unsafe { tcp::r4os_rust_net_tcp_connection_on_port(port, &mut id) } {
        Some(id)
    } else {
        None
    }
}
fn has_listener(port: u16) -> bool {
    unsafe { tcp::r4os_rust_net_tcp_has_listener(port) }
}
fn close_listener(port: u16) {
    unsafe {
        tcp::r4os_rust_net_tcp_close_listener(port);
    }
}
fn abort_port(port: u16) {
    unsafe {
        tcp::r4os_rust_net_tcp_abort_connections_on_port(port, b"listen-timeout".as_ptr(), 14);
    }
}
fn claim(id: u32) {
    unsafe {
        tcp::r4os_rust_net_tcp_claim_accepted(id);
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadAccept {
    EchoOnce,
    ReadOnce,
    ReadOnListener,
}
unsafe fn accept_read(
    port: u16,
    out: *mut u8,
    len: usize,
    id_out: *mut u32,
    mode: ReadAccept,
) -> i32 {
    if mode != ReadAccept::EchoOnce {
        unsafe {
            id_out.write(0);
        }
    }
    if port == 0 || len == 0 {
        return BAD_STATE;
    }
    if adapters::r4os_rust_net_adapter_count() == 0 {
        timeout(b"listen-no-adapter");
        return BAD_STATE;
    }
    if mode == ReadAccept::ReadOnListener {
        if !has_listener(port) {
            timeout(b"listen-missing");
            return BAD_STATE;
        }
    } else if !unsafe { tcp::r4os_rust_net_tcp_listen(port) } {
        return BAD_STATE;
    }
    let deadline = timing::Deadline::start(timing::DEFAULT_TCP_LISTEN_TIMEOUT_TICKS, 0);
    loop {
        poll();
        if let Some(id) = data_on_port(port) {
            let got = unsafe { tcp::r4os_rust_net_tcp_read(id, out, len) };
            if got > 0 {
                if mode != ReadAccept::EchoOnce {
                    unsafe {
                        id_out.write(id);
                    }
                    claim(id);
                }
                window_update(id);
                if mode == ReadAccept::EchoOnce {
                    unsafe {
                        send(id, tcp::FLAG_ACK | tcp::FLAG_PSH, out, got as usize);
                    }
                    close(id);
                }
                if mode != ReadAccept::ReadOnListener {
                    close_listener(port);
                }
                return got;
            }
        }
        if deadline.expired_now() {
            break;
        }
        tcp_wait::r4os_rust_net_tcp_wait_port_data(port, SLICE);
    }
    if mode == ReadAccept::ReadOnListener {
        timeout(b"accept-timeout");
    } else {
        close_listener(port);
        abort_port(port);
        timeout(b"listen-timeout");
    }
    0
}
unsafe fn accept(port: u16, id_out: *mut u32, polling: bool) -> i32 {
    unsafe {
        id_out.write(0);
    }
    if port == 0 {
        return BAD_STATE;
    }
    // Poll admission checks the existing listener before adapter presence;
    // the blocking call preserves the opposite original order and timeout labels.
    if polling {
        if !has_listener(port) {
            error(b"listen-missing");
            return BAD_STATE;
        }
        if adapters::r4os_rust_net_adapter_count() == 0 {
            error(b"accept-poll-empty");
            return 0;
        }
    } else {
        if adapters::r4os_rust_net_adapter_count() == 0 {
            timeout(b"listen-no-adapter");
            return BAD_STATE;
        }
        if !has_listener(port) {
            timeout(b"listen-missing");
            return BAD_STATE;
        }
    }
    let deadline = timing::Deadline::start(timing::DEFAULT_TCP_LISTEN_TIMEOUT_TICKS, 0);
    loop {
        poll();
        if let Some(id) = connection_on_port(port) {
            unsafe {
                id_out.write(id);
            }
            claim(id);
            send_empty(id, tcp::FLAG_ACK);
            return 1;
        }
        if polling {
            error(b"accept-poll-empty");
            return 0;
        }
        if deadline.expired_now() {
            timeout(b"accept-timeout");
            return 0;
        }
        tcp_wait::r4os_rust_net_tcp_wait_port_connection(port, SLICE);
    }
}
fn drive_close(id: u32) -> bool {
    if !unsafe { tcp::r4os_rust_net_tcp_close_ready(id) } {
        return false;
    }
    send_empty(id, tcp::FLAG_ACK | tcp::FLAG_FIN) == Some(0)
}
fn close(id: u32) -> i32 {
    let identity = identity(id);
    if closed(id) {
        send_empty(id, tcp::FLAG_ACK);
        let result = unsafe { tcp::r4os_rust_net_tcp_close(id) };
        if let Some(identity) = identity {
            unsafe {
                tcp_rto::r4os_rust_tcp_rto_release(&identity);
            }
        }
        return result;
    }
    if !unsafe { tcp::r4os_rust_net_tcp_request_close(id) } {
        abort_connection(id, b"close-state");
        return BAD_STATE;
    }
    drive_close(id);
    wire::TCP_RESULT_OK
}
fn abort(id: u32) -> i32 {
    let mut found = false;
    for slot in 0..tcp::MAX_CONNECTIONS as u32 {
        let mut info = MaybeUninit::<TcpConnectionInfo>::uninit();
        if unsafe { tcp::r4os_rust_net_tcp_connection_info(slot, info.as_mut_ptr()) } <= 0 {
            continue;
        }
        if unsafe { info.assume_init() }.id == id {
            found = true;
            break;
        }
    }
    if !found {
        return wire::TCP_RESULT_NO_CONNECTION;
    }
    send_empty(id, tcp::FLAG_ACK | tcp::FLAG_RST);
    abort_connection(id, b"abort");
    wire::TCP_RESULT_OK
}
unsafe extern "C" fn resend_data(id: u32) -> u8 {
    retransmit(id, SegmentKind::Data, b"data-retry")
}
unsafe extern "C" fn resend_fin(id: u32) -> u8 {
    retransmit(id, SegmentKind::Fin, b"fin-retry")
}
fn maintenance(now: u64) {
    for slot in 0..tcp::MAX_CONNECTIONS {
        let mut id = 0;
        if unsafe { tcp::r4os_rust_net_tcp_delayed_ack_due(slot, now, &mut id) } {
            send_empty(id, tcp::FLAG_ACK);
        }
    }
    let peers = tcp_rto::RtoPeers {
        data: resend_data,
        fin: resend_fin,
    };
    unsafe {
        tcp_rto::r4os_rust_tcp_rto_sweep(now, &peers);
    }
    for slot in 0..tcp::MAX_CONNECTIONS {
        let mut identity = MaybeUninit::<ConnectionIdentity>::uninit();
        if unsafe { tcp::r4os_rust_net_tcp_connection_identity_at(slot, identity.as_mut_ptr()) } {
            drive_close(unsafe { identity.assume_init() }.connection_id);
        }
    }
    unsafe {
        tcp::r4os_rust_net_tcp_reap_time_wait(now);
    }
    dhcp_client::r4os_rust_dhcp_client_retry_task();
}
#[repr(C)]
pub struct SendResult {
    pub available: bool,
    pub result: u8,
}
/// # Safety
/// Singleton bootstrap installs one immutable kernel-lifetime peer table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_reset(input: *const Peers) {
    let _guard = Guard::acquire(2);
    unsafe {
        PEERS.0.get().write(input);
    }
}
/// # Safety
/// Exact readable IP retained through transport/waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_connect(
    ip: *const [u8; 4],
    port: u16,
    bounded: bool,
    tick: u64,
) -> i32 {
    let ip = unsafe { *ip };
    connect(ip, port, if bounded { Some(tick) } else { None })
}
/// # Safety
/// Readable caller bytes remain retained through transport/waits/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_write(id: u32, data: *const u8, len: usize) -> i32 {
    unsafe { write(id, data, len) }
}
/// # Safety
/// Exclusive writable caller output retained through transport/waits/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_read(id: u32, out: *mut u8, len: usize) -> i32 {
    unsafe { read(id, out, len, false) }
}
/// # Safety
/// Exclusive writable caller output retained through transport/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_read_available(
    id: u32,
    out: *mut u8,
    len: usize,
) -> i32 {
    unsafe { read(id, out, len, true) }
}
/// # Safety
/// Exclusive writable caller output retained through transport/waits/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_echo_once(
    port: u16,
    out: *mut u8,
    len: usize,
) -> i32 {
    unsafe { accept_read(port, out, len, ptr::null_mut(), ReadAccept::EchoOnce) }
}
/// # Safety
/// Separate exclusive writable caller data/ID retained through waits/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_accept_read_once(
    port: u16,
    out: *mut u8,
    len: usize,
    id: *mut u32,
) -> i32 {
    unsafe { accept_read(port, out, len, id, ReadAccept::ReadOnce) }
}
/// # Safety
/// Separate exclusive writable caller data/ID retained through waits/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_accept_read(
    port: u16,
    out: *mut u8,
    len: usize,
    id: *mut u32,
) -> i32 {
    unsafe { accept_read(port, out, len, id, ReadAccept::ReadOnListener) }
}
/// # Safety
/// Exact writable caller ID retained through transport/waits/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_accept(port: u16, id: *mut u32) -> i32 {
    unsafe { accept(port, id, false) }
}
/// # Safety
/// Exact writable caller ID retained through transport/copies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_accept_poll(port: u16, id: *mut u32) -> i32 {
    unsafe { accept(port, id, true) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_client_close(id: u32) -> i32 {
    close(id)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_client_abort(id: u32) -> i32 {
    abort(id)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_client_retransmit_last(id: u32) -> i32 {
    if retransmit(id, SegmentKind::Data, b"data-retry") == 0 {
        0
    } else {
        BAD_STATE
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_client_maintenance(now: u64) {
    maintenance(now);
}
/// # Safety
/// Readable caller bytes and separate exact writable result, retained through
/// transport/copies. The optional result uses an explicit paired C record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_send(
    id: u32,
    flags: u16,
    data: *const u8,
    len: usize,
    out: *mut SendResult,
) {
    let result = unsafe { send(id, flags, data, len) };
    unsafe {
        out.write(SendResult {
            available: result.is_some(),
            result: result.unwrap_or(0),
        });
    }
}
/// # Safety
/// Readable retained original immutable kernel-string label.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_retransmit(
    id: u32,
    kind: SegmentKind,
    reason: Text,
) -> u8 {
    let reason = if reason.len == 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(reason.pointer, reason.len) }
    };
    retransmit(id, kind, reason)
}
/// # Safety
/// Exact readable IP.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_client_next_seq(ip: *const [u8; 4], port: u16) -> u32 {
    next_seq(unsafe { *ip }, port)
}

const _: () = {
    assert!(core::mem::size_of::<Peers>() == 16);
    assert!(core::mem::align_of::<Peers>() == 8);
    assert!(core::mem::offset_of!(Peers, send_ipv4) == 0);
    assert!(core::mem::offset_of!(Peers, poll) == 8);
};

const _: () = {
    assert!(core::mem::size_of::<SendResult>() == 2);
    assert!(core::mem::align_of::<SendResult>() == 1);
    assert!(core::mem::offset_of!(SendResult, available) == 0);
    assert!(core::mem::offset_of!(SendResult, result) == 1);
};
