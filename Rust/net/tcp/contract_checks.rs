//! Exact contract_checks operations of the parent owner; no duplicated state.
use super::*;

pub(crate) unsafe fn self_test_connect(remote_ip: [u8; 4], remote_port: u16) -> i32 {
    unsafe {
        let Some(slot) = alloc_connection() else {
            return -1;
        };
        let c = &mut *connection(slot);
        reset_connection_slot(c);
        c.used = true;
        c.id = *NEXT_ID.0.get();
        c.generation = next_slot_generation(slot);
        c.state = State::SynSent;
        c.local_port = *NEXT_PORT.0.get();
        c.remote_port = remote_port;
        c.remote_ip = remote_ip;
        c.seq = 0x1000 + *NEXT_ID.0.get();
        *NEXT_ID.0.get() = (*NEXT_ID.0.get()).wrapping_add(1);
        *NEXT_PORT.0.get() = (*NEXT_PORT.0.get()).wrapping_add(1);
        (*stats()).syn_tx += 1;
        (*stats()).synack_rx += 1;
        (*stats()).ack_tx += 1;
        c.ack = 0x2001;
        c.state = State::Established;
        i32::try_from(c.id).expect("TCP self connection id")
    }
}

pub(super) unsafe fn probe_connection(
    remote: [u8; 4],
    port: u16,
) -> Option<(ProbeEnd, *mut Connection)> {
    unsafe {
        let result = self_test_connect(remote, port);
        if result <= 0 {
            return None;
        }
        let id = result as u32;
        let end = ProbeEnd { id, abort: false };
        let c = by_id(id)?;
        Some((end, c))
    }
}

pub(super) unsafe fn probe_view(
    remote: [u8; 4],
    c: *const Connection,
    seq: u32,
    ack: u32,
    flags: u16,
    bytes: &[u8],
) -> SegmentView {
    unsafe {
        SegmentView {
            source_ip: remote,
            dest_ip: [192, 0, 2, 2],
            source_port: (*c).remote_port,
            dest_port: (*c).local_port,
            seq,
            ack,
            flags,
            window: MAX_ADVERTISED_WINDOW,
            payload: Text {
                pointer: bytes.as_ptr(),
                len: bytes.len(),
            },
            ..SegmentView::EMPTY
        }
    }
}

pub(crate) unsafe fn buffer_limit_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 90];
        let before = (*stats()).rx_drops;
        let Some((_end, c)) = probe_connection(remote, 65090) else {
            return false;
        };
        let chunk = [0x51; 512];
        let mut filled = 0;
        while filled < BUFFER_SIZE {
            let want = chunk.len().min(BUFFER_SIZE - filled);
            let copied = write_rx(&mut *c, &chunk[..want]);
            if copied != want {
                return false;
            }
            filled += copied;
        }
        if rx_free(&*c) != 0 {
            return false;
        }
        let view = probe_view(remote, c, (*c).ack, (*c).seq, FLAG_ACK | FLAG_PSH, b"X");
        if apply_rx_view(view).is_none() {
            return false;
        }
        if (*stats()).rx_drops != before + 1
            || (*c).rx_drops != 1
            || payload((*stats()).last_error) != b"rx-full"
        {
            return false;
        }

        // Only this owned probe connection is seeded while the TCP owner is
        // held. No early return may publish the synthetic near-limit value.
        (*c).rx_drops = u32::MAX - 1;
        let stale_reset = probe_view(remote, c, (*c).ack.wrapping_sub(1), (*c).seq, FLAG_RST, b"");
        let at_limit = apply_rx_view(stale_reset).is_some()
            && (*c).rx_drops == u32::MAX
            && payload((*stats()).last_error) == b"rst-stale";
        let stale_data = probe_view(
            remote,
            c,
            (*c).ack.wrapping_sub(1),
            (*c).seq,
            FLAG_ACK | FLAG_PSH,
            b"X",
        );
        let packet_held = apply_rx_view(stale_data).is_some()
            && (*c).rx_drops == u32::MAX
            && payload((*stats()).last_error) == b"seq-stale";
        let full = probe_view(remote, c, (*c).ack, (*c).seq, FLAG_ACK | FLAG_PSH, b"XYZ");
        let bytes_held = apply_rx_view(full).is_some()
            && (*c).rx_drops == u32::MAX
            && (*stats()).rx_drops == before + 6
            && payload((*stats()).last_error) == b"rx-full";
        // Retain only the real packet/byte accounting for later diagnostics.
        (*c).rx_drops = ((*stats()).rx_drops - before).min(u64::from(u32::MAX)) as u32;
        at_limit && packet_held && bytes_held
    }
}

