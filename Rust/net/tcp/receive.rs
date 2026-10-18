//! Exact receive operations of the parent owner; no duplicated state.
use super::*;

pub(super) unsafe fn record_view(view: SegmentView) {
    unsafe {
        let s = &mut *stats();
        s.last_flags = view.flags;
        s.last_source_port = view.source_port;
        s.last_dest_port = view.dest_port;
        s.last_seq = view.seq;
        s.last_ack = view.ack;
        s.last_payload_len = u32::try_from(view.payload.len).expect("TCP receive payload");
        s.last_error = text(b"none");
    }
}

pub(crate) unsafe fn apply_rx_view(view: SegmentView) -> Option<SegmentView> {
    unsafe {
        (*stats()).rx_segments += 1;
        record_view(view);
        if view.flags & FLAG_RST != 0 {
            (*stats()).rst_rx += 1;
            if let Some(c) = match_connection(view) {
                let c = &mut *c;
                if !reset_seq_acceptable(c, view) {
                    c.rx_drops = c.rx_drops.saturating_add(1);
                    (*stats()).rx_drops += 1;
                    (*stats()).last_error = text(b"rst-stale");
                    return Some(view);
                }
                discard_catalog(c);
                c.state = State::Closed;
                c.used = false;
            }
            (*stats()).last_error = text(b"rst");
            return Some(view);
        }
        if let Some(c) = match_connection(view) {
            let c = &mut *c;
            if c.state == State::SynSent
                && view.flags & (FLAG_SYN | FLAG_ACK) == FLAG_SYN | FLAG_ACK
                && view.ack == c.seq.wrapping_add(1)
            {
                apply_peer_handshake_options(c, view);
                c.seq = c.seq.wrapping_add(1);
                c.ack = view.seq.wrapping_add(1);
                c.state = State::Established;
                update_tx_window(c, view);
                count_negotiation(c);
                (*stats()).synack_rx += 1;
                (*stats()).last_error = text(b"synack");
            } else if c.state == State::SynReceived
                && view.flags & FLAG_ACK != 0
                && view.ack == c.seq.wrapping_add(1)
            {
                c.seq = c.seq.wrapping_add(1);
                c.state = State::Established;
                update_tx_window(c, view);
                count_negotiation(c);
                (*stats()).accepts += 1;
                (*stats()).last_error = text(b"accepted");
            } else if c.state == State::Established {
                if (view.payload.len != 0 || view.flags & FLAG_FIN != 0)
                    && !segment_seq_acceptable(c, view)
                {
                    c.rx_drops = c.rx_drops.saturating_add(1);
                    (*stats()).rx_drops += 1;
                    (*stats()).last_error = text(b"seq-stale");
                    return Some(view);
                }
                update_tx_window(c, view);
                if view.payload.len != 0 {
                    accept_payload(c, view.seq, payload(view.payload));
                }
                // FIN consumes only the next exact in-order sequence number. The
                // original drain-then-close userland contract is retained.
                if view.flags & FLAG_FIN != 0
                    && view.seq.wrapping_add(view.payload.len as u32) == c.ack
                {
                    c.ack = c.ack.wrapping_add(1);
                    c.state = State::Closed;
                    (*stats()).last_error = text(b"fin-rx");
                }
            } else if c.state == State::FinWait && view.flags & FLAG_ACK != 0 {
                update_tx_window(c, view);
                if view.flags & FLAG_FIN != 0 && view.seq == c.ack {
                    c.ack = c.ack.wrapping_add(1);
                }
                c.state = State::Closed;
                if view.flags & FLAG_FIN == 0 {
                    discard_catalog(c);
                    c.used = false;
                }
                (*stats()).last_error = text(b"closed");
            }
        }
        Some(view)
    }
}

pub(crate) unsafe fn apply_tx_view(view: SegmentView) -> Option<SegmentView> {
    unsafe {
        record_view(view);
        (*stats()).tx_segments += 1;
        Some(view)
    }
}

