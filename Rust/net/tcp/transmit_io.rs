//! Exact transmit_io operations of the parent owner; no duplicated state.
use super::*;

pub(crate) unsafe fn plan_send(id: u32, flags: u16) -> Option<SendPlan> {
    unsafe {
        let c = &*by_id(id)?;
        Some(SendPlan {
            local_port: c.local_port,
            remote_port: c.remote_port,
            remote_ip: c.remote_ip,
            seq: c.seq,
            ack: c.ack,
            rx_window: advertised_window_for_flags(c, flags),
            options: options_for_flags(c, flags),
        })
    }
}

pub(crate) unsafe fn send_allowance(id: u32, requested: usize) -> usize {
    unsafe {
        let Some(c) = by_id(id) else {
            return 0;
        };
        let c = &*c;
        if c.state != State::Established || c.close_requested {
            return 0;
        }
        tcp_runtime::send_allowance(
            c.tx_window,
            requested.min(usize::from(c.peer_mss)),
            c.sent.has_capacity(),
        )
    }
}

pub(crate) unsafe fn write_block_reason(id: u32) -> WriteBlockReason {
    unsafe {
        let Some(c) = by_id(id) else {
            return WriteBlockReason::State;
        };
        let c = &*c;
        if c.state != State::Established || c.close_requested {
            WriteBlockReason::State
        } else if c.tx_window == 0 {
            WriteBlockReason::RemoteWindow
        } else if !c.sent.has_capacity() {
            WriteBlockReason::Catalog
        } else {
            WriteBlockReason::None
        }
    }
}

pub(crate) unsafe fn can_track_send(id: u32, flags: u16, len: usize) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &*c;
        let ack_only = flags == FLAG_ACK && len == 0;
        if c.state == State::Closed && !ack_only {
            return false;
        }
        !tcp_runtime::needs_tracking(flags, len)
            || (len <= SENT_PAYLOAD_CAPACITY && c.sent.has_capacity())
    }
}

pub(crate) unsafe fn plan_retransmit(id: u32, kind: RetransmitKind) -> Option<RetransmitPlan> {
    unsafe {
        let c = &*by_id(id)?;
        if c.state == State::Closed {
            return None;
        }
        let segment = c.sent.oldest(kind)?;
        let bytes = segment.payload_slice();
        let mut plan = RetransmitPlan {
            local_port: c.local_port,
            remote_port: c.remote_port,
            remote_ip: c.remote_ip,
            seq: segment.seq,
            ack: segment.ack,
            flags: segment.flags,
            rx_window: advertised_window_for_flags(c, segment.flags),
            options: options_for_flags(c, segment.flags),
            payload_storage: [0; SENT_PAYLOAD_CAPACITY],
            payload_len: bytes.len() as u16,
            token: segment.token,
            sent_tick: segment.sent_tick,
            retransmits: segment.retransmits,
        };
        plan.payload_storage[..bytes.len()].copy_from_slice(bytes);
        Some(plan)
    }
}

pub(crate) unsafe fn outstanding_info(id: u32, kind: RetransmitKind) -> Option<OutstandingInfo> {
    unsafe {
        let c = &*by_id(id)?;
        let s = c.sent.oldest(kind)?;
        Some(OutstandingInfo {
            sent_tick: s.sent_tick,
            retransmits: s.retransmits,
        })
    }
}

pub(crate) unsafe fn outstanding_count(id: u32) -> usize {
    unsafe { by_id(id).map_or(0, |c| (*c).sent.count()) }
}

pub(crate) unsafe fn outstanding_bytes(id: u32) -> usize {
    unsafe { by_id(id).map_or(0, |c| (*c).sent.outstanding_bytes()) }
}

