//! Sole bounded UDP socket/FIFO owner. The core retains packet construction
//! and transport until its cut. Foreign output copies hold task admission,
//! without borrowing the resident table across a pageable memory access.
use crate::sched::{
    sync_locks,
    sync_types::{Text, UnwindGuard, WaitQueue},
};
use core::{cell::UnsafeCell, marker::PhantomData, ptr, slice};
pub const CAPACITY: usize = 8;
pub const QUEUE_CAPACITY: usize = 4;
pub const PAYLOAD_MAX: usize = 1472;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Info {
    pub source_ip: [u8; 4],
    pub dest_ip: [u8; 4],
    pub source_port: u16,
    pub dest_port: u16,
    pub length: u16,
}
impl Info {
    const EMPTY: Self = Self {
        source_ip: [0; 4],
        dest_ip: [0; 4],
        source_port: 0,
        dest_port: 0,
        length: 0,
    };
}
#[derive(Clone, Copy)]
struct Packet {
    info: Info,
    payload: [u8; PAYLOAD_MAX],
}
impl Packet {
    const EMPTY: Self = Self {
        info: Info::EMPTY,
        payload: [0; PAYLOAD_MAX],
    };
}
struct Socket {
    active: bool,
    handle: u32,
    port: u16,
    queue: [Packet; QUEUE_CAPACITY],
    head: usize,
    tail: usize,
    count: usize,
    rx_packets: u64,
    tx_packets: u64,
    drops: u64,
}
impl Socket {
    const EMPTY: Self = Self {
        active: false,
        handle: 0,
        port: 0,
        queue: [Packet::EMPTY; QUEUE_CAPACITY],
        head: 0,
        tail: 0,
        count: 0,
        rx_packets: 0,
        tx_packets: 0,
        drops: 0,
    };
}
struct State {
    sockets: [Socket; CAPACITY],
    next_handle: u32,
    delivered: u64,
    drops: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: every table access holds the one exact native task-bound owner.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    sockets: [const { Socket::EMPTY }; CAPACITY],
    next_handle: 1,
    delivered: 0,
    drops: 0,
}));
struct Owner(UnsafeCell<UnwindGuard>);
// SAFETY: only native unwind-guard operations mutate the resident guard.
unsafe impl Sync for Owner {}
static OWNER: Owner = Owner(UnsafeCell::new(UnwindGuard {
    owner: 0,
    owner_generation: 0,
    boot_owner: false,
    depth: 0,
    name: Text::literal(b"net-udp-sockets"),
    queue: WaitQueue::EMPTY,
    ..UnwindGuard::EMPTY
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
            "Exact UDP socket owner release"
        );
    }
}
fn state() -> *mut State {
    STATE.0.get()
}
fn text(value: &'static [u8]) -> Text {
    Text::literal(value)
}
unsafe fn by_handle(handle: u32) -> Option<usize> {
    if handle == 0 {
        return None;
    }
    unsafe {
        (*state())
            .sockets
            .iter()
            .position(|s| s.active && s.handle == handle)
    }
}
unsafe fn by_port(port: u16) -> Option<usize> {
    unsafe {
        (*state())
            .sockets
            .iter()
            .position(|s| s.active && s.port == port)
    }
}
unsafe fn bind(port: u16) -> (i32, Text) {
    unsafe {
        if port == 0 {
            return (-1, text(b"bind-port"));
        }
        if by_port(port).is_some() {
            return (-2, text(b"bind-in-use"));
        }
        let s = state();
        for index in 0..CAPACITY {
            if (*s).sockets[index].active {
                continue;
            }
            let handle = (*s).next_handle;
            (*s).next_handle = (*s).next_handle.wrapping_add(1);
            if (*s).next_handle == 0 {
                (*s).next_handle = 1;
            }
            (*s).sockets[index] = Socket {
                active: true,
                handle,
                port,
                ..Socket::EMPTY
            };
            return (
                i32::try_from(handle).expect("UDP socket handle"),
                text(b"bind-ok"),
            );
        }
        (-3, text(b"bind-full"))
    }
}
unsafe fn close(handle: u32) -> (i32, Text) {
    unsafe {
        let Some(index) = by_handle(handle) else {
            return (-1, text(b"close-handle"));
        };
        (*state()).sockets[index] = Socket::EMPTY;
        (0, text(b"close-ok"))
    }
}
unsafe fn close_all() -> u32 {
    unsafe {
        let mut count = 0;
        for socket in &mut (*state()).sockets {
            if socket.active {
                *socket = Socket::EMPTY;
                count += 1;
            }
        }
        count
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Summary {
    pub active: u32,
    pub queued: u32,
    pub delivered: u64,
    pub drops: u64,
}
impl Summary {
    const EMPTY: Self = Self {
        active: 0,
        queued: 0,
        delivered: 0,
        drops: 0,
    };
}
unsafe fn summary() -> Summary {
    unsafe {
        let s = &*state();
        let mut value = Summary {
            delivered: s.delivered,
            drops: s.drops,
            ..Summary::EMPTY
        };
        for socket in &s.sockets {
            if socket.active {
                value.active += 1;
                value.queued += socket.count as u32;
            }
        }
        value
    }
}
unsafe fn dispatch(info: Info, payload: &[u8]) -> Option<Text> {
    unsafe {
        let index = by_port(info.dest_port)?;
        let s = state();
        let socket = &mut (*s).sockets[index];
        if payload.len() > PAYLOAD_MAX {
            socket.drops += 1;
            (*s).drops += 1;
            return Some(text(b"socket-payload"));
        }
        if socket.count >= QUEUE_CAPACITY {
            socket.drops += 1;
            (*s).drops += 1;
            return Some(text(b"socket-full"));
        }
        let tail = socket.tail;
        socket.queue[tail] = Packet {
            info: Info {
                length: payload.len() as u16,
                ..info
            },
            ..Packet::EMPTY
        };
        socket.queue[tail].payload[..payload.len()].copy_from_slice(payload);
        socket.tail = (socket.tail + 1) % QUEUE_CAPACITY;
        socket.count += 1;
        socket.rx_packets += 1;
        (*s).delivered += 1;
        None
    }
}
/// # Safety
/// Bootstrap only; no live socket operations. Guard storage is never reset.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_sockets_reset() {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        ptr::write_bytes(state(), 0, 1);
        (*state()).next_handle = 1;
    }
}
/// # Safety
/// Exact separate writable error output, original caller lifecycle admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_socket_bind(port: u16, error: *mut Text) -> i32 {
    let Some(_guard) = Guard::acquire() else {
        unsafe { error.write(text(b"cancelled")) };
        return -1;
    };
    let (code, reason) = unsafe { bind(port) };
    unsafe { error.write(reason) };
    code
}
/// # Safety
/// Exact separate writable error output; handle retains its original identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_socket_close(handle: u32, error: *mut Text) -> i32 {
    let Some(_guard) = Guard::acquire() else {
        unsafe { error.write(text(b"cancelled")) };
        return -1;
    };
    let (code, reason) = unsafe { close(handle) };
    unsafe { error.write(reason) };
    code
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_udp_sockets_close_all() -> u32 {
    let Some(_guard) = Guard::acquire() else {
        return 0;
    };
    unsafe { close_all() }
}
/// # Safety
/// Exact writable summary copy, no live table pointer escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_socket_summary(out: *mut Summary) {
    let Some(_guard) = Guard::acquire() else {
        unsafe { out.write(Summary::EMPTY) };
        return;
    };
    unsafe { out.write(summary()) };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_udp_socket_bound(port: u16) -> bool {
    let Some(_guard) = Guard::acquire() else {
        return false;
    };
    unsafe { by_port(port).is_some() }
}
/// # Safety
/// Exact writable port output. Transport gets a value, never a slot pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_socket_port(handle: u32, out: *mut u16) -> bool {
    let Some(_guard) = Guard::acquire() else {
        return false;
    };
    unsafe {
        let Some(index) = by_handle(handle) else {
            return false;
        };
        out.write((*state()).sockets[index].port);
        true
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_udp_socket_note_tx(handle: u32) {
    let Some(_guard) = Guard::acquire() else {
        return;
    };
    unsafe {
        if let Some(index) = by_handle(handle) {
            (*state()).sockets[index].tx_packets += 1;
        }
    }
}
/// # Safety
/// Kernel-owned resident immutable datagram/metadata through this bounded copy.
/// Error is separate writable Text storage; no-data/success leaves it unused.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_socket_dispatch(
    info: *const Info,
    data: *const u8,
    len: usize,
    error: *mut Text,
) -> bool {
    let Some(_guard) = Guard::acquire() else {
        unsafe { error.write(text(b"cancelled")) };
        return true;
    };
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data, len) }
    };
    if let Some(reason) = unsafe { dispatch(*info, bytes) } {
        unsafe { error.write(reason) };
        true
    } else {
        false
    }
}
/// # Safety
/// Output ranges are exclusive writable caller storage, separate from the
/// resident FIFO. A pageable caller copy retains the exact task admission;
/// no resident table reference spans that copy or its possible page fault.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_socket_recv(
    handle: u32,
    info: *mut Info,
    data: *mut u8,
    capacity: usize,
    error: *mut Text,
) -> i32 {
    let Some(_guard) = Guard::acquire() else {
        unsafe { error.write(text(b"cancelled")) };
        return -1;
    };
    unsafe {
        let Some(index) = by_handle(handle) else {
            error.write(text(b"recv-handle"));
            return -1;
        };
        let s = state();
        if (*s).sockets[index].count == 0 {
            error.write(text(b"would-block"));
            return 0;
        }
        let head = (*s).sockets[index].head;
        let queued = (*s).sockets[index].queue[head];
        let len = usize::from(queued.info.length);
        if capacity < len {
            error.write(text(b"recv-small"));
            return -2;
        }
        info.write(queued.info);
        if len != 0 {
            ptr::copy_nonoverlapping(queued.payload.as_ptr(), data, len);
        }
        (*s).sockets[index].head = (head + 1) % QUEUE_CAPACITY;
        (*s).sockets[index].count -= 1;
        error.write(text(b"recv-ok"));
        len as i32
    }
}

const _: () = {
    assert!(core::mem::size_of::<Info>() == 14);
    assert!(core::mem::align_of::<Info>() == 2);
    assert!(core::mem::offset_of!(Info, source_ip) == 0);
    assert!(core::mem::offset_of!(Info, dest_ip) == 4);
    assert!(core::mem::offset_of!(Info, source_port) == 8);
    assert!(core::mem::offset_of!(Info, dest_port) == 10);
    assert!(core::mem::offset_of!(Info, length) == 12);
};
const _: () = {
    assert!(core::mem::size_of::<Summary>() == 24);
    assert!(core::mem::align_of::<Summary>() == 8);
    assert!(core::mem::offset_of!(Summary, active) == 0);
    assert!(core::mem::offset_of!(Summary, queued) == 4);
    assert!(core::mem::offset_of!(Summary, delivered) == 8);
    assert!(core::mem::offset_of!(Summary, drops) == 16);
};