pub(super) unsafe fn accept_payload(c: &mut Connection, seq: u32, bytes: &[u8]) {
    unsafe {
        if bytes.is_empty() {
            return;
        }
        if seq == c.ack {
            accept_in_order_payload(c, seq, bytes);
            flush_out_of_order_payload(c);
            return;
        }
        if tcp_runtime::seq_after(seq, c.ack) {
            store_out_of_order_payload(c, seq, bytes);
            return;
        }
        let overlap = c.ack.wrapping_sub(seq) as usize;
        if overlap < bytes.len() {
            accept_in_order_payload(c, c.ack, &bytes[overlap..]);
            flush_out_of_order_payload(c);
        } else {
            (*stats()).last_error = text(b"seq-dup");
        }
    }
}

pub(super) unsafe fn accept_in_order_payload(c: &mut Connection, seq: u32, bytes: &[u8]) {
    unsafe {
        let copied = write_rx(c, bytes);
        if copied < bytes.len() {
            let dropped = bytes.len() - copied;
            c.rx_drops = c
                .rx_drops
                .saturating_add(u32::try_from(dropped).expect("TCP drops"));
            (*stats()).rx_drops += dropped as u64;
        }
        c.rx_bytes += copied as u64;
        c.ack = seq.wrapping_add(copied as u32);
        if copied != 0 {
            (*stats()).data_rx += 1;
            (*stats()).last_error = text(if copied == bytes.len() {
                b"data-rx"
            } else {
                b"rx-full"
            });
        } else {
            (*stats()).last_error = text(b"rx-full");
        }
    }
}

pub(super) unsafe fn store_out_of_order_payload(c: &mut Connection, seq: u32, bytes: &[u8]) {
    unsafe {
        if bytes.len() > OUT_OF_ORDER_SLOT_SIZE {
            c.rx_drops = c
                .rx_drops
                .saturating_add(u32::try_from(bytes.len()).expect("TCP large OOO"));
            (*stats()).rx_drops += bytes.len() as u64;
            (*stats()).last_error = text(b"ooo-large");
            return;
        }
        let end = seq.wrapping_add(bytes.len() as u32);
        if !tcp_runtime::seq_after(end, c.ack) {
            (*stats()).last_error = text(b"seq-dup");
            return;
        }
        if let Some(index) = find_out_of_order_covering(c, seq, bytes.len()) {
            let slot = &mut c.out_of_order[index];
            if bytes.len() > slot.len {
                slot.payload[..bytes.len()].copy_from_slice(bytes);
                slot.len = bytes.len();
            }
            (*stats()).last_error = text(b"ooo-dup");
            return;
        }
        let Some(index) = find_out_of_order_free(c)
            .or_else(|| select_out_of_order_replacement(c, seq, bytes.len()))
        else {
            c.rx_drops = c.rx_drops.saturating_add(bytes.len() as u32);
            (*stats()).rx_drops += bytes.len() as u64;
            (*stats()).last_error = text(b"ooo-full");
            return;
        };
        let slot = &mut c.out_of_order[index];
        if slot.valid {
            c.rx_drops = c.rx_drops.saturating_add(slot.len as u32);
            (*stats()).rx_drops += slot.len as u64;
        }
        slot.payload[..bytes.len()].copy_from_slice(bytes);
        slot.seq = seq;
        slot.len = bytes.len();
        slot.valid = true;
        (*stats()).last_error = text(b"ooo-store");
    }
}

pub(super) unsafe fn flush_out_of_order_payload(c: &mut Connection) {
    unsafe {
        while let Some(index) = find_flushable_out_of_order(c) {
            let offset = c.ack.wrapping_sub(c.out_of_order[index].seq) as usize;
            if offset >= c.out_of_order[index].len {
                c.out_of_order[index] = OutOfOrderSegment::EMPTY;
                continue;
            }
            let remaining = c.out_of_order[index].len - offset;
            if rx_free(c) < remaining {
                (*stats()).last_error = text(b"ooo-wait");
                return;
            }
            let mut bytes = [0u8; OUT_OF_ORDER_SLOT_SIZE];
            bytes[..remaining]
                .copy_from_slice(&c.out_of_order[index].payload[offset..c.out_of_order[index].len]);
            let seq = c.ack;
            c.out_of_order[index] = OutOfOrderSegment::EMPTY;
            accept_in_order_payload(c, seq, &bytes[..remaining]);
            if (*stats()).last_error.len == 7 && payload((*stats()).last_error) == b"data-rx" {
                (*stats()).last_error = text(b"ooo-flush");
            }
        }
    }
}