pub(super) unsafe fn small_out_of_order_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 91];
        let before = (*stats()).rx_drops;
        let Some((end, c)) = probe_connection(remote, 65091) else {
            return false;
        };
        let base = (*c).ack;
        let third = probe_view(remote, c, base + 6, (*c).seq, FLAG_ACK | FLAG_PSH, b"GHI");
        if apply_rx_view(third).is_none() {
            return false;
        }
        if out_of_order_count(&*c) != 1 || (*c).rx_len != 0 || (*c).ack != base {
            return false;
        }
        let second = probe_view(remote, c, base + 3, (*c).seq, FLAG_ACK | FLAG_PSH, b"DEF");
        if apply_rx_view(second).is_none() {
            return false;
        }
        if out_of_order_count(&*c) != 2 || (*c).rx_len != 0 || (*c).ack != base {
            return false;
        }
        let first = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, b"ABC");
        if apply_rx_view(first).is_none() {
            return false;
        }
        let mut out = [0; 9];
        let got = read(end.id, &mut out);
        (*stats()).self_tests += 1;
        got == 9
            && out == *b"ABCDEFGHI"
            && out_of_order_count(&*c) == 0
            && (*stats()).rx_drops == before
    }
}

pub(super) fn fill_out_of_order_probe_payload(out: &mut [u8], segment: usize) {
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = ((segment * 17 + index) & 255) as u8;
    }
}

pub(super) fn verify_out_of_order_probe_payload(bytes: &[u8], segment: usize) -> bool {
    bytes
        .iter()
        .enumerate()
        .all(|(index, &byte)| byte == ((segment * 17 + index) & 255) as u8)
}

pub(super) unsafe fn burst_out_of_order_probe() -> bool {
    unsafe {
        const COUNT: usize = 40;
        const SIZE: usize = 1024;
        let remote = [198, 51, 100, 92];
        let before = (*stats()).rx_drops;
        let Some((end, c)) = probe_connection(remote, 65092) else {
            return false;
        };
        let base = (*c).ack;
        let mut bytes = [0; SIZE];
        let mut index = COUNT;
        while index > 1 {
            index -= 1;
            fill_out_of_order_probe_payload(&mut bytes, index);
            let view = probe_view(
                remote,
                c,
                base + (index * SIZE) as u32,
                (*c).seq,
                FLAG_ACK | FLAG_PSH,
                &bytes,
            );
            if apply_rx_view(view).is_none() {
                return false;
            }
        }
        if out_of_order_count(&*c) != COUNT - 1 || (*c).rx_len != 0 || (*c).ack != base {
            return false;
        }
        fill_out_of_order_probe_payload(&mut bytes, 0);
        let view = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, &bytes);
        if apply_rx_view(view).is_none() {
            return false;
        }
        if out_of_order_count(&*c) != 0 || (*c).rx_len != COUNT * SIZE {
            return false;
        }
        let mut out = [0; SIZE];
        for index in 0..COUNT {
            if read(end.id, &mut out) != SIZE as i32
                || !verify_out_of_order_probe_payload(&out, index)
            {
                return false;
            }
        }
        (*stats()).self_tests += 1;
        out_of_order_count(&*c) == 0 && (*c).rx_len == 0 && (*stats()).rx_drops == before
    }
}

pub(crate) unsafe fn out_of_order_probe() -> bool {
    unsafe { small_out_of_order_probe() && burst_out_of_order_probe() }
}

pub(crate) unsafe fn overlap_retransmit_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 92];
        let Some((_end, c)) = probe_connection(remote, 65092) else {
            return false;
        };
        let base = (*c).ack;
        let first = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, b"ABCDE");
        if apply_rx_view(first).is_none() || (*c).ack != base.wrapping_add(5) {
            return false;
        }
        let before = (*c).rx_bytes;
        let overlap = probe_view(remote, c, base, (*c).seq, FLAG_ACK | FLAG_PSH, b"ABCDEFGH");
        if apply_rx_view(overlap).is_none() {
            return false;
        }
        (*c).ack == base.wrapping_add(8) && (*c).rx_bytes == before + 3
    }
}

pub(crate) unsafe fn wraparound_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 93];
        let Some((_end, c)) = probe_connection(remote, 65093) else {
            return false;
        };
        (*c).ack = 0xffff_fffe;
        let before = (*c).rx_bytes;
        let first = probe_view(remote, c, 0xffff_fffe, (*c).seq, FLAG_ACK | FLAG_PSH, b"AB");
        if apply_rx_view(first).is_none() || (*c).ack != 0 {
            return false;
        }
        let second = probe_view(remote, c, 0, (*c).seq, FLAG_ACK | FLAG_PSH, b"CD");
        if apply_rx_view(second).is_none() || (*c).ack != 2 || (*c).rx_bytes != before + 4 {
            return false;
        }
        let fin = probe_view(remote, c, 2, (*c).seq, FLAG_ACK | FLAG_FIN, b"");
        if apply_rx_view(fin).is_none() {
            return false;
        }
        (*c).ack == 3 && (*c).state == State::Closed
    }
}

pub(crate) unsafe fn graceful_close_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 94];
        let Some((end, c)) = probe_connection(remote, 65094) else {
            return false;
        };
        let id = end.id;
        if !commit_sent(id, FLAG_ACK | FLAG_PSH, b"last", 1)
            || !request_close(id)
            || close_ready(id)
            || send_allowance(id, 1) != 0
        {
            return false;
        }
        let ack = probe_view(remote, c, (*c).ack, (*c).seq, FLAG_ACK, b"");
        if apply_rx_view(ack).is_none() || !close_ready(id) {
            return false;
        }
        if !commit_sent(id, FLAG_ACK | FLAG_FIN, b"", 2) || !fin_waiting(id) || close_ready(id) {
            return false;
        }
        let ack = probe_view(remote, c, (*c).ack, (*c).seq, FLAG_ACK, b"");
        if apply_rx_view(ack).is_none() {
            return false;
        }
        connection_identity(id).is_none()
    }
}