pub(super) unsafe fn remember_sent(
    c: &mut Connection,
    flags: u16,
    bytes: &[u8],
    tick: u64,
) -> bool {
    if !tcp_runtime::needs_tracking(flags, bytes.len()) {
        return true;
    }
    if c.sent.track(c.seq, c.ack, flags, bytes, tick).is_none() {
        return false;
    }
    unsafe { note_catalog_added(bytes.len()) };
    c.last_seq = c.seq;
    c.last_ack = c.ack;
    c.last_flags = flags;
    c.last_len = bytes.len();
    true
}

pub(crate) unsafe fn commit_sent(id: u32, flags: u16, bytes: &[u8], tick: u64) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &mut *c;
        if !remember_sent(c, flags, bytes, tick) {
            return false;
        }
        if flags & FLAG_SYN != 0 {
            if flags & FLAG_ACK != 0 {
                (*stats()).synack_tx += 1;
                (*stats()).last_error = text(b"synack-tx");
            } else {
                (*stats()).syn_tx += 1;
                (*stats()).last_error = text(b"syn");
            }
            return true;
        }
        if flags & FLAG_ACK != 0 {
            (*stats()).ack_tx += 1;
            if bytes.is_empty() && flags & (FLAG_SYN | FLAG_FIN | FLAG_RST) == 0 {
                (*stats()).pure_ack_tx += 1;
            }
        }
        if !bytes.is_empty() {
            c.seq = c
                .seq
                .wrapping_add(u32::try_from(bytes.len()).expect("TCP payload"));
            c.tx_window = c.tx_window.saturating_sub(bytes.len() as u32);
            c.tx_bytes += bytes.len() as u64;
            (*stats()).data_tx += 1;
            (*stats()).last_error = text(b"data");
        }
        if flags & FLAG_FIN != 0 {
            c.seq = c.seq.wrapping_add(1);
            c.close_requested = false;
            c.state = State::FinWait;
            (*stats()).fin_tx += 1;
            (*stats()).last_error = text(b"fin");
        }
        true
    }
}

pub(crate) unsafe fn record_retransmit(id: u32, token: u64, tick: u64, reason: Text) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &mut *c;
        if !c.sent.mark_retransmitted(token, tick) {
            return false;
        }
        c.retransmits += 1;
        (*stats()).retransmits += 1;
        (*stats()).last_error = reason;
        true
    }
}

pub(crate) unsafe fn tx_ack_of(id: u32) -> u32 {
    unsafe { by_id(id).map_or(0, |c| (*c).tx_ack) }
}

pub(crate) unsafe fn rx_ack_of(id: u32) -> u32 {
    unsafe { by_id(id).map_or(0, |c| (*c).ack) }
}

pub(crate) unsafe fn request_ack(
    id: u32,
    previous: u32,
    view: SegmentView,
    now: u64,
    delay: u64,
) -> AckRequest {
    unsafe {
        let Some(c) = by_id(id) else {
            return AckRequest::None;
        };
        let c = &mut *c;
        if view.payload.len == 0 && view.flags & FLAG_FIN == 0 {
            return AckRequest::None;
        }
        let immediate = view.flags & FLAG_FIN != 0
            || c.ack == previous
            || view.seq != previous
            || rx_free(c) == 0;
        if c.delayed_ack_pending {
            c.delayed_ack_segments = c.delayed_ack_segments.saturating_add(1);
            c.delayed_ack_immediate = true;
            c.delayed_ack_deadline = now;
            (*stats()).ack_coalesced = (*stats()).ack_coalesced.wrapping_add(1);
            return AckRequest::Immediate;
        }
        c.delayed_ack_pending = true;
        c.delayed_ack_segments = 1;
        c.delayed_ack_immediate = immediate;
        c.delayed_ack_deadline = if immediate {
            now
        } else {
            now.saturating_add(delay.max(1))
        };
        if immediate {
            return AckRequest::Immediate;
        }
        (*stats()).delayed_ack_requests = (*stats()).delayed_ack_requests.wrapping_add(1);
        AckRequest::Delayed
    }
}

