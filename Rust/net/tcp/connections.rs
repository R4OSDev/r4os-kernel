//! Exact connections operations of the parent owner; no duplicated state.
use super::*;

pub(crate) unsafe fn begin_live_connect(
    remote_ip: [u8; 4],
    remote_port: u16,
    initial_seq: u32,
) -> i32 {
    unsafe {
        let Some(slot) = alloc_connection() else {
            (*stats()).last_error = text(b"no-conn");
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
        c.seq = initial_seq;
        c.window_scale_offered = true;
        *NEXT_ID.0.get() = (*NEXT_ID.0.get()).wrapping_add(1);
        if *NEXT_ID.0.get() == 0 {
            *NEXT_ID.0.get() = 1;
        }
        *NEXT_PORT.0.get() = (*NEXT_PORT.0.get()).wrapping_add(1);
        if *NEXT_PORT.0.get() < 49152 {
            *NEXT_PORT.0.get() = 49152;
        }
        (*stats()).last_error = text(b"syn-sent");
        i32::try_from(c.id).expect("TCP live connection id")
    }
}

pub(crate) unsafe fn has_listener(port: u16) -> bool {
    unsafe {
        (0..MAX_LISTENERS).any(|slot| (*listener(slot)).used && (*listener(slot)).port == port)
    }
}

pub(crate) unsafe fn listener_identity(port: u16) -> Option<ListenerIdentity> {
    unsafe {
        (0..MAX_LISTENERS).find_map(|slot| {
            let l = &*listener(slot);
            (l.used && l.port == port).then_some(ListenerIdentity {
                slot,
                port,
                generation: l.generation,
            })
        })
    }
}

pub(crate) fn query_listener(identity: ListenerIdentity, data: bool) -> Result<Option<u32>, ()> {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return Err(());
    };
    unsafe {
        if listener_identity(identity.port) != Some(identity) {
            return Err(());
        }
        Ok(if data {
            connection_with_data_on_port(identity.port)
        } else {
            connection_on_port(identity.port)
        })
    }
}

pub(crate) fn finish_listener(identity: ListenerIdentity, abort: bool) {
    let Some(_guard) = super::super::tcp_owner::Guard::acquire() else {
        return;
    };
    unsafe {
        if listener_identity(identity.port) == Some(identity) {
            close_listener(identity.port);
        }
        if abort {
            for slot in 0..MAX_CONNECTIONS {
                let c = &mut *connection(slot);
                if c.used
                    && c.local_port == identity.port
                    && c.listener_generation == identity.generation
                {
                    reset_connection_slot(c);
                    (*stats()).last_error = text(b"listen-timeout");
                }
            }
        }
    }
}

pub(crate) unsafe fn listen(port: u16) -> bool {
    unsafe {
        if port == 0 {
            return false;
        }
        if has_listener(port) {
            return true;
        }
        for slot in 0..MAX_LISTENERS {
            let l = &mut *listener(slot);
            if l.used {
                continue;
            }
            let generation = *NEXT_LISTENER_GENERATION.0.get();
            if generation == 0 {
                (*stats()).last_error = text(b"listen-generation");
                return false;
            }
            *l = Listener {
                used: true,
                port,
                generation,
            };
            *NEXT_LISTENER_GENERATION.0.get() = generation.checked_add(1).unwrap_or(0);
            (*stats()).last_error = text(b"listen");
            return true;
        }
        (*stats()).last_error = text(b"listen-full");
        false
    }
}

pub(crate) unsafe fn close_listener(port: u16) {
    unsafe {
        for slot in 0..MAX_LISTENERS {
            let l = &mut *listener(slot);
            if l.used && l.port == port {
                *l = Listener::EMPTY;
                (*stats()).last_error = text(b"listen-close");
                return;
            }
        }
    }
}

pub(crate) unsafe fn accept_inbound(view: SegmentView, initial_seq: u32) -> Option<u32> {
    unsafe {
        let listener = listener_identity(view.dest_port)?;
        if view.flags & FLAG_SYN == 0 || view.flags & FLAG_ACK != 0 {
            return None;
        }
        if let Some(c) = match_connection(view) {
            if (*c).state == State::SynReceived && (*c).listener_generation == listener.generation {
                return Some((*c).id);
            }
            reset_connection_slot(&mut *c);
            (*stats()).last_error = text(b"syn-reuse");
        }
        let Some(slot) = alloc_connection() else {
            (*stats()).last_error = text(b"accept-full");
            return None;
        };
        let c = &mut *connection(slot);
        reset_connection_slot(c);
        c.used = true;
        c.id = *NEXT_ID.0.get();
        c.generation = next_slot_generation(slot);
        c.state = State::SynReceived;
        c.listener_generation = listener.generation;
        c.local_port = view.dest_port;
        c.remote_port = view.source_port;
        c.remote_ip = view.source_ip;
        c.seq = initial_seq;
        c.ack = view.seq.wrapping_add(1);
        c.tx_window = u32::from(view.window);
        c.peer_mss = sanitized_peer_mss(view.mss);
        c.peer_window_scale = if view.window_scale_present {
            view.window_scale.min(MAX_WINDOW_SCALE)
        } else {
            0
        };
        c.window_scale_offered = view.window_scale_present;
        c.window_scale_received = view.window_scale_present;
        c.window_scale_active = view.window_scale_present;
        *NEXT_ID.0.get() = (*NEXT_ID.0.get()).wrapping_add(1);
        if *NEXT_ID.0.get() == 0 {
            *NEXT_ID.0.get() = 1;
        }
        (*stats()).listen_syn_rx += 1;
        (*stats()).last_error = text(b"syn-rx");
        Some(c.id)
    }
}

