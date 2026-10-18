//! Exact foreign_api operations of the parent owner; no duplicated state.
use super::*;

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_slot_bytes() -> usize {
    core::mem::size_of::<Connection>()
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_option_metadata(mss: u16, window_scale: u8) -> u32 {
    option_metadata(
        mss,
        if window_scale == 255 {
            None
        } else {
            Some(window_scale)
        },
    )
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_option_mss(metadata: u32) -> u16 {
    option_mss(metadata)
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_option_window_scale(
    metadata: u32,
    result: *mut u8,
) -> bool {
    unsafe {
        if let Some(value) = option_window_scale(metadata) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_reset() {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { reset() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_get_stats(result: *mut Stats) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        unsafe {
            result.write(Stats {
                last_error: Text::literal(b"cancelled"),
                ..Stats::EMPTY
            })
        };
        return;
    };
    unsafe { result.write(stats().read()) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_write_start(requested: usize) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { record_write_start(requested) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_write_finish(
    requested: usize,
    written: usize,
    segments: usize,
    block: u8,
) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        record_write_finish(
            requested,
            written,
            segments,
            match block {
                0 => WriteBlockReason::None,
                1 => WriteBlockReason::RemoteWindow,
                2 => WriteBlockReason::Catalog,
                3 => WriteBlockReason::State,
                _ => panic!("TCP typed discriminant"),
            },
        )
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_backend_busy() {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { record_backend_busy() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_totals(
    segment_count: *mut u32,
    byte_count: *mut u32,
) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        unsafe {
            segment_count.write(0);
            byte_count.write(0)
        };
        return;
    };
    unsafe {
        segment_count.write((*stats()).outstanding_segments_current);
        byte_count.write((*stats()).outstanding_bytes_current)
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_summary(out: *mut Summary) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        unsafe {
            out.write(Summary {
                max_connections: MAX_CONNECTIONS as u32,
                buffer_size: BUFFER_SIZE as u32,
                ..Summary::default()
            })
        };
        return;
    };
    unsafe { out.write(summary()) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_info(
    index: u32,
    out: *mut ConnectionInfo,
) -> i32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe {
        if let Some(value) = connection_info(index) {
            out.write(value);
            1
        } else {
            0
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_self_test_connect(
    remote_ip: *const [u8; 4],
    remote_port: u16,
) -> i32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { self_test_connect(*remote_ip, remote_port) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_begin_live_connect(
    remote_ip: *const [u8; 4],
    remote_port: u16,
    initial_seq: u32,
) -> i32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { begin_live_connect(*remote_ip, remote_port, initial_seq) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_listen(port: u16) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { listen(port) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_close_listener(port: u16) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { close_listener(port) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_has_listener(port: u16) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { has_listener(port) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_accept_inbound(
    view: *const SegmentView,
    initial_seq: u32,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = accept_inbound(*view, initial_seq) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_abort(
    conn_id: u32,
    reason: *const u8,
    reason_len: usize,
) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        abort(
            conn_id,
            Text {
                pointer: reason,
                len: reason_len,
            },
        )
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_mark_timeout(reason: *const u8, reason_len: usize) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        mark_timeout(Text {
            pointer: reason,
            len: reason_len,
        })
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_malformed(reason: *const u8, reason_len: usize) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        record_malformed(Text {
            pointer: reason,
            len: reason_len,
        })
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_checksum_error() {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { record_checksum_error() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_set_error(reason: *const u8, reason_len: usize) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        set_error(Text {
            pointer: reason,
            len: reason_len,
        })
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_established(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { established(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_request_close(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { request_close(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_close_ready(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { close_ready(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_fin_waiting(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { fin_waiting(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_identity(
    conn_id: u32,
    result: *mut ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_identity(conn_id) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_identity_at(
    slot: usize,
    result: *mut ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_identity_at(slot) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_identity_active(
    identity: *const ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { identity_active(*identity) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_with_data_on_port(
    port: u16,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_with_data_on_port(port) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_on_port(port: u16, result: *mut u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_on_port(port) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_claim_accepted(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { claim_accepted(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_id_for_segment(
    view: *const SegmentView,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_id_for_segment(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_connection_identity_for_segment(
    view: *const SegmentView,
    result: *mut ConnectionIdentity,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = connection_identity_for_segment(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_abort_connections_on_port(
    port: u16,
    reason: *const u8,
    reason_len: usize,
) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        abort_connections_on_port(
            port,
            Text {
                pointer: reason,
                len: reason_len,
            },
        )
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_abort_all(
    reason: *const u8,
    reason_len: usize,
    result: *mut CleanupResult,
) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        unsafe { result.write(CleanupResult::EMPTY) };
        return;
    };
    unsafe {
        result.write(abort_all(Text {
            pointer: reason,
            len: reason_len,
        }))
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_plan_send(
    conn_id: u32,
    flags: u16,
    result: *mut SendPlan,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = plan_send(conn_id, flags) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_send_allowance(conn_id: u32, requested: usize) -> usize {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { send_allowance(conn_id, requested) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_write_block_reason(conn_id: u32) -> u8 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 3;
    };
    unsafe { write_block_reason(conn_id) as u8 }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_can_track_send(
    conn_id: u32,
    flags: u16,
    payload_len: usize,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { can_track_send(conn_id, flags, payload_len) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_plan_retransmit(
    conn_id: u32,
    kind: u8,
    result: *mut RetransmitPlan,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = plan_retransmit(
            conn_id,
            match kind {
                0 => RetransmitKind::Any,
                1 => RetransmitKind::Data,
                2 => RetransmitKind::Syn,
                3 => RetransmitKind::Fin,
                _ => panic!("TCP typed discriminant"),
            },
        ) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_info(
    conn_id: u32,
    kind: u8,
    result: *mut OutstandingInfo,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = outstanding_info(
            conn_id,
            match kind {
                0 => RetransmitKind::Any,
                1 => RetransmitKind::Data,
                2 => RetransmitKind::Syn,
                3 => RetransmitKind::Fin,
                _ => panic!("TCP typed discriminant"),
            },
        ) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_count(conn_id: u32) -> usize {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { outstanding_count(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_outstanding_bytes(conn_id: u32) -> usize {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { outstanding_bytes(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_commit_sent(
    conn_id: u32,
    flags: u16,
    payload: *const u8,
    payload_len: usize,
    sent_tick: u64,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    if payload_len > SENT_PAYLOAD_CAPACITY {
        return false;
    }
    let mut staged = [0u8; SENT_PAYLOAD_CAPACITY];
    if payload_len != 0 {
        unsafe { core::ptr::copy_nonoverlapping(payload, staged.as_mut_ptr(), payload_len) };
    }
    unsafe { commit_sent(conn_id, flags, &staged[..payload_len], sent_tick) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_record_retransmit(
    conn_id: u32,
    token: u64,
    sent_tick: u64,
    reason: *const u8,
    reason_len: usize,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        record_retransmit(
            conn_id,
            token,
            sent_tick,
            Text {
                pointer: reason,
                len: reason_len,
            },
        )
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_tx_ack_of(conn_id: u32) -> u32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { tx_ack_of(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_rx_ack_of(conn_id: u32) -> u32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { rx_ack_of(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_request_ack(
    conn_id: u32,
    previous_ack: u32,
    view: *const SegmentView,
    now: u64,
    delay_ticks: u64,
) -> u8 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { request_ack(conn_id, previous_ack, *view, now, delay_ticks) as u8 }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_delayed_ack_due(
    slot: usize,
    now: u64,
    result: *mut u32,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = delayed_ack_due(slot, now) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_note_ack_sent(
    conn_id: u32,
    piggybacked: bool,
    advertised_window: u16,
) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe { note_ack_sent(conn_id, piggybacked, advertised_window) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_window_update_required(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { window_update_required(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_readable(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { readable(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_closed(conn_id: u32) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { closed(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_loopback_write(
    conn_id: u32,
    data: *const u8,
    data_len: usize,
) -> i32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { loopback_write(conn_id, data, data_len) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_read(conn_id: u32, out: *mut u8, out_len: usize) -> i32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { read_foreign(conn_id, out, out_len) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_close(conn_id: u32) -> i32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return -1;
    };
    unsafe { close(conn_id) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_buffer_limit_probe() -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { buffer_limit_probe() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_out_of_order_probe() -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { out_of_order_probe() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_apply_rx_view(
    view: *const SegmentView,
    result: *mut SegmentView,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = apply_rx_view(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_apply_tx_view(
    view: *const SegmentView,
    result: *mut SegmentView,
) -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe {
        if let Some(value) = apply_tx_view(*view) {
            result.write(value);
            true
        } else {
            false
        }
    }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_reap_time_wait(now: u64) -> u32 {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return 0;
    };
    unsafe { reap_time_wait(now) }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_overlap_retransmit_probe() -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { overlap_retransmit_probe() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_wraparound_probe() -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { wraparound_probe() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_graceful_close_probe() -> bool {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return false;
    };
    unsafe { graceful_close_probe() }
}

/// # Safety
/// Exact private C records and bounded borrowed ranges; original caller
/// ownership and stable diagnostic-string lifetimes remain in force.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_tcp_performance_contract_probe() -> bool {
    let original = {
        let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
            return false;
        };
        unsafe { performance_contract_probe() }
    };
    original && super::super::tcp_wait::check()
}

/// Caller retains exact TCP task admission; output may fault. Every state
/// borrow ends before copying to it. The original initial read limit fixes
/// the prefix, including when intermediate reads flush out-of-order data.
pub(super) unsafe fn read_foreign(id: u32, out: *mut u8, capacity: usize) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        let total = capacity.min((*c).rx_len);
        if total == 0 {
            return read(id, &mut []);
        }
        let mut staged = [0u8; 4096];
        let mut copied = 0;
        while copied < total {
            let run = (total - copied).min(staged.len());
            let got = read(id, &mut staged[..run]);
            assert_eq!(got, run as i32, "Exact admitted TCP read prefix");
            core::ptr::copy_nonoverlapping(staged.as_ptr(), out.add(copied), run);
            copied += run;
        }
        total as i32
    }
}

/// Caller retains exact TCP task admission. No resident-table reference is
/// live during a potentially faulting read from foreign caller bytes.
pub(super) unsafe fn loopback_write_foreign(id: u32, data: *const u8, len: usize) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        if (*c).state != State::Established || len > BUFFER_SIZE {
            return -1;
        }
        (*c).tx_bytes += len as u64;
        (*stats()).data_tx += 1;
        let total = len.min(rx_free(&*c));
        let mut copied = 0;
        let mut staged = [0u8; 4096];
        while copied < total {
            let run = (total - copied).min(staged.len());
            core::ptr::copy_nonoverlapping(data.add(copied), staged.as_mut_ptr(), run);
            let wrote = write_rx(&mut *c, &staged[..run]);
            assert_eq!(wrote, run, "Exact admitted TCP loopback prefix");
            copied += run;
        }
        (*c).rx_bytes += copied as u64;
        (*stats()).data_rx += 1;
        len as i32
    }
}

/// # Safety
/// Native TCP task owner is held and slot is in the fixed connection table.
/// This copied readiness projection is published before scheduler enrollment
/// or wake. A scheduler predicate must never acquire the waitable TCP owner.
pub(crate) unsafe fn wait_observation_word(slot: usize) -> u64 {
    unsafe {
        let c = &*connection(slot);
        if !c.used {
            return 0;
        }
        let mut flags = 0u64;
        // The same owner-local state rules as established/readable/closed.
        if c.state == State::Established {
            flags |= 1;
        }
        if c.rx_len != 0 || c.state == State::Closed {
            flags |= 2;
        }
        if matches!(c.state, State::Closed | State::TimeWait | State::LastAck) {
            flags |= 4;
        }
        if c.state == State::Established && !c.accepted_claimed {
            flags |= 8;
            if c.rx_len != 0 {
                flags |= 16;
            }
        }
        u64::from(c.id) | (u64::from(c.local_port) << 32) | (flags << 48)
    }
}

/// Non-owning projection only, under the exact TCP task owner. Native
/// connection generation/ID and listener generation remain the authorities.
pub(crate) unsafe fn wait_observations(
    connections: &mut [(u64, u64); MAX_CONNECTIONS],
    listeners: &mut [(u64, u64); MAX_LISTENERS],
) {
    unsafe {
        for (slot, out) in listeners.iter_mut().enumerate() {
            let l = &*listener(slot);
            *out = (if l.used { l.generation } else { 0 }, 0);
        }
        for (slot, out) in connections.iter_mut().enumerate() {
            let c = &*connection(slot);
            if !c.used || c.id == 0 || c.generation == 0 {
                *out = (0, 0);
                continue;
            }
            let flags = wait_observation_word(slot) >> 48;
            *out = (u64::from(c.id) | (u64::from(c.generation) << 32), flags);
            if flags & 8 == 0 || c.listener_generation == 0 {
                continue;
            }
            for (index, observed) in listeners.iter_mut().enumerate() {
                if observed.0 == c.listener_generation && (*listener(index)).port == c.local_port {
                    observed.1 |= flags & 24;
                    break;
                }
            }
        }
    }
}
