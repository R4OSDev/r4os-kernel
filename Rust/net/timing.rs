//! Original network tick deadlines and operation status labels. Functional
//! clocks come from the already-native common timer, independent of metrics.
use crate::{platform::timer, sched::sync_types::Text};
pub const fn ms_to_ticks(ms: u64) -> u64 {
    let value = ms * crate::arch::pit::DEFAULT_HZ as u64 / 1000;
    if value == 0 { 1 } else { value }
}
pub const DEFAULT_ARP_CACHE_TTL_TICKS: u64 = ms_to_ticks(30_000);
pub const DEFAULT_ARP_RESOLVE_TIMEOUT_TICKS: u64 = ms_to_ticks(500);
pub const DEFAULT_DHCP_TIMEOUT_TICKS: u64 = ms_to_ticks(2_000);
pub const DEFAULT_DNS_TIMEOUT_TICKS: u64 = ms_to_ticks(4_000);
pub const DEFAULT_TCP_LISTEN_TIMEOUT_TICKS: u64 = ms_to_ticks(10_000);
pub const DEFAULT_TCP_RETRANSMIT_TIMEOUT_TICKS: u64 = ms_to_ticks(1_500);
pub const DEFAULT_TCP_TIME_WAIT_TICKS: u64 = ms_to_ticks(3_000);
pub const DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS: u64 = ms_to_ticks(4_000);
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Status {
    pub ticks: u64,
    pub hz: u64,
    pub arp_cache_ttl_ticks: u64,
    pub arp_resolve_timeout_ticks: u64,
    pub dhcp_timeout_ticks: u64,
    pub dns_timeout_ticks: u64,
    pub tcp_listen_timeout_ticks: u64,
    pub tcp_retransmit_timeout_ticks: u64,
    pub tcp_time_wait_ticks: u64,
    pub service_operation_timeout_ticks: u64,
    pub operation_status_count: u32,
}
pub fn now_ticks() -> u64 {
    timer::tick_count()
}
pub fn tick_hz() -> u64 {
    u64::from(timer::frequency())
}
pub fn status() -> Status {
    Status {
        ticks: now_ticks(),
        hz: tick_hz(),
        arp_cache_ttl_ticks: DEFAULT_ARP_CACHE_TTL_TICKS,
        arp_resolve_timeout_ticks: DEFAULT_ARP_RESOLVE_TIMEOUT_TICKS,
        dhcp_timeout_ticks: DEFAULT_DHCP_TIMEOUT_TICKS,
        dns_timeout_ticks: DEFAULT_DNS_TIMEOUT_TICKS,
        tcp_listen_timeout_ticks: DEFAULT_TCP_LISTEN_TIMEOUT_TICKS,
        tcp_retransmit_timeout_ticks: DEFAULT_TCP_RETRANSMIT_TIMEOUT_TICKS,
        tcp_time_wait_ticks: DEFAULT_TCP_TIME_WAIT_TICKS,
        service_operation_timeout_ticks: DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS,
        operation_status_count: 7,
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Deadline {
    pub start_tick: u64,
    pub timeout_ticks: u64,
    pub max_loops: usize,
}
impl Deadline {
    pub fn start(timeout_ticks: u64, max_loops: usize) -> Self {
        Self {
            start_tick: now_ticks(),
            timeout_ticks,
            max_loops,
        }
    }
    pub fn expired(self, now: u64) -> bool {
        self.timeout_ticks != 0
            && now >= self.start_tick
            && now - self.start_tick >= self.timeout_ticks
    }
    pub fn expired_now(self) -> bool {
        self.expired(now_ticks())
    }
    pub fn loop_limit_reached(self, loops: usize) -> bool {
        self.max_loops != 0 && loops >= self.max_loops
    }
    pub fn elapsed_ticks(self) -> u64 {
        elapsed_ticks_since(self.start_tick)
    }
}
pub fn elapsed_ticks_since(start: u64) -> u64 {
    if start == 0 {
        0
    } else {
        now_ticks().saturating_sub(start)
    }
}
pub fn elapsed_seconds_since(start: u64) -> u32 {
    let hz = tick_hz();
    elapsed_ticks_since(start)
        .checked_div(hz)
        .unwrap_or(0)
        .min(u64::from(u32::MAX)) as u32
}
pub fn seconds_remaining(total: u32, elapsed: u32) -> u32 {
    if total == 0 || elapsed >= total {
        0
    } else {
        total - elapsed
    }
}
pub fn ticks_from_seconds(seconds: u32) -> u64 {
    let hz = tick_hz();
    if hz == 0 || seconds == 0 {
        0
    } else {
        u64::from(seconds) * hz
    }
}
pub fn operation_status_name(state: u8) -> Text {
    Text::literal(match state {
        0 => b"idle",
        1 => b"pending",
        2 => b"ok",
        3 => b"timeout",
        4 => b"failed",
        5 => b"cancelled",
        6 => b"would-block",
        _ => panic!("Typed network operation status"),
    })
}
pub fn contract_check() -> bool {
    let d = Deadline {
        start_tick: 10,
        timeout_ticks: 5,
        max_loops: 3,
    };
    if d.expired(14)
        || !d.expired(15)
        || d.loop_limit_reached(2)
        || !d.loop_limit_reached(3)
        || seconds_remaining(10, 3) != 7
        || seconds_remaining(10, 10) != 0
    {
        return false;
    }
    let would_block = operation_status_name(6);
    let cancelled = operation_status_name(5);
    if unsafe { core::slice::from_raw_parts(would_block.pointer, would_block.len) }
        != b"would-block"
        || unsafe { core::slice::from_raw_parts(cancelled.pointer, cancelled.len) } != b"cancelled"
    {
        return false;
    }
    let s = status();
    s.tcp_time_wait_ticks == DEFAULT_TCP_TIME_WAIT_TICKS
        && s.service_operation_timeout_ticks == DEFAULT_SERVICE_OPERATION_TIMEOUT_TICKS
        && s.operation_status_count == 7
}
/// # Safety
/// Exclusive paired diagnostic output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_timing_status(out: *mut Status) {
    unsafe {
        out.write(status());
    }
}
/// # Safety
/// Exclusive paired deadline output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_deadline_start(
    timeout: u64,
    loops: usize,
    out: *mut Deadline,
) {
    unsafe {
        out.write(Deadline::start(timeout, loops));
    }
}
/// # Safety
/// Borrowed exact deadline record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_deadline_expired(d: *const Deadline, now: u64) -> bool {
    unsafe { (*d).expired(now) }
}
/// # Safety
/// Borrowed exact deadline record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_deadline_loops(d: *const Deadline, loops: usize) -> bool {
    unsafe { (*d).loop_limit_reached(loops) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_now_ticks() -> u64 {
    now_ticks()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_tick_hz() -> u64 {
    tick_hz()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_elapsed_ticks(start: u64) -> u64 {
    elapsed_ticks_since(start)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_elapsed_seconds(start: u64) -> u32 {
    elapsed_seconds_since(start)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_seconds_remaining(total: u32, elapsed: u32) -> u32 {
    seconds_remaining(total, elapsed)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_ticks_from_seconds(seconds: u32) -> u64 {
    ticks_from_seconds(seconds)
}
/// # Safety
/// Exclusive fixed Text projection; label bytes remain immutable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_operation_name(state: u8, out: *mut Text) {
    unsafe {
        out.write(operation_status_name(state));
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_timing_check() -> bool {
    contract_check()
}

const _: () = {
    assert!(core::mem::size_of::<Status>() == 88);
    assert!(core::mem::align_of::<Status>() == 8);
    assert!(core::mem::offset_of!(Status, ticks) == 0);
    assert!(core::mem::offset_of!(Status, hz) == 8);
    assert!(core::mem::offset_of!(Status, arp_cache_ttl_ticks) == 16);
    assert!(core::mem::offset_of!(Status, arp_resolve_timeout_ticks) == 24);
    assert!(core::mem::offset_of!(Status, dhcp_timeout_ticks) == 32);
    assert!(core::mem::offset_of!(Status, dns_timeout_ticks) == 40);
    assert!(core::mem::offset_of!(Status, tcp_listen_timeout_ticks) == 48);
    assert!(core::mem::offset_of!(Status, tcp_retransmit_timeout_ticks) == 56);
    assert!(core::mem::offset_of!(Status, tcp_time_wait_ticks) == 64);
    assert!(core::mem::offset_of!(Status, service_operation_timeout_ticks) == 72);
    assert!(core::mem::offset_of!(Status, operation_status_count) == 80);
};
const _: () = {
    assert!(core::mem::size_of::<Deadline>() == 24);
    assert!(core::mem::align_of::<Deadline>() == 8);
    assert!(core::mem::offset_of!(Deadline, start_tick) == 0);
    assert!(core::mem::offset_of!(Deadline, timeout_ticks) == 8);
    assert!(core::mem::offset_of!(Deadline, max_loops) == 16);
};