pub(super) fn find_out_of_order_free(c: &Connection) -> Option<usize> {
    c.out_of_order.iter().position(|slot| !slot.valid)
}

pub(super) fn find_out_of_order_covering(c: &Connection, seq: u32, len: usize) -> Option<usize> {
    let end = seq.wrapping_add(len as u32);
    c.out_of_order.iter().position(|slot| {
        if !slot.valid {
            return false;
        }
        let slot_end = slot.seq.wrapping_add(slot.len as u32);
        let seq_covered = slot.seq == seq
            || (!tcp_runtime::seq_after(slot.seq, seq) && tcp_runtime::seq_after(slot_end, seq));
        let end_covered = slot_end == end || tcp_runtime::seq_after(slot_end, end);
        seq_covered && end_covered
    })
}

pub(super) fn select_out_of_order_replacement(
    c: &Connection,
    seq: u32,
    _len: usize,
) -> Option<usize> {
    let distance = seq.wrapping_sub(c.ack);
    let mut farthest = None;
    let mut farthest_distance = 0;
    for (index, slot) in c.out_of_order.iter().enumerate() {
        if !slot.valid {
            continue;
        }
        let current = slot.seq.wrapping_sub(c.ack);
        if farthest.is_none() || current > farthest_distance {
            farthest = Some(index);
            farthest_distance = current;
        }
    }
    if distance < farthest_distance {
        farthest
    } else {
        None
    }
}

pub(super) fn find_flushable_out_of_order(c: &Connection) -> Option<usize> {
    let mut best = None;
    let mut best_offset = 0;
    for (index, slot) in c.out_of_order.iter().enumerate() {
        if !slot.valid {
            continue;
        }
        let end = slot.seq.wrapping_add(slot.len as u32);
        let at_ack = slot.seq == c.ack;
        let overlaps =
            !tcp_runtime::seq_after(slot.seq, c.ack) && tcp_runtime::seq_after(end, c.ack);
        if !at_ack && !overlaps {
            continue;
        }
        let offset = c.ack.wrapping_sub(slot.seq);
        if best.is_none() || offset < best_offset {
            best = Some(index);
            best_offset = offset;
        }
    }
    best
}

pub(super) fn out_of_order_count(c: &Connection) -> usize {
    c.out_of_order.iter().filter(|slot| slot.valid).count()
}

pub(super) unsafe fn update_tx_window(c: &mut Connection, view: SegmentView) {
    unsafe {
        let shift = if c.window_scale_active && view.flags & FLAG_SYN == 0 {
            c.peer_window_scale.min(MAX_WINDOW_SCALE)
        } else {
            0
        };
        let advertised = u32::from(view.window) << shift;
        if view.flags & FLAG_ACK == 0 {
            c.tx_window = advertised;
            return;
        }
        if c.tx_ack == 0 || tcp_runtime::seq_after(view.ack, c.tx_ack) {
            c.tx_ack = view.ack;
        }
        let segments = c.sent.count();
        let bytes = c.sent.outstanding_bytes();
        c.sent.acknowledge(view.ack);
        note_catalog_reduced(segments, bytes, c.sent.count(), c.sent.outstanding_bytes());
        let right_edge = view.ack.wrapping_add(advertised);
        c.tx_window = if tcp_runtime::seq_after(right_edge, c.seq) {
            right_edge.wrapping_sub(c.seq)
        } else {
            0
        };
    }
}

pub(super) fn write_rx(c: &mut Connection, bytes: &[u8]) -> usize {
    let len = bytes.len().min(rx_free(c));
    let mut copied = 0;
    while copied < len {
        let tail = (c.rx_head + c.rx_len) % c.rx.len();
        let run = (len - copied).min(c.rx.len() - tail);
        c.rx[tail..tail + run].copy_from_slice(&bytes[copied..copied + run]);
        c.rx_len += run;
        copied += run;
    }
    len
}

