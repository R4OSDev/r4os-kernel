//! Existing bounded UDP receive-wait coordination. Data/FIFO and polling
//! remain at their exact owners; no token or state borrow crosses a peer call.
use super::{timing, udp_sockets::Info};
#[repr(C)]
pub struct Peers {
    pub recv: unsafe extern "C" fn(u32, *mut Info, *mut u8, usize) -> i32,
    pub poll: unsafe extern "C" fn(usize),
    pub idle: unsafe extern "C" fn(),
    pub error: unsafe extern "C" fn(*const u8, usize),
}
fn set_error(peers: &Peers, reason: &[u8]) {
    unsafe { (peers.error)(reason.as_ptr(), reason.len()) };
}
/// # Safety
/// Stable exact peer table and exclusive writable caller output through the
/// whole call. Peers preserve the current caller, FIFO, metadata and IRQ owners.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_udp_recv_wait(
    handle: u32,
    info: *mut Info,
    data: *mut u8,
    capacity: usize,
    timeout: u64,
    peers: *const Peers,
) -> i32 {
    let peers = unsafe { &*peers };
    let deadline = timing::Deadline::start(timeout, 200_000);
    for _ in 0..200_000 {
        let got = unsafe { (peers.recv)(handle, info, data, capacity) };
        if got != 0 {
            return got;
        }
        if timeout == 0 {
            set_error(peers, b"would-block");
            return 0;
        }
        unsafe { (peers.poll)(512) };
        if deadline.expired_now() {
            set_error(peers, b"recv-timeout");
            return 0;
        }
        unsafe { (peers.idle)() };
    }
    set_error(peers, b"recv-timeout");
    0
}
const _: () = {
    assert!(core::mem::size_of::<Peers>() == 32);
    assert!(core::mem::align_of::<Peers>() == 8);
    assert!(core::mem::offset_of!(Peers, recv) == 0);
    assert!(core::mem::offset_of!(Peers, poll) == 8);
    assert!(core::mem::offset_of!(Peers, idle) == 16);
    assert!(core::mem::offset_of!(Peers, error) == 24);
};

// Existing standard polling/idle/error peers, shared by diagnostics and R4NET.
use crate::sched::sync_types::Text;
unsafe extern "C" fn udp_poll(rounds: usize) {
    unsafe {
        super::rx_runtime::r4os_rust_net_rx_runtime_poll(rounds, 255, super::frames::peers().rx);
    }
}
unsafe extern "C" fn udp_idle() {
    crate::sched::scheduler::yield_task();
    unsafe {
        core::arch::asm!("sti", "hlt", options(nomem, nostack));
    }
}
unsafe extern "C" fn udp_error(data: *const u8, len: usize) {
    unsafe {
        super::protocol_stats::r4os_rust_net_udp_owned_error(Text { pointer: data, len });
    }
}
pub(crate) const STANDARD_PEERS: Peers = super::udp_runtime::Peers {
    recv: super::frames::r4os_rust_frames_udp_recv,
    poll: udp_poll,
    idle: udp_idle,
    error: udp_error,
};
