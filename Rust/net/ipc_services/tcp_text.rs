//! Exact tcp_text operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn tcp_state_name(state: u8) -> &'static [u8] {
    match state {
        0 => b"closed",
        1 => b"syn-sent",
        2 => b"established",
        3 => b"fin-wait",
        4 => b"syn-received",
        _ => b"unknown",
    }
}

pub(super) fn write_tcp_status(w: &mut Writer<'_>) {
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

pub(super) fn write_tcp_operation(w: &mut Writer<'_>, op: u16, payload: &[u8], client_id: u16) {
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

pub(super) fn write_tcp_connect(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
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

pub(super) fn write_tcp_write(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
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
pub(super) fn write_tcp_read(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
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

pub(super) fn write_tcp_close(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
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

pub(super) fn write_tcp_listen(w: &mut Writer<'_>, payload: &[u8]) {
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
pub(super) fn write_tcp_accept_read(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
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

pub(super) fn write_tcp_accept(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
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

pub(super) fn write_tcp_close_listen(w: &mut Writer<'_>, payload: &[u8]) {
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

pub(super) fn write_tcp_poll(w: &mut Writer<'_>, payload: &[u8], client_id: u16) {
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

pub(super) fn write_tcp_connections(w: &mut Writer<'_>) {
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

pub(super) fn write_tcp_handle_usage(w: &mut Writer<'_>) {
    w.num(state::count());
    w.text(b"/".as_slice());
    w.num(TCP_SERVICE_HANDLES);
}
