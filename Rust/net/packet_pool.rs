//! Sole original diagnostic packet pool. Its private numeric handle lends one
//! resident slot until release; adapter removal retains callback-drain admission.
use crate::memory::owner::Guard;
use core::{cell::UnsafeCell, ptr};
pub const CAPACITY: usize = 16;
pub const MAX_FRAME: usize = 1536;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Packet {
    pub adapter_index: usize,
    pub len: usize,
    pub flags: u32,
    pub data: [u8; MAX_FRAME],
}
impl Packet {
    const EMPTY: Self = Self {
        adapter_index: 0,
        len: 0,
        flags: 0,
        data: [0; MAX_FRAME],
    };
}
struct State {
    packets: [Packet; CAPACITY],
    used: [bool; CAPACITY],
    drops: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: native metadata access holds network admission. Only the exact live
// slot's kernel borrower accesses its payload; free/reset/removal requires that
// borrower to have finished. No Rust reference escapes a native operation.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    packets: [Packet::EMPTY; CAPACITY],
    used: [false; CAPACITY],
    drops: 0,
}));
fn state() -> *mut State {
    STATE.0.get()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_packet_pool_reset() {
    let _guard = Guard::acquire(2);
    unsafe {
        ptr::write_bytes(state(), 0, 1);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_packet_alloc() -> usize {
    let _guard = Guard::acquire(2);
    unsafe {
        let s = state();
        for index in 0..CAPACITY {
            if (*s).used[index] {
                continue;
            }
            (*s).used[index] = true;
            (*s).packets[index] = Packet::EMPTY;
            return index;
        }
        (*s).drops += 1;
        usize::MAX
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_packet_free(handle: usize) {
    if handle >= CAPACITY {
        return;
    }
    let _guard = Guard::acquire(2);
    unsafe {
        (*state()).used[handle] = false;
        (*state()).packets[handle].len = 0;
    }
}
/// # Safety
/// Caller retains the exact live private slot lease and callback admission;
/// no other borrower, reset, release or adapter removal overlaps payload use.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_packet(handle: usize) -> *mut Packet {
    if handle >= CAPACITY {
        return ptr::null_mut();
    }
    let _guard = Guard::acquire(2);
    unsafe {
        if !(*state()).used[handle] {
            ptr::null_mut()
        } else {
            ptr::addr_of_mut!((*state()).packets[handle])
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_packet_free_count() -> usize {
    let _guard = Guard::acquire(2);
    unsafe { (*state()).used.iter().filter(|&&used| !used).count() }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_packet_drops() -> u64 {
    let _guard = Guard::acquire(2);
    unsafe { (*state()).drops }
}
/// # Safety
/// Registry mutation owns the completed backend/callback drain, including any
/// old index borrowed by a frame. It compacts the adapter table after this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_packet_remove_adapter(adapter: usize) {
    let _guard = Guard::acquire(2);
    unsafe {
        let s = state();
        for index in 0..CAPACITY {
            if !(*s).used[index] {
                continue;
            }
            if (*s).packets[index].adapter_index == adapter {
                (*s).used[index] = false;
                (*s).packets[index] = Packet::EMPTY;
            } else if (*s).packets[index].adapter_index > adapter {
                (*s).packets[index].adapter_index -= 1;
            }
        }
    }
}

const _: () = {
    assert!(core::mem::size_of::<Packet>() == 1560);
    assert!(core::mem::align_of::<Packet>() == 8);
    assert!(core::mem::offset_of!(Packet, adapter_index) == 0);
    assert!(core::mem::offset_of!(Packet, len) == 8);
    assert!(core::mem::offset_of!(Packet, flags) == 16);
    assert!(core::mem::offset_of!(Packet, data) == 20);
};