pub(crate) unsafe fn delayed_ack_due(slot: usize, now: u64) -> Option<u32> {
    if slot >= MAX_CONNECTIONS {
        return None;
    }
    unsafe {
        let c = &*connection(slot);
        if !c.used
            || !c.delayed_ack_pending
            || (!c.delayed_ack_immediate
                && !tcp_runtime::deadline_reached(now, c.delayed_ack_deadline))
        {
            None
        } else {
            Some(c.id)
        }
    }
}

pub(crate) unsafe fn note_ack_sent(id: u32, piggybacked: bool, window: u16) {
    unsafe {
        let Some(c) = by_id(id) else {
            return;
        };
        let c = &mut *c;
        c.last_advertised_window = window;
        if c.window_update_pending {
            c.window_update_pending = false;
            (*stats()).window_update_tx = (*stats()).window_update_tx.wrapping_add(1);
        }
        if !c.delayed_ack_pending {
            return;
        }
        if piggybacked {
            (*stats()).ack_piggybacked = (*stats()).ack_piggybacked.wrapping_add(1);
        } else if c.delayed_ack_immediate {
            (*stats()).immediate_ack_tx = (*stats()).immediate_ack_tx.wrapping_add(1);
        } else {
            (*stats()).delayed_ack_tx = (*stats()).delayed_ack_tx.wrapping_add(1);
        }
        c.delayed_ack_pending = false;
        c.delayed_ack_immediate = false;
        c.delayed_ack_segments = 0;
        c.delayed_ack_deadline = 0;
    }
}

pub(crate) unsafe fn window_update_required(id: u32) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        let c = &mut *c;
        if c.delayed_ack_pending {
            return true;
        }
        let current = advertised_window(c);
        if current <= c.last_advertised_window {
            return false;
        }
        let shift = if c.window_scale_active {
            LOCAL_WINDOW_SCALE
        } else {
            0
        };
        let wire_mss = 1.max(LOCAL_MSS >> shift);
        if c.last_advertised_window != 0 && current - c.last_advertised_window < wire_mss {
            return false;
        }
        c.window_update_pending = true;
        true
    }
}

pub(crate) unsafe fn readable(id: u32) -> bool {
    unsafe { by_id(id).is_none_or(|c| (*c).rx_len != 0 || (*c).state == State::Closed) }
}

pub(crate) unsafe fn closed(id: u32) -> bool {
    unsafe {
        by_id(id)
            .is_none_or(|c| matches!((*c).state, State::Closed | State::TimeWait | State::LastAck))
    }
}

pub(crate) unsafe fn loopback_write(id: u32, bytes: *const u8, len: usize) -> i32 {
    unsafe { loopback_write_foreign(id, bytes, len) }
}

pub(crate) unsafe fn read(id: u32, out: &mut [u8]) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        let c = &mut *c;
        let len = out.len().min(c.rx_len);
        if len != 0 {
            let mut copied = 0;
            while copied < len {
                let run = (len - copied).min(c.rx.len() - c.rx_head);
                out[copied..copied + run].copy_from_slice(&c.rx[c.rx_head..c.rx_head + run]);
                c.rx_head = (c.rx_head + run) % c.rx.len();
                c.rx_len -= run;
                copied += run;
            }
            if c.rx_len == 0 {
                c.rx_head = 0;
            }
            flush_out_of_order_payload(c);
        } else if c.state == State::Closed {
            discard_catalog(c);
            c.used = false;
            return -1;
        }
        len as i32
    }
}

pub(crate) unsafe fn close(id: u32) -> i32 {
    unsafe {
        let Some(c) = by_id(id) else {
            return -1;
        };
        let c = &mut *c;
        if c.state == State::Closed {
            discard_catalog(c);
            c.used = false;
            return 0;
        }
        if c.state != State::FinWait {
            c.state = State::FinWait;
            (*stats()).fin_tx += 1;
        }
        c.state = State::Closed;
        discard_catalog(c);
        c.used = false;
        0
    }
}
