//! Original connection-generation SRTT/backoff and proactive retry selection.
//! RTO metadata shares the native TCP task owner. It releases that owner before
//! sending a packet, which may resolve ARP, execute R4P or wait for a backend.
use super::{
    tcp,
    tcp_owner::Guard,
    tcp_runtime::{ConnectionIdentity, RtoTable, SegmentKind},
    timing,
};
use core::cell::UnsafeCell;
const MIN_TICKS: u64 = timing::ms_to_ticks(50);
const MAX_TICKS: u64 = timing::ms_to_ticks(1500);
const PROACTIVE_MAX_RESEND: u8 = 5;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RtoStatus {
    pub proactive_retransmits: u64,
    pub samples: u64,
    pub last_ticks: u64,
}
impl RtoStatus {
    const EMPTY: Self = Self {
        proactive_retransmits: 0,
        samples: 0,
        last_ticks: 0,
    };
}
struct State {
    table: RtoTable,
    status: RtoStatus,
}
struct Storage(UnsafeCell<State>);
// SAFETY: all access holds the same native TCP task owner. No state/table
// reference or owner token spans a network callback or foreign output write.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    table: RtoTable::EMPTY,
    status: RtoStatus::EMPTY,
}));
fn state() -> *mut State {
    STATE.0.get()
}
fn from_srtt(srtt: u64) -> u64 {
    if srtt == 0 {
        MAX_TICKS
    } else {
        (srtt * 2).clamp(MIN_TICKS, MAX_TICKS)
    }
}
fn backed_off(mut ticks: u64, retransmits: u8) -> u64 {
    for _ in 0..retransmits {
        ticks *= 2;
        if ticks >= MAX_TICKS {
            return MAX_TICKS;
        }
    }
    ticks
}
unsafe fn for_conn(id: u32) -> u64 {
    unsafe {
        let Some(identity) = tcp::connection_identity(id) else {
            return MAX_TICKS;
        };
        let Some(st) = (*state()).table.bind(identity) else {
            return MAX_TICKS;
        };
        from_srtt(st.srtt_ticks)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_rto_reset(full: bool) {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        (*state()).table.reset();
        if full {
            (*state()).status = RtoStatus::EMPTY;
        }
    }
}
/// # Safety
/// Exact readable connection-generation identity through this metadata call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_rto_release(identity: *const ConnectionIdentity) {
    let identity = unsafe { *identity };
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        (*state()).table.release(identity);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_rto_for_conn(id: u32) -> u64 {
    let Some(_guard) = Guard::acquire() else {
        return MAX_TICKS;
    };
    unsafe { for_conn(id) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_rto_backoff(id: u32, retransmits: u8) -> u64 {
    let Some(_guard) = Guard::acquire() else {
        return MAX_TICKS;
    };
    backed_off(unsafe { for_conn(id) }, retransmits)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_rto_stamp_send(id: u32) {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        let Some(identity) = tcp::connection_identity(id) else {
            return;
        };
        let Some(st) = (*state()).table.bind(identity) else {
            return;
        };
        if st.sent_tick != 0 {
            return;
        }
        let Some(outstanding) = tcp::outstanding_info(id, SegmentKind::Any) else {
            return;
        };
        st.sent_tick = outstanding.sent_tick;
        st.sent_tx_ack = tcp::tx_ack_of(id);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_tcp_rto_refresh(id: u32) {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        let Some(identity) = tcp::connection_identity(id) else {
            return;
        };
        let Some(st) = (*state()).table.bind(identity) else {
            return;
        };
        st.sent_tick = tcp::outstanding_info(id, SegmentKind::Any).map_or(0, |info| info.sent_tick);
        st.sent_tx_ack = tcp::tx_ack_of(id);
    }
}
/// # Safety
/// Exact readable connection-generation identity through this metadata call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_rto_on_ack(identity: *const ConnectionIdentity) {
    let identity = unsafe { *identity };
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        if !tcp::identity_active(identity) {
            (*state()).table.release(identity);
            return;
        }
        let Some(st) = (*state()).table.bind(identity) else {
            return;
        };
        if st.sent_tick == 0 {
            return;
        }
        let ack = tcp::tx_ack_of(identity.connection_id);
        if ack == st.sent_tx_ack {
            return;
        }
        let sample = timing::now_ticks().saturating_sub(st.sent_tick).max(1);
        st.srtt_ticks = if st.srtt_ticks == 0 {
            sample
        } else {
            (st.srtt_ticks * 7 + sample) / 8
        };
        let ticks = st.srtt_ticks;
        st.sent_tick = tcp::outstanding_info(identity.connection_id, SegmentKind::Any)
            .map_or(0, |info| info.sent_tick);
        st.sent_tx_ack = ack;
        (*state()).status.samples = (*state()).status.samples.wrapping_add(1);
        (*state()).status.last_ticks = ticks;
    }
}
/// # Safety
/// Writable exact paired output; it may be pageable and is written after admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_rto_status(out: *mut RtoStatus) {
    let value = if let Some(_guard) = Guard::acquire() {
        unsafe { (*state()).status }
    } else {
        RtoStatus::EMPTY
    };
    unsafe {
        out.write(value);
    }
}
#[derive(Clone, Copy)]
enum RetryKind {
    Data,
    Fin,
}
unsafe fn retry(slot: usize, now: u64) -> Option<(u32, RetryKind)> {
    unsafe {
        let Some(identity) = tcp::connection_identity_at(slot) else {
            (*state()).table.release_slot(slot);
            return None;
        };
        let id = identity.connection_id;
        let (kind, segment) = if tcp::fin_waiting(id) {
            (RetryKind::Fin, SegmentKind::Fin)
        } else if tcp::established(id) {
            (RetryKind::Data, SegmentKind::Data)
        } else {
            return None;
        };
        let outstanding = tcp::outstanding_info(id, segment)?;
        if outstanding.retransmits >= PROACTIVE_MAX_RESEND {
            return None;
        }
        let st = (*state()).table.bind(identity)?;
        if st.sent_tick == 0 || st.sent_tick != outstanding.sent_tick {
            st.sent_tick = outstanding.sent_tick;
            st.sent_tx_ack = tcp::tx_ack_of(id);
        }
        let rto = backed_off(from_srtt(st.srtt_ticks), outstanding.retransmits);
        if !super::tcp_runtime::deadline_reached(now, st.sent_tick.wrapping_add(rto))
            || tcp::tx_ack_of(id) != st.sent_tx_ack
        {
            return None;
        }
        Some((id, kind))
    }
}
#[repr(C)]
pub struct RtoPeers {
    pub data: unsafe extern "C" fn(u32) -> u8,
    pub fin: unsafe extern "C" fn(u32) -> u8,
}
/// # Safety
/// Immutable resident paired callbacks owned by the current transport. Callback
/// success is the original TxResult.ok=0; it retains its exact send identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_tcp_rto_sweep(now: u64, peers: *const RtoPeers) {
    let peers = unsafe { &*peers };
    for slot in 0..tcp::MAX_CONNECTIONS {
        let next = {
            let Some(_guard) = Guard::acquire() else {
                return;
            };
            unsafe { retry(slot, now) }
        };
        let Some((id, kind)) = next else {
            continue;
        };
        let result = unsafe {
            match kind {
                RetryKind::Data => (peers.data)(id),
                RetryKind::Fin => (peers.fin)(id),
            }
        };
        if result == 0 {
            let Some(_guard) = Guard::acquire() else {
                return;
            };
            unsafe {
                (*state()).status.proactive_retransmits =
                    (*state()).status.proactive_retransmits.wrapping_add(1);
            }
        }
    }
}

const _: () = {
    assert!(core::mem::size_of::<RtoStatus>() == 24);
    assert!(core::mem::align_of::<RtoStatus>() == 8);
    assert!(core::mem::offset_of!(RtoStatus, proactive_retransmits) == 0);
    assert!(core::mem::offset_of!(RtoStatus, samples) == 8);
    assert!(core::mem::offset_of!(RtoStatus, last_ticks) == 16);
};
const _: () = {
    assert!(core::mem::size_of::<RtoPeers>() == 16);
    assert!(core::mem::align_of::<RtoPeers>() == 8);
    assert!(core::mem::offset_of!(RtoPeers, data) == 0);
    assert!(core::mem::offset_of!(RtoPeers, fin) == 8);
};
