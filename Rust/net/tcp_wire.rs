//! Original TCP RX/TX dispatch, ACK policy and generation-bound RTO updates.
//! A segment view borrows this activation's owned wire record. That record
//! stays in the same frame through native TCP operations and any send/wait.
use super::{
    ipv4, protocol_runtime, r4p_contract as wire, readers, tcp, tcp_client, tcp_rto,
    tcp_runtime::ConnectionIdentity, tcp_wait, timing,
};
use crate::sched::sync_types::Text;
use core::mem::MaybeUninit;
pub type NativeIpView = ipv4::PacketView;
unsafe fn decode(packet: NativeIpView, tx: bool, checksum: bool) -> Option<wire::TcpOp> {
    let input = readers::WireReadInput {
        source_ip: packet.source_ip,
        dest_ip: packet.dest_ip,
        own_mac: [0; 6],
        protocol: packet.protocol,
        l4_checksum_valid: checksum,
    };
    let mut op = MaybeUninit::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        if tx {
            readers::r4os_rust_wire_read_tcp_tx(
                &input,
                packet.payload.pointer,
                packet.payload.len,
                op.as_mut_ptr(),
                report.as_mut_ptr(),
            );
        } else {
            readers::r4os_rust_wire_read_tcp_rx(
                &input,
                packet.payload.pointer,
                packet.payload.len,
                op.as_mut_ptr(),
                report.as_mut_ptr(),
            );
        }
    }
    let report = unsafe { report.assume_init() };
    protocol_runtime::r4os_rust_net_note_tcp_dispatch_failures(report.dispatch_failures);
    if report.reason.len != 0 {
        unsafe {
            protocol_runtime::r4os_rust_net_tcp_wire_error_set(report.reason);
        }
    }
    if !report.completed {
        return None;
    }
    let op = unsafe { op.assume_init() };
    protocol_runtime::r4os_rust_net_tcp_wire_result(op.result, op.flags);
    Some(op)
}
fn view(op: &wire::TcpOp, packet: NativeIpView) -> Option<tcp::SegmentView> {
    let len = op.payload_len as usize;
    if len > op.payload.len() {
        return None;
    }
    let mut scale = 0;
    let present = unsafe { tcp::r4os_rust_net_tcp_option_window_scale(op.index, &mut scale) };
    Some(tcp::SegmentView {
        source_ip: packet.source_ip,
        dest_ip: packet.dest_ip,
        source_port: op.source_port,
        dest_port: op.dest_port,
        seq: op.seq,
        ack: op.ack,
        flags: op.flags,
        window: op.reserved,
        mss: unsafe { tcp::r4os_rust_net_tcp_option_mss(op.index) },
        window_scale: if present { scale } else { 0 },
        window_scale_present: present,
        payload: Text {
            pointer: op.payload.as_ptr(),
            len,
        },
    })
}
fn bad_view() {
    protocol_runtime::r4os_rust_net_note_tcp_dispatch_failures(1);
    unsafe {
        protocol_runtime::r4os_rust_net_tcp_wire_error_set(Text::literal(b"r4p-view"));
    }
}
pub(super) unsafe fn rx(packet: NativeIpView, checksum: bool) {
    let Some(op) = (unsafe { decode(packet, false, checksum) }) else {
        return;
    };
    protocol_runtime::r4os_rust_net_tcp_wire_reject(op.result);
    if op.result != wire::TCP_RESULT_OK {
        return;
    }
    protocol_runtime::r4os_rust_net_note_tcp_r4p_rx(1);
    // Form the view only after the operation's final move into this frame.
    let Some(view) = view(&op, packet) else {
        bad_view();
        return;
    };
    let mut identity = MaybeUninit::<ConnectionIdentity>::uninit();
    let identity = if unsafe {
        tcp::r4os_rust_net_tcp_connection_identity_for_segment(&view, identity.as_mut_ptr())
    } {
        Some(unsafe { identity.assume_init() })
    } else {
        None
    };
    let id = if let Some(identity) = identity {
        Some(identity.connection_id)
    } else {
        let mut id = 0;
        if unsafe { tcp::r4os_rust_net_tcp_connection_id_for_segment(&view, &mut id) } {
            Some(id)
        } else {
            None
        }
    };
    let previous_ack = id.map_or(0, |id| unsafe { tcp::r4os_rust_net_tcp_rx_ack_of(id) });
    let mut parsed = MaybeUninit::<tcp::SegmentView>::uninit();
    if !unsafe { tcp::r4os_rust_net_tcp_apply_rx_view(&view, parsed.as_mut_ptr()) } {
        return;
    }
    let parsed = unsafe { parsed.assume_init() };
    if parsed.flags & tcp::FLAG_SYN != 0 && parsed.flags & tcp::FLAG_ACK == 0 {
        let mut id = 0;
        if unsafe {
            tcp::r4os_rust_net_tcp_accept_inbound(
                &parsed,
                tcp_client::next_seq(parsed.source_ip, parsed.source_port),
                &mut id,
            )
        } {
            unsafe {
                tcp_client::send(id, tcp::FLAG_SYN | tcp::FLAG_ACK, b"".as_ptr(), 0);
            }
        }
    } else if (parsed.payload.len != 0 || parsed.flags & tcp::FLAG_FIN != 0)
        && parsed.flags & tcp::FLAG_RST == 0
        && let Some(id) = id
        && unsafe {
            tcp::r4os_rust_net_tcp_request_ack(
                id,
                previous_ack,
                &parsed,
                timing::now_ticks(),
                timing::ms_to_ticks(40),
            )
        } == tcp::AckRequest::Immediate as u8
    {
        unsafe {
            tcp_client::send(id, tcp::FLAG_ACK, b"".as_ptr(), 0);
        }
    }
    if parsed.flags & tcp::FLAG_ACK != 0
        && let Some(identity) = identity
    {
        unsafe {
            tcp_rto::r4os_rust_tcp_rto_on_ack(&identity);
        }
    }
    if let Some(identity) = identity
        && !unsafe { tcp::r4os_rust_net_tcp_identity_active(&identity) }
    {
        unsafe {
            tcp_rto::r4os_rust_tcp_rto_release(&identity);
        }
    }
    tcp_wait::r4os_rust_net_tcp_activity_signal();
}
pub(super) unsafe fn tx(packet: NativeIpView) {
    let Some(op) = (unsafe { decode(packet, true, false) }) else {
        return;
    };
    if op.result != wire::TCP_RESULT_OK {
        return;
    }
    let Some(view) = view(&op, packet) else {
        bad_view();
        return;
    };
    let mut parsed = MaybeUninit::<tcp::SegmentView>::uninit();
    unsafe {
        tcp::r4os_rust_net_tcp_apply_tx_view(&view, parsed.as_mut_ptr());
    }
    protocol_runtime::r4os_rust_net_note_tcp_r4p_tx(1);
}
/// # Safety
/// Readable paired view and retained packet bytes throughout R4P dispatch,
/// native state changes, ACK sends and waits. No CPU owner spans these calls.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_wire_receive(
    input: *const NativeIpView,
    checksum: bool,
) {
    let input = unsafe { *input };
    unsafe {
        rx(input, checksum);
    }
}
/// # Safety
/// Readable paired view and retained packet bytes throughout R4P dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_wire_transmitted(input: *const NativeIpView) {
    let input = unsafe { *input };
    unsafe {
        tx(input);
    }
}

const _: () = {
    assert!(core::mem::size_of::<NativeIpView>() == 32);
    assert!(core::mem::align_of::<NativeIpView>() == 8);
    assert!(core::mem::offset_of!(NativeIpView, protocol) == 0);
    assert!(core::mem::offset_of!(NativeIpView, source_ip) == 1);
    assert!(core::mem::offset_of!(NativeIpView, dest_ip) == 5);
    assert!(core::mem::offset_of!(NativeIpView, payload) == 16);
};
