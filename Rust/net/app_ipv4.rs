//! Sole original six-bucket application IPv4 queues. Full buckets evict their
//! oldest entry; protocol-filtered reads select the original lowest sequence.
//! A task-bound owner covers the copied output acknowledgement and queue removal.
use super::core_bytes;
use crate::sched::{
    sync_locks,
    sync_types::{Text, UnwindGuard, WaitQueue},
};
use core::{cell::UnsafeCell, marker::PhantomData, ptr, slice};
pub const BUCKETS: usize = 6;
pub const CAPACITY: usize = 64;
pub const PAYLOAD_MAX: usize = 1536;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Info {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub protocol: u8,
    pub source_port: u16,
    pub dest_port: u16,
    pub truncated: bool,
    pub payload_len: u32,
}
impl Info {
    pub const EMPTY: Self = Self {
        source_ip: [0; 4],
        dest_ip: [0; 4],
        protocol: 0,
        source_port: 0,
        dest_port: 0,
        truncated: false,
        payload_len: 0,
    };
}
#[derive(Clone, Copy)]
struct Entry {
    info: Info,
    seq: u64,
    payload: [u8; PAYLOAD_MAX],
}
impl Entry {
    const EMPTY: Self = Self {
        info: Info::EMPTY,
        seq: 0,
        payload: [0; PAYLOAD_MAX],
    };
}
struct Queue {
    entries: [Entry; CAPACITY],
    head: usize,
    tail: usize,
    count: usize,
    drops: u64,
}
impl Queue {
    const EMPTY: Self = Self {
        entries: [Entry::EMPTY; CAPACITY],
        head: 0,
        tail: 0,
        count: 0,
        drops: 0,
    };
}
struct State {
    queues: [Queue; BUCKETS],
    next_seq: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: all queue accesses hold the same native task owner. A read copies
// its resident entry before touching caller memory; no queue reference crosses
// a potentially waiting fault. Reset is bootstrap-only and leaves owner intact.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    queues: [const { Queue::EMPTY }; BUCKETS],
    next_seq: 1,
}));
struct Owner(UnsafeCell<UnwindGuard>);
// SAFETY: only native guard operations mutate this exact resident owner.
unsafe impl Sync for Owner {}
static OWNER: Owner = Owner(UnsafeCell::new(UnwindGuard {
    owner: 0,
    owner_generation: 0,
    boot_owner: false,
    depth: 0,
    name: Text::literal(b"net-app-ipv4"),
    queue: WaitQueue::EMPTY,
}));
struct Guard {
    _task_bound: PhantomData<*mut ()>,
}
impl Guard {
    fn acquire() -> Option<Self> {
        crate::memory::owner::prepare_stack();
        if unsafe { sync_locks::guard_enter(OWNER.0.get(), u64::MAX) } {
            Some(Self {
                _task_bound: PhantomData,
            })
        } else {
            None
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        assert!(
            unsafe { sync_locks::guard_leave(OWNER.0.get()) },
            "Exact application IPv4 owner release"
        );
    }
}
fn state() -> *mut State {
    STATE.0.get()
}
fn ports(protocol: u8, data: &[u8]) -> (u16, u16) {
    if matches!(protocol, 17 | 6) && data.len() >= 4 {
        (
            core_bytes::read_be16(data, 0),
            core_bytes::read_be16(data, 2),
        )
    } else {
        (0, 0)
    }
}
fn bucket(protocol: u8, ports: (u16, u16)) -> usize {
    match protocol {
        1 => 0,
        17 => {
            if ports.0 == 67 || ports.1 == 68 {
                1
            } else if ports.0 == 53 || ports.1 == 53 {
                2
            } else {
                3
            }
        }
        6 => 4,
        _ => 5,
    }
}
unsafe fn enqueue(mut info: Info, data: &[u8]) {
    unsafe {
        let ports = ports(info.protocol, data);
        let b = bucket(info.protocol, ports);
        let s = state();
        if (*s).queues[b].count == CAPACITY {
            let head = (*s).queues[b].head;
            (*s).queues[b].entries[head] = Entry::EMPTY;
            (*s).queues[b].head = (head + 1) % CAPACITY;
            (*s).queues[b].count -= 1;
            (*s).queues[b].drops += 1;
        }
        let tail = (*s).queues[b].tail;
        (*s).queues[b].entries[tail] = Entry::EMPTY;
        let copied = data.len().min(PAYLOAD_MAX);
        (&mut (*s).queues[b].entries[tail].payload)[..copied].copy_from_slice(&data[..copied]);
        info.source_port = ports.0;
        info.dest_port = ports.1;
        info.truncated = data.len() > PAYLOAD_MAX;
        info.payload_len =
            u32::try_from(data.len()).expect("Application IPv4 original payload length");
        (*s).queues[b].entries[tail].info = info;
        (*s).queues[b].entries[tail].seq = (*s).next_seq;
        (*s).next_seq = (*s).next_seq.wrapping_add(1);
        if (*s).next_seq == 0 {
            (*s).next_seq = 1;
        }
        (*s).queues[b].tail = (tail + 1) % CAPACITY;
        (*s).queues[b].count += 1;
    }
}
unsafe fn index(b: usize, offset: usize) -> usize {
    unsafe { ((*state()).queues[b].head + offset) % CAPACITY }
}
unsafe fn remove(b: usize, offset: usize) {
    unsafe {
        if b >= BUCKETS || offset >= (*state()).queues[b].count {
            return;
        }
        let s = state();
        let mut i = offset;
        while i + 1 < (*s).queues[b].count {
            let dst = index(b, i);
            let src = index(b, i + 1);
            (*s).queues[b].entries[dst] = (*s).queues[b].entries[src];
            i += 1;
        }
        (*s).queues[b].count -= 1;
        let tail = ((*s).queues[b].head + (*s).queues[b].count) % CAPACITY;
        (*s).queues[b].tail = tail;
        (*s).queues[b].entries[tail] = Entry::EMPTY;
    }
}
/// # Safety
/// Singleton bootstrap before admitting queue consumers/producers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_app_ipv4_reset() {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        ptr::write_bytes(state(), 0, 1);
        (*state()).next_seq = 1;
    }
}
/// # Safety
/// Immutable resident kernel frame and exact metadata through the bounded copy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_app_ipv4_enqueue(
    info: *const Info,
    data: *const u8,
    len: usize,
) {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    let data = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, len) }
    };
    unsafe { enqueue(*info, data) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_app_ipv4_queued() -> usize {
    let Some(_guard) = Guard::acquire() else {
        return 0;
    };
    unsafe { (*state()).queues.iter().map(|q| q.count).sum() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_app_ipv4_drops() -> u64 {
    let Some(_guard) = Guard::acquire() else {
        return 0;
    };
    unsafe { (*state()).queues.iter().map(|q| q.drops).sum() }
}
/// # Safety
/// Separate writable caller outputs through completion. The task owner pins
/// the exact selected entry until both output writes finish, including a
/// pageable caller copy. Truncated reads still consume the original entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_app_ipv4_read(
    protocol: u8,
    out: *mut Info,
    data: *mut u8,
    capacity: usize,
) -> i32 {
    let Some(_guard) = Guard::acquire() else {
        unsafe { out.write(Info::EMPTY) };
        return 0;
    };
    unsafe {
        let s = state();
        let mut best = None;
        let mut best_offset = 0;
        let mut best_seq = u64::MAX;
        for b in 0..BUCKETS {
            for offset in 0..(*s).queues[b].count {
                let idx = index(b, offset);
                let entry = &(*s).queues[b].entries[idx];
                if protocol != 0 && protocol != entry.info.protocol {
                    continue;
                }
                if entry.seq < best_seq {
                    best = Some(b);
                    best_offset = offset;
                    best_seq = entry.seq;
                }
            }
        }
        let Some(b) = best else {
            out.write(Info::EMPTY);
            return 0;
        };
        let idx = index(b, best_offset);
        let entry = (*s).queues[b].entries[idx];
        let mut info = entry.info;
        let available = (info.payload_len as usize).min(PAYLOAD_MAX);
        let copied = available.min(capacity);
        if copied != 0 {
            ptr::copy_nonoverlapping(entry.payload.as_ptr(), data, copied);
        }
        if copied < available {
            info.truncated = true;
        }
        out.write(info);
        remove(b, best_offset);
        copied as i32
    }
}

const _: () = {
    assert!(core::mem::size_of::<Info>() == 20);
    assert!(core::mem::align_of::<Info>() == 4);
    assert!(core::mem::offset_of!(Info, source_ip) == 0);
    assert!(core::mem::offset_of!(Info, dest_ip) == 4);
    assert!(core::mem::offset_of!(Info, protocol) == 8);
    assert!(core::mem::offset_of!(Info, source_port) == 10);
    assert!(core::mem::offset_of!(Info, dest_port) == 12);
    assert!(core::mem::offset_of!(Info, truncated) == 14);
    assert!(core::mem::offset_of!(Info, payload_len) == 16);
};
