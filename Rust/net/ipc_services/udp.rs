//! Exact udp operations of the parent owner; no duplicated state.
use super::*;

pub(super) fn set_udp_service_status(out: &mut UdpServiceResult, status: Status) {
    out.flags = with_service_status(out.flags, status);
    out.service_status = status as u32;
}

pub(super) fn set_udp_lifecycle(out: &mut UdpServiceResult, cause: u32) {
    if cause == 0 || out.lifecycle_cause == cause {
        return;
    }
    out.lifecycle_cause = cause;
    out.flags |= UDP_FLAG_LIFECYCLE_VALID;
    state::record_udp(cause);
}

pub(super) fn udp_last_error() -> Text {
    net::udp_error()
}

#[inline(never)]
pub(super) fn handle_udp_result_operation(
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

pub(super) fn write_udp_status(w: &mut Writer<'_>) {
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

pub(super) fn make_udp_status_result() -> UdpServiceStatus {
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

pub(super) fn write_udp_operation(w: &mut Writer<'_>, op: u16, payload: &[u8]) {
    match op {
        OP_UDP_BIND => write_udp_bind(w, payload),
        OP_UDP_SENDTO => write_udp_send_to(w, payload),
        OP_UDP_RECV => write_udp_recv(w, payload),
        OP_UDP_CLOSE => write_udp_close(w, payload),
        _ => unreachable!(),
    };
}

pub(super) fn is_udp_operation(op: u16) -> bool {
    op == OP_UDP_BIND || op == OP_UDP_SENDTO || op == OP_UDP_RECV || op == OP_UDP_CLOSE
}

pub(super) fn is_udp_result_operation(op: u16) -> bool {
    op == OP_UDP_BIND_RESULT
        || op == OP_UDP_SENDTO_RESULT
        || op == OP_UDP_RECV_RESULT
        || op == OP_UDP_CLOSE_RESULT
}

pub(super) fn make_udp_result<'a>(
    op: u16,
    payload: &[u8],
    data_out: &'a mut [u8],
) -> UdpResultPayload<'a> {
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

pub(super) fn udp_result_action(op: u16) -> u16 {
    match op {
        OP_UDP_BIND_RESULT => UDP_ACTION_BIND,
        OP_UDP_SENDTO_RESULT => UDP_ACTION_SENDTO,
        OP_UDP_RECV_RESULT => UDP_ACTION_RECV,
        OP_UDP_CLOSE_RESULT => UDP_ACTION_CLOSE,
        _ => 0,
    }
}

pub(super) fn udp_bind_result(out: &mut UdpServiceResult, payload: &[u8]) -> &'static [u8] {
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

pub(super) fn udp_send_to_result(out: &mut UdpServiceResult, payload: &[u8]) -> &'static [u8] {
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

pub(super) fn udp_recv_result<'a>(
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
    let mut info: super::super::udp_sockets::Info =
        unsafe { core::mem::zeroed::<super::super::udp_sockets::Info>() };
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

pub(super) fn udp_close_result(out: &mut UdpServiceResult, payload: &[u8]) -> &'static [u8] {
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

pub(super) fn udp_bad_request_result(out: &mut UdpServiceResult) -> &'static [u8] {
    out.result = -1;
    set_udp_service_status(out, Status::Failed);
    copy_bounded(&mut out.last_error[0..], b"bad-request".as_slice());
    b"".as_slice()
}

pub(super) fn fill_udp_result_status(out: &mut UdpServiceResult) {
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

pub(super) fn set_udp_operation_status(out: &mut UdpServiceResult) {
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

pub(super) fn write_udp_bind(w: &mut Writer<'_>, payload: &[u8]) {
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

pub(super) fn write_udp_send_to(w: &mut Writer<'_>, payload: &[u8]) {
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

pub(super) fn write_udp_recv(w: &mut Writer<'_>, payload: &[u8]) {
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
    let mut info: super::super::udp_sockets::Info =
        unsafe { core::mem::zeroed::<super::super::udp_sockets::Info>() };
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

pub(super) fn write_udp_close(w: &mut Writer<'_>, payload: &[u8]) {
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