pub(crate) unsafe fn performance_contract_probe() -> bool {
    unsafe {
        let remote = [198, 51, 100, 95];
        let initial = 0x6000u32;
        let remote_seq = 0x7000;
        let before_mss = (*stats()).mss_negotiated;
        let before_scale = (*stats()).window_scale_negotiated;
        let before_coalesced = (*stats()).ack_coalesced;
        let result = begin_live_connect(remote, 65095, initial);
        if result <= 0 {
            return false;
        }
        let id = result as u32;
        let _end = ProbeEnd { id, abort: true };
        let Some(syn) = plan_send(id, FLAG_SYN) else {
            return false;
        };
        if syn.rx_window != MAX_ADVERTISED_WINDOW
            || option_mss(syn.options) != LOCAL_MSS
            || option_window_scale(syn.options) != Some(LOCAL_WINDOW_SCALE)
        {
            return false;
        }
        if !commit_sent(id, FLAG_SYN, b"", 1) {
            return false;
        }
        let Some(c) = by_id(id) else {
            return false;
        };
        let mut syn_ack = probe_view(
            remote,
            c,
            remote_seq,
            initial.wrapping_add(1),
            FLAG_SYN | FLAG_ACK,
            b"",
        );
        syn_ack.mss = LOCAL_MSS;
        syn_ack.window_scale = 3;
        syn_ack.window_scale_present = true;
        if apply_rx_view(syn_ack).is_none()
            || !established(id)
            || (*c).peer_mss != LOCAL_MSS
            || !(*c).window_scale_active
        {
            return false;
        }
        if (*stats()).mss_negotiated != before_mss.wrapping_add(1)
            || (*stats()).window_scale_negotiated != before_scale.wrapping_add(1)
            || sanitized_peer_mss(0) != DEFAULT_PEER_MSS
            || sanitized_peer_mss(4096) != LOCAL_MSS
        {
            return false;
        }
        const REQUESTED: usize = 4068;
        let bytes = [0x5a; LOCAL_MSS as usize];
        let mut written = 0;
        let mut segments = 0;
        while written < REQUESTED {
            let allowed = send_allowance(id, REQUESTED - written);
            if allowed == 0 {
                return false;
            }
            let flags = FLAG_ACK
                | if written + allowed == REQUESTED {
                    FLAG_PSH
                } else {
                    0
                };
            if !commit_sent(id, flags, &bytes[..allowed], 2 + segments) {
                return false;
            }
            written += allowed;
            segments += 1;
        }
        if segments != 3 || outstanding_count(id) != 3 || outstanding_bytes(id) != REQUESTED {
            return false;
        }
        let partial = initial
            .wrapping_add(1)
            .wrapping_add(u32::from(LOCAL_MSS))
            .wrapping_add(100);
        let ack = probe_view(remote, c, (*c).ack, partial, FLAG_ACK, b"");
        if apply_rx_view(ack).is_none() {
            return false;
        }
        let Some(retransmit) = plan_retransmit(id, RetransmitKind::Data) else {
            return false;
        };
        if outstanding_count(id) != 2
            || outstanding_bytes(id) != REQUESTED - usize::from(LOCAL_MSS) - 100
            || retransmit.seq != partial
        {
            return false;
        }
        let mut zero = ack;
        zero.window = 0;
        if apply_rx_view(zero).is_none()
            || send_allowance(id, 1) != 0
            || write_block_reason(id) != WriteBlockReason::RemoteWindow
        {
            return false;
        }
        let previous = (*c).ack;
        let first = probe_view(remote, c, previous, partial, FLAG_ACK | FLAG_PSH, b"A");
        if apply_rx_view(first).is_none()
            || request_ack(id, previous, first, 100, 5) != AckRequest::Delayed
        {
            return false;
        }
        let Some(identity) = connection_identity(id) else {
            return false;
        };
        if delayed_ack_due(identity.slot, 104).is_some() {
            return false;
        }
        let previous = (*c).ack;
        let mut second = first;
        second.seq = previous;
        second.payload = Text::literal(b"B");
        if apply_rx_view(second).is_none()
            || request_ack(id, previous, second, 101, 5) != AckRequest::Immediate
        {
            return false;
        }
        if delayed_ack_due(identity.slot, 101) != Some(id)
            || (*stats()).ack_coalesced != before_coalesced.wrapping_add(1)
        {
            return false;
        }
        let Some(plan) = plan_send(id, FLAG_ACK) else {
            return false;
        };
        note_ack_sent(id, false, plan.rx_window);
        if delayed_ack_due(identity.slot, 200).is_some() {
            return false;
        }
        (*stats()).self_tests = (*stats()).self_tests.wrapping_add(1);
        true
    }
}