pub(super) fn rx_free(c: &Connection) -> usize {
    c.rx.len() - c.rx_len
}

pub(super) fn reset_seq_acceptable(c: &Connection, view: SegmentView) -> bool {
    match c.state {
        State::SynSent => view.flags & FLAG_ACK != 0 && view.ack == c.seq.wrapping_add(1),
        State::SynReceived
        | State::Established
        | State::FinWait
        | State::LastAck
        | State::TimeWait => segment_seq_acceptable(c, view),
        State::Closed => false,
    }
}

pub(super) fn segment_seq_acceptable(c: &Connection, view: SegmentView) -> bool {
    if view.seq == c.ack {
        return true;
    }
    if view.payload.len != 0 {
        let end = view.seq.wrapping_add(view.payload.len as u32);
        if !tcp_runtime::seq_after(view.seq, c.ack) && tcp_runtime::seq_after(end, c.ack) {
            return true;
        }
    }
    view.seq.wrapping_sub(c.ack) < rx_free(c).max(1) as u32
}

pub(crate) unsafe fn reap_time_wait(now: u64) -> u32 {
    unsafe {
        let mut freed = 0;
        for slot in 0..MAX_CONNECTIONS {
            let c = &mut *connection(slot);
            if !c.used {
                continue;
            }
            if c.state == State::Closed && c.rx_len == 0 {
                if c.time_wait_until == 0 {
                    c.time_wait_until = now.wrapping_add(TIME_WAIT_TICKS);
                } else if tcp_runtime::deadline_reached(now, c.time_wait_until) {
                    reset_connection_slot(c);
                    freed += 1;
                }
                continue;
            }
            if c.state != State::FinWait && !(c.state == State::Established && c.close_requested) {
                continue;
            }
            if c.time_wait_until == 0 {
                c.time_wait_until = now.wrapping_add(TIME_WAIT_TICKS);
                continue;
            }
            if tcp_runtime::deadline_reached(now, c.time_wait_until) {
                reset_connection_slot(c);
                freed += 1;
            }
        }
        freed
    }
}

pub(super) fn advertised_window(c: &Connection) -> u16 {
    let free = rx_free(c);
    let scaled = if c.window_scale_active {
        free >> LOCAL_WINDOW_SCALE
    } else {
        free
    };
    scaled.min(usize::from(MAX_ADVERTISED_WINDOW)) as u16
}

pub(super) fn advertised_window_for_flags(c: &Connection, flags: u16) -> u16 {
    if flags & FLAG_SYN != 0 {
        rx_free(c).min(usize::from(MAX_ADVERTISED_WINDOW)) as u16
    } else {
        advertised_window(c)
    }
}

pub(super) fn sanitized_peer_mss(value: u16) -> u16 {
    if value == 0 {
        DEFAULT_PEER_MSS
    } else {
        LOCAL_MSS.min(value)
    }
}

pub(super) fn apply_peer_handshake_options(c: &mut Connection, view: SegmentView) {
    c.peer_mss = sanitized_peer_mss(view.mss);
    c.window_scale_received = view.window_scale_present;
    c.peer_window_scale = if view.window_scale_present {
        view.window_scale.min(MAX_WINDOW_SCALE)
    } else {
        0
    };
    c.window_scale_active = c.window_scale_offered && c.window_scale_received;
}

pub(super) unsafe fn count_negotiation(c: &mut Connection) {
    if c.negotiation_counted {
        return;
    }
    c.negotiation_counted = true;
    unsafe {
        if c.peer_mss != DEFAULT_PEER_MSS {
            (*stats()).mss_negotiated += 1;
        }
        if c.window_scale_active {
            (*stats()).window_scale_negotiated += 1;
        }
    }
}

pub(super) fn options_for_flags(c: &Connection, flags: u16) -> u32 {
    if flags & FLAG_SYN == 0 {
        0
    } else {
        option_metadata(
            LOCAL_MSS,
            if c.window_scale_offered {
                Some(LOCAL_WINDOW_SCALE)
            } else {
                None
            },
        )
    }
}