pub(crate) unsafe fn abort(conn_id: u32, reason: Text) {
    unsafe {
        if let Some(c) = by_id(conn_id) {
            discard_catalog(&mut *c);
            (*c).state = State::Closed;
            (*c).used = false;
            (*stats()).last_error = reason;
        }
    }
}

pub(crate) unsafe fn mark_timeout(reason: Text) {
    unsafe {
        (*stats()).timeouts += 1;
        (*stats()).last_error = reason;
    }
}

pub(crate) unsafe fn record_malformed(reason: Text) {
    unsafe {
        (*stats()).malformed += 1;
        (*stats()).last_error = reason;
    }
}

pub(crate) unsafe fn record_checksum_error() {
    unsafe {
        (*stats()).checksum_errors += 1;
        (*stats()).last_error = text(b"checksum");
    }
}

pub(crate) unsafe fn set_error(reason: Text) {
    unsafe {
        (*stats()).last_error = reason;
    }
}

pub(crate) unsafe fn established(id: u32) -> bool {
    unsafe { by_id(id).is_some_and(|c| (*c).state == State::Established) }
}

pub(crate) unsafe fn request_close(id: u32) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        match (*c).state {
            State::Established => {
                (*c).close_requested = true;
                (*stats()).last_error = text(b"close-pending");
                true
            }
            State::FinWait | State::LastAck | State::TimeWait | State::Closed => true,
            _ => false,
        }
    }
}

pub(crate) unsafe fn close_ready(id: u32) -> bool {
    unsafe {
        by_id(id).is_some_and(|c| {
            (*c).state == State::Established && (*c).close_requested && (*c).sent.count() == 0
        })
    }
}

pub(crate) unsafe fn fin_waiting(id: u32) -> bool {
    unsafe { by_id(id).is_some_and(|c| (*c).state == State::FinWait) }
}

pub(crate) unsafe fn connection_identity(id: u32) -> Option<ConnectionIdentity> {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = &*connection(slot);
            if c.used && c.id == id && c.generation != 0 {
                return Some(ConnectionIdentity {
                    connection_id: c.id,
                    slot,
                    generation: c.generation,
                });
            }
        }
        None
    }
}

pub(crate) unsafe fn connection_identity_at(slot: usize) -> Option<ConnectionIdentity> {
    if slot >= MAX_CONNECTIONS {
        return None;
    }
    unsafe {
        let c = &*connection(slot);
        if !c.used || c.id == 0 || c.generation == 0 {
            None
        } else {
            Some(ConnectionIdentity {
                connection_id: c.id,
                slot,
                generation: c.generation,
            })
        }
    }
}

pub(crate) unsafe fn identity_active(identity: ConnectionIdentity) -> bool {
    unsafe { connection_identity_at(identity.slot).is_some_and(|current| current.eql(identity)) }
}

pub(crate) unsafe fn connection_with_data_on_port(port: u16) -> Option<u32> {
    unsafe {
        let listener = listener_identity(port)?;
        for slot in 0..MAX_CONNECTIONS {
            let c = &*connection(slot);
            if c.used
                && c.state == State::Established
                && c.local_port == port
                && c.listener_generation == listener.generation
                && !c.accepted_claimed
                && c.rx_len != 0
            {
                return Some(c.id);
            }
        }
        None
    }
}

pub(crate) unsafe fn connection_on_port(port: u16) -> Option<u32> {
    unsafe {
        let listener = listener_identity(port)?;
        for slot in 0..MAX_CONNECTIONS {
            let c = &*connection(slot);
            if c.used
                && c.state == State::Established
                && c.local_port == port
                && c.listener_generation == listener.generation
                && !c.accepted_claimed
            {
                return Some(c.id);
            }
        }
        None
    }
}

pub(crate) unsafe fn claim_accepted(id: u32) -> bool {
    unsafe {
        let Some(c) = by_id(id) else {
            return false;
        };
        if (*c).accepted_claimed {
            return false;
        }
        (*c).accepted_claimed = true;
        true
    }
}

pub(crate) unsafe fn connection_id_for_segment(view: SegmentView) -> Option<u32> {
    unsafe { match_connection(view).map(|c| (*c).id) }
}

pub(crate) unsafe fn connection_identity_for_segment(
    view: SegmentView,
) -> Option<ConnectionIdentity> {
    unsafe { connection_identity(connection_id_for_segment(view)?) }
}

pub(crate) unsafe fn abort_connections_on_port(port: u16, reason: Text) {
    unsafe {
        for slot in 0..MAX_CONNECTIONS {
            let c = &mut *connection(slot);
            if c.used && c.local_port == port {
                reset_connection_slot(c);
                (*stats()).last_error = reason;
            }
        }
    }
}

pub(crate) unsafe fn abort_all(reason: Text) -> CleanupResult {
    unsafe {
        let mut result = CleanupResult::EMPTY;
        for slot in 0..MAX_CONNECTIONS {
            let c = &mut *connection(slot);
            if c.used {
                reset_connection_slot(c);
                result.connections += 1;
            }
        }
        for slot in 0..MAX_LISTENERS {
            let l = &mut *listener(slot);
            if l.used {
                *l = Listener::EMPTY;
                result.listeners += 1;
            }
        }
        if result.connections != 0 || result.listeners != 0 {
            (*stats()).last_error = reason;
        }
        result
    }
}
