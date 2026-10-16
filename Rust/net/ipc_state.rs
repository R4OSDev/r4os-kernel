//! Sole network-service handle/tombstone and lifecycle metadata owner.
//! TCP queries/close may wait: snapshot entries, release short admission, then
//! revalidate the exact handle/connection/owner before retiring or consuming it.
use super::{ipc_types::*, tcp, tcp_client};
use crate::{contract, memory::owner::Guard, sched::sync_types::Text};
use core::{cell::UnsafeCell, mem::MaybeUninit, slice};
#[derive(Clone, Copy, PartialEq, Eq)]
struct Handle {
    used: bool,
    handle: u32,
    conn_id: u32,
    owner_id: u16,
}
impl Handle {
    const EMPTY: Self = Self {
        used: false,
        handle: 0,
        conn_id: 0,
        owner_id: 0,
    };
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Tombstone {
    entry: Handle,
    cause: u32,
}
impl Tombstone {
    const EMPTY: Self = Self {
        entry: Handle::EMPTY,
        cause: SOCKET_LIFECYCLE_CLOSED_PUBLIC,
    };
}
#[derive(Clone, Copy)]
pub(super) struct Metadata {
    pub owner_mismatches: u64,
    pub stale_handles_reaped: u64,
    pub read_wait_timeouts: u64,
    pub accept_wait_timeouts: u64,
    pub write_would_block: u64,
    pub stale_responses_skipped: u64,
    pub tcp_lifecycle: SocketLifecycleCounters,
    pub udp_lifecycle: SocketLifecycleCounters,
    pub restart: RestartStatus,
    pub backpressure: ServiceBackpressureStatus,
    pub lifecycle: ServiceLifecycleStatus,
}
impl Metadata {
    const EMPTY: Self = Self {
        owner_mismatches: 0,
        stale_handles_reaped: 0,
        read_wait_timeouts: 0,
        accept_wait_timeouts: 0,
        write_would_block: 0,
        stale_responses_skipped: 0,
        tcp_lifecycle: SocketLifecycleCounters::EMPTY,
        udp_lifecycle: SocketLifecycleCounters::EMPTY,
        restart: RestartStatus::EMPTY,
        backpressure: ServiceBackpressureStatus::EMPTY,
        lifecycle: ServiceLifecycleStatus::EMPTY,
    };
}
struct State {
    handles: [Handle; TCP_SERVICE_HANDLES],
    tombs: [Tombstone; TCP_STALE_HANDLE_TOMBSTONES],
    next_tomb: usize,
    next_handle: u32,
    metadata: Metadata,
}
struct Storage(UnsafeCell<State>);
// SAFETY: only bounded resident operations hold short network admission;
// no state reference escapes, and no TCP/I/O/callback operation occurs inside it.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    handles: [Handle::EMPTY; TCP_SERVICE_HANDLES],
    tombs: [Tombstone::EMPTY; TCP_STALE_HANDLE_TOMBSTONES],
    next_tomb: 0,
    next_handle: 1,
    metadata: Metadata::EMPTY,
}));
fn state<T>(f: impl FnOnce(&mut State) -> T) -> T {
    let _guard = Guard::acquire(2);
    unsafe { f(&mut *STATE.0.get()) }
}
pub(super) fn snapshot() -> Metadata {
    state(|s| s.metadata)
}
pub(super) fn note_payload_too_large() {
    state(|s| s.metadata.backpressure.payload_too_large += 1);
}
pub(super) fn note_response_small() {
    state(|s| s.metadata.backpressure.response_buffer_small += 1);
}
pub(super) fn note_send_failure() {
    state(|s| s.metadata.backpressure.send_failures += 1);
}
pub(super) fn note_write_would_block() {
    state(|s| s.metadata.write_would_block += 1);
}
pub(super) fn note_accept_timeout() {
    state(|s| s.metadata.accept_wait_timeouts += 1);
}
pub(super) fn record_close(status: u8) {
    state(|s| match status {
        2 => s.metadata.lifecycle.close_ok += 1,
        5 => s.metadata.lifecycle.close_cancelled += 1,
        _ => s.metadata.lifecycle.close_failed += 1,
    });
}
pub(super) fn note_cleanup(reason: &[u8], connections: u32, listeners: u32) {
    state(|s| {
        if connections != 0 || listeners != 0 {
            s.metadata.lifecycle.cleanup_aborts += 1;
        }
        if reason == b"poweroff" {
            s.metadata.lifecycle.cleanup_poweroff += 1;
        } else if reason == b"reboot" {
            s.metadata.lifecycle.cleanup_reboot += 1;
        }
    });
}
pub(super) fn record_restart(queued: u32) {
    state(|s| {
        s.metadata.restart.restarts += 1;
        s.metadata.restart.last_queued = queued;
        s.metadata.restart.last_handles = s.handles.iter().filter(|h| h.used).count() as u32;
        s.metadata.restart.last_tombstones = s.tombs.iter().filter(|t| t.entry.used).count() as u32;
    });
}
pub(super) fn backpressure() -> ServiceBackpressureStatus {
    state(|s| {
        s.metadata.backpressure.stale_skips = s.metadata.stale_responses_skipped;
        s.metadata.backpressure
    })
}
fn record(counters: &mut SocketLifecycleCounters, cause: u32) {
    if cause == SOCKET_LIFECYCLE_UNKNOWN_PUBLIC {
        return;
    }
    counters.last_cause = cause;
    match cause {
        SOCKET_LIFECYCLE_CLOSED_PUBLIC => counters.closed += 1,
        SOCKET_LIFECYCLE_RESET_PUBLIC => counters.reset += 1,
        SOCKET_LIFECYCLE_TIMEOUT_PUBLIC => counters.timeout += 1,
        SOCKET_LIFECYCLE_PEER_GONE_PUBLIC => counters.peer_gone += 1,
        SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC => counters.local_abort += 1,
        SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC => counters.local_close += 1,
        SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC => counters.pending_close += 1,
        SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC => counters.would_block += 1,
        SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC => counters.bad_handle += 1,
        SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC => counters.owner_mismatch += 1,
        SOCKET_LIFECYCLE_DROPPED_PUBLIC => counters.dropped += 1,
        _ => {}
    }
}
pub(super) fn record_tcp(cause: u32) {
    state(|s| record(&mut s.metadata.tcp_lifecycle, cause));
}
pub(super) fn record_udp(cause: u32) {
    state(|s| record(&mut s.metadata.udp_lifecycle, cause));
}
pub(super) fn clear_handles() {
    state(|s| {
        s.handles.fill(Handle::EMPTY);
        s.tombs.fill(Tombstone::EMPTY);
        s.next_tomb = 0;
        s.next_handle = 1;
    });
}
pub(super) fn count_raw() -> u32 {
    state(|s| s.handles.iter().filter(|h| h.used).count() as u32)
}
pub(super) fn tomb_count() -> u32 {
    state(|s| s.tombs.iter().filter(|h| h.entry.used).count() as u32)
}
pub(super) fn connection(id: u32) -> Option<contract::TcpConnectionInfo> {
    let mut summary = MaybeUninit::uninit();
    let summary = unsafe {
        tcp::r4os_rust_net_tcp_summary(summary.as_mut_ptr());
        summary.assume_init()
    };
    for index in 0..summary.max_connections {
        let mut info = MaybeUninit::uninit();
        if unsafe { tcp::r4os_rust_net_tcp_connection_info(index, info.as_mut_ptr()) } <= 0 {
            continue;
        }
        let info = unsafe { info.assume_init() };
        if info.id == id {
            return Some(info);
        }
    }
    None
}
pub(super) fn usable(id: u32) -> bool {
    connection(id).is_some_and(|c| c.state != 0 || c.pending_rx != 0)
}
pub(super) fn tcp_last_error() -> Text {
    let mut out = MaybeUninit::uninit();
    unsafe {
        tcp::r4os_rust_net_tcp_get_stats(out.as_mut_ptr());
        out.assume_init()
    }
    .last_error
}
pub(super) fn text(value: Text) -> &'static [u8] {
    // SAFETY: owner status labels here are stable kernel strings, no table borrow.
    if value.len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(value.pointer, value.len) }
    }
}
pub(super) fn lifecycle_from_error(error: &[u8]) -> u32 {
    let contains = |needle: &[u8]| super::core_bytes::mem_contains(error, needle);
    if error.is_empty() {
        return SOCKET_LIFECYCLE_UNKNOWN_PUBLIC;
    }
    if contains(b"owner-mismatch") {
        return SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC;
    }
    if contains(b"bad-handle") || contains(b"handle-full") || contains(b"dead-handle") {
        return SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC;
    }
    if contains(b"would-block") || contains(b"poll-empty") {
        return SOCKET_LIFECYCLE_WOULD_BLOCK_PUBLIC;
    }
    if contains(b"timeout") {
        return SOCKET_LIFECYCLE_TIMEOUT_PUBLIC;
    }
    if contains(b"rst") || contains(b"reset") {
        return SOCKET_LIFECYCLE_RESET_PUBLIC;
    }
    if contains(b"fin-rx") || contains(b"peer") {
        return SOCKET_LIFECYCLE_PEER_GONE_PUBLIC;
    }
    if contains(b"abort") {
        return SOCKET_LIFECYCLE_LOCAL_ABORT_PUBLIC;
    }
    if contains(b"close") {
        return SOCKET_LIFECYCLE_LOCAL_CLOSE_PUBLIC;
    }
    if contains(b"bad-state") {
        return SOCKET_LIFECYCLE_CLOSED_PUBLIC;
    }
    SOCKET_LIFECYCLE_UNKNOWN_PUBLIC
}
pub(super) fn cause(id: u32) -> u32 {
    if let Some(c) = connection(id) {
        return match c.state {
            0 => {
                if c.pending_rx != 0 {
                    SOCKET_LIFECYCLE_PEER_GONE_PUBLIC
                } else {
                    SOCKET_LIFECYCLE_CLOSED_PUBLIC
                }
            }
            3 => SOCKET_LIFECYCLE_PENDING_CLOSE_PUBLIC,
            _ => SOCKET_LIFECYCLE_ACTIVE_PUBLIC,
        };
    }
    let cause = lifecycle_from_error(text(tcp_last_error()));
    if cause == 0 {
        SOCKET_LIFECYCLE_PEER_GONE_PUBLIC
    } else {
        cause
    }
}
fn remember(s: &mut State, entry: Handle, cause: u32) {
    if !entry.used || entry.handle == 0 {
        return;
    }
    let tomb = Tombstone { entry, cause };
    if let Some(t) = s
        .tombs
        .iter_mut()
        .find(|t| t.entry.used && t.entry.handle == entry.handle)
    {
        *t = tomb;
        return;
    }
    if let Some(t) = s.tombs.iter_mut().find(|t| !t.entry.used) {
        *t = tomb;
        return;
    }
    s.tombs[s.next_tomb] = tomb;
    s.next_tomb = (s.next_tomb + 1) % TCP_STALE_HANDLE_TOMBSTONES;
}
fn reap_at(index: usize, entry: Handle, cause: u32) {
    let retire = state(|s| {
        if s.handles[index] != entry {
            return false;
        }
        remember(s, entry, cause);
        s.handles[index] = Handle::EMPTY;
        s.metadata.stale_handles_reaped += 1;
        record(&mut s.metadata.tcp_lifecycle, cause);
        true
    });
    if retire {
        tcp_client::r4os_rust_tcp_client_close(entry.conn_id);
    }
}
pub(super) fn reap() {
    for index in 0..TCP_SERVICE_HANDLES {
        let entry = state(|s| s.handles[index]);
        if !entry.used || usable(entry.conn_id) {
            continue;
        }
        reap_at(index, entry, cause(entry.conn_id));
    }
}
fn owner_matches(handle: u16, request: u16) -> bool {
    handle == 0 || request == 0 || handle == request
}
pub(super) fn allocate(id: u32, owner: u16) -> Option<u32> {
    reap();
    if let Some(existing) = for_connection(id, owner) {
        return Some(existing);
    }
    state(|s| {
        // Recheck allocation identity after the last waitable TCP query.
        if let Some(h) = s
            .handles
            .iter()
            .find(|h| h.used && h.conn_id == id && h.owner_id == owner)
        {
            return Some(h.handle);
        }
        let index = s.handles.iter().position(|h| !h.used)?;
        let handle = s.next_handle;
        s.next_handle = s.next_handle.wrapping_add(1);
        if s.next_handle == 0 {
            s.next_handle = 1;
        }
        s.handles[index] = Handle {
            used: true,
            handle,
            conn_id: id,
            owner_id: owner,
        };
        Some(handle)
    })
}
#[derive(Clone, Copy)]
pub(super) struct Lookup {
    pub conn_id: u32,
    pub result: i32,
    pub last_error: &'static [u8],
    pub lifecycle_cause: u32,
}
impl Lookup {
    const BAD: Self = Self {
        conn_id: 0,
        result: -1,
        last_error: b"bad-handle",
        lifecycle_cause: SOCKET_LIFECYCLE_BAD_HANDLE_PUBLIC,
    };
}
fn mismatch(s: &mut State) -> Lookup {
    s.metadata.owner_mismatches += 1;
    record(
        &mut s.metadata.tcp_lifecycle,
        SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC,
    );
    Lookup {
        result: -4,
        last_error: b"owner-mismatch",
        lifecycle_cause: SOCKET_LIFECYCLE_OWNER_MISMATCH_PUBLIC,
        ..Lookup::BAD
    }
}
pub(super) fn resolve(handle: u32, owner: u16) -> Lookup {
    for index in 0..TCP_SERVICE_HANDLES {
        let entry = state(|s| s.handles[index]);
        if !entry.used || entry.handle != handle {
            continue;
        }
        if !usable(entry.conn_id) {
            let cause = cause(entry.conn_id);
            reap_at(index, entry, cause);
            return Lookup {
                result: -5,
                last_error: b"dead-handle",
                lifecycle_cause: cause,
                ..Lookup::BAD
            };
        }
        if !owner_matches(entry.owner_id, owner) {
            if let Some(result) = state(|s| (s.handles[index] == entry).then(|| mismatch(s))) {
                return result;
            }
            continue;
        }
        let cause = cause(entry.conn_id);
        if state(|s| s.handles[index] == entry) {
            return Lookup {
                conn_id: entry.conn_id,
                result: 0,
                last_error: b"",
                lifecycle_cause: cause,
            };
        }
    }
    Lookup::BAD
}
pub(super) fn resolve_close(handle: u32, owner: u16) -> Lookup {
    for index in 0..TCP_SERVICE_HANDLES {
        let entry = state(|s| s.handles[index]);
        if !entry.used || entry.handle != handle {
            continue;
        }
        // Close checks ownership before the waitable lifecycle query, as before.
        if !owner_matches(entry.owner_id, owner) {
            if let Some(result) = state(|s| (s.handles[index] == entry).then(|| mismatch(s))) {
                return result;
            }
            continue;
        }
        let cause = cause(entry.conn_id);
        if state(|s| s.handles[index] == entry) {
            return Lookup {
                conn_id: entry.conn_id,
                result: 0,
                last_error: b"",
                lifecycle_cause: cause,
            };
        }
    }
    state(|s| {
        for index in 0..TCP_STALE_HANDLE_TOMBSTONES {
            let tomb = s.tombs[index];
            if !tomb.entry.used || tomb.entry.handle != handle {
                continue;
            }
            if !owner_matches(tomb.entry.owner_id, owner) {
                return mismatch(s);
            }
            s.tombs[index] = Tombstone::EMPTY;
            return Lookup {
                conn_id: tomb.entry.conn_id,
                result: 0,
                last_error: b"",
                lifecycle_cause: tomb.cause,
            };
        }
        Lookup::BAD
    })
}
pub(super) fn release(handle: u32) {
    state(|s| {
        if let Some(h) = s.handles.iter_mut().find(|h| h.used && h.handle == handle) {
            *h = Handle::EMPTY;
            return;
        }
        if let Some(t) = s
            .tombs
            .iter_mut()
            .find(|t| t.entry.used && t.entry.handle == handle)
        {
            *t = Tombstone::EMPTY;
        }
    });
}
pub(super) fn count() -> u32 {
    reap();
    count_raw()
}
pub(super) fn owned_count(owner: u16) -> u16 {
    if owner == 0 {
        return 0;
    }
    reap();
    state(|s| {
        s.handles
            .iter()
            .filter(|h| h.used && h.owner_id == owner)
            .count() as u16
    })
}
pub(super) fn legacy_count() -> u16 {
    reap();
    state(|s| {
        s.handles
            .iter()
            .filter(|h| h.used && h.owner_id == 0)
            .count() as u16
    })
}
pub(super) fn for_connection(id: u32, owner: u16) -> Option<u32> {
    reap();
    state(|s| {
        s.handles
            .iter()
            .find(|h| h.used && h.conn_id == id && h.owner_id == owner)
            .map(|h| h.handle)
    })
}
pub(super) fn for_connection_any(id: u32) -> Option<u32> {
    reap();
    state(|s| {
        s.handles
            .iter()
            .find(|h| h.used && h.conn_id == id)
            .map(|h| h.handle)
    })
}
pub(super) fn owner_for_connection(id: u32) -> u16 {
    reap();
    state(|s| {
        s.handles
            .iter()
            .find(|h| h.used && h.conn_id == id)
            .map_or(0, |h| h.owner_id)
    })
}
