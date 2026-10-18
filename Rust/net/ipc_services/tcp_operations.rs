//! Exact tcp_operations operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn set_tcp_service_status(out: &mut TcpServiceResult, status: Status) {
    out.flags = with_service_status(out.flags, status);
    out.service_status = status as u32;
}

pub(super) fn set_tcp_lifecycle(out: &mut TcpServiceResult, cause: u32) {
    if cause == 0 || out.lifecycle_cause == cause {
        return;
    }
    out.lifecycle_cause = cause;
    out.flags |= TCP_FLAG_LIFECYCLE_VALID;
    state::record_tcp(cause);
}

#[inline(never)]
pub(super) fn handle_tcp_result_operation(
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

pub(super) fn make_tcp_status_result(client_id: u16) -> TcpServiceStatus {
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

pub(super) fn is_tcp_operation(op: u16) -> bool {
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

pub(super) fn is_tcp_result_operation(op: u16) -> bool {
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

pub(super) fn make_tcp_result<'a>(
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

pub(super) fn tcp_result_action(op: u16) -> u16 {
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

pub(super) fn tcp_connect_result(
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

pub(super) fn tcp_write_result(
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

pub(super) fn tcp_read_result<'a>(
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

pub(super) fn tcp_close_result(
    out: &mut TcpServiceResult,
    payload: &[u8],
    client_id: u16,
) -> &'static [u8] {
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

pub(super) fn tcp_abort_result(
    out: &mut TcpServiceResult,
    payload: &[u8],
    client_id: u16,
) -> &'static [u8] {
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

pub(super) fn tcp_retransmit_result(
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

pub(super) fn tcp_listen_result(out: &mut TcpServiceResult, payload: &[u8]) -> &'static [u8] {
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

pub(super) fn tcp_accept_read_result<'a>(
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

pub(super) fn tcp_accept_result(
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

pub(super) fn tcp_accept_poll_result(
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

pub(super) fn tcp_close_listen_result(out: &mut TcpServiceResult, payload: &[u8]) -> &'static [u8] {
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

pub(super) fn tcp_poll_result(
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
    net::tcp_poll_service();
    out.result = 0;
    out.flags |= TCP_FLAG_OK | TCP_FLAG_HANDLE_VALID | TCP_FLAG_CONN_VALID;
    fill_tcp_connection_fields(out);
    if out.lifecycle_cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
        set_tcp_lifecycle(out, state::cause(out.conn_id));
    }
    b"".as_slice()
}

pub(super) fn tcp_bad_request_result(out: &mut TcpServiceResult) -> &'static [u8] {
    out.result = -1;
    set_tcp_service_status(out, Status::Failed);
    copy_bounded(&mut out.last_error[0..], b"bad-request".as_slice());
    b"".as_slice()
}

pub(super) fn fill_tcp_result_status(out: &mut TcpServiceResult) {
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

pub(super) fn set_tcp_operation_status(out: &mut TcpServiceResult) {
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

pub(super) fn fill_tcp_connection_fields(out: &mut TcpServiceResult) {
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
