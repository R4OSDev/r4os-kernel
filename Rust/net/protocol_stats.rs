//! Sole original Ethernet/ARP/IPv4/ICMP/UDP metadata and ARP resolution.
//! Short network admission covers resident metadata only. Protocol execution,
//! caller output, polling and waits occur outside it. Cache methods reacquire
//! the same network owner, never a different equal-ranked I/O owner.
use super::{
    arp, cache, config, core_bytes, ethernet, icmp, ipv4, r4p_contract as wire, timing, udp,
};
use crate::{memory::owner::Guard, sched::sync_types::Text};
use core::cell::UnsafeCell;
struct Metadata {
    ethernet: ethernet::Stats,
    arp: arp::Stats,
    ipv4: ipv4::Stats,
    icmp: icmp::Stats,
    udp: udp::Stats,
    buffer_small: u64,
    icmp_sequence: u16,
}
impl Metadata {
    const EMPTY: Self = Self {
        ethernet: ethernet::Stats::EMPTY,
        arp: arp::Stats::EMPTY,
        ipv4: ipv4::Stats::EMPTY,
        icmp: icmp::Stats::EMPTY,
        udp: udp::Stats::EMPTY,
        buffer_small: 0,
        icmp_sequence: 1,
    };
}
struct Storage(UnsafeCell<Metadata>);
// SAFETY: every access holds the short common network metadata owner.
unsafe impl Sync for Storage {}
static STORAGE: Storage = Storage(UnsafeCell::new(Metadata::EMPTY));
fn metadata<T>(f: impl FnOnce(&mut Metadata) -> T) -> T {
    let _guard = Guard::acquire(2);
    unsafe { f(&mut *STORAGE.0.get()) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_protocol_stats_reset() {
    metadata(|s| *s = Metadata::EMPTY);
}
/// # Safety
/// Exact writable paired output, copied after metadata admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ethernet_owned_stats(out: *mut ethernet::Stats) {
    let value = metadata(|s| s.ethernet);
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Label retains the existing immutable kernel-string lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ethernet_owned_error(reason: Text) {
    metadata(|s| s.ethernet.last_error = reason);
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ethernet_owned_rx(input: *const wire::EthernetFrameOp) {
    let op = unsafe { *input };
    metadata(|s| ethernet::apply_rx_result(&mut s.ethernet, &op))
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ethernet_owned_tx(input: *const wire::EthernetFrameOp) {
    let op = unsafe { *input };
    metadata(|s| ethernet::apply_tx_result(&mut s.ethernet, &op))
}
/// # Safety
/// Exact writable paired output, copied after metadata admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_owned_stats(out: *mut arp::Stats, expire: bool) {
    let value = metadata(|s| {
        if expire {
            unsafe {
                cache::r4os_rust_arp_cache_expire(&mut s.arp);
            }
        }
        s.arp
    });
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Label retains the existing immutable kernel-string lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_owned_error(reason: Text) {
    metadata(|s| s.arp.last_error = reason);
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_owned_rx(input: *const wire::ArpOp) -> bool {
    let op = unsafe { *input };
    metadata(|s| arp::apply_rx_result(&mut s.arp, &op))
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_owned_tx(input: *const wire::ArpOp) {
    let op = unsafe { *input };
    metadata(|s| arp::apply_tx_result(&mut s.arp, &op))
}
/// # Safety
/// Exact writable paired output, copied after metadata admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_owned_stats(out: *mut ipv4::Stats) {
    let value = metadata(|s| s.ipv4);
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Label retains the existing immutable kernel-string lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_owned_error(reason: Text) {
    metadata(|s| s.ipv4.last_error = reason);
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_owned_rx(input: *const wire::Ipv4Op) {
    let op = unsafe { *input };
    metadata(|s| ipv4::apply_rx_result(&mut s.ipv4, &op))
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_owned_tx(input: *const wire::Ipv4Op) {
    let op = unsafe { *input };
    metadata(|s| ipv4::apply_tx_result(&mut s.ipv4, &op))
}
/// # Safety
/// Exact writable paired output, copied after metadata admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_icmp_owned_stats(out: *mut icmp::Stats) {
    let value = metadata(|s| s.icmp);
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Label retains the existing immutable kernel-string lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_icmp_owned_error(reason: Text) {
    metadata(|s| s.icmp.last_error = reason);
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_icmp_owned_rx(input: *const wire::IcmpOp) {
    let op = unsafe { *input };
    metadata(|s| icmp::apply_rx_result(&mut s.icmp, &op))
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_icmp_owned_tx(input: *const wire::IcmpOp) {
    let op = unsafe { *input };
    metadata(|s| icmp::apply_tx_result(&mut s.icmp, &op))
}
/// # Safety
/// Exact writable paired output, copied after metadata admission ends.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_udp_owned_stats(out: *mut udp::Stats) {
    let value = metadata(|s| s.udp);
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Label retains the existing immutable kernel-string lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_udp_owned_error(reason: Text) {
    metadata(|s| s.udp.last_error = reason);
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_udp_owned_rx(input: *const wire::UdpOp) {
    let op = unsafe { *input };
    metadata(|s| udp::apply_rx_result(&mut s.udp, &op))
}
/// # Safety
/// Readable paired operation staged before metadata admission. No reference
/// to a caller operation spans admission or protocol/driver callbacks.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_udp_owned_tx(input: *const wire::UdpOp) {
    let op = unsafe { *input };
    metadata(|s| udp::apply_tx_result(&mut s.udp, &op))
}

#[repr(C)]
pub struct ArpPeers {
    pub request: unsafe extern "C" fn(usize, *const [u8; 4]) -> u8,
    pub poll: unsafe extern "C" fn(usize),
    pub idle: unsafe extern "C" fn(),
}
fn arp_error(reason: Text) {
    metadata(|s| s.arp.last_error = reason);
}
fn lookup(ip: [u8; 4]) -> Option<[u8; 6]> {
    let mut out = [0; 6];
    let found =
        metadata(|s| unsafe { cache::r4os_rust_arp_cache_lookup(&mut s.arp, &ip, &mut out) });
    if found { Some(out) } else { None }
}
fn wait_arp(ip: [u8; 4], peers: &ArpPeers) -> bool {
    let deadline = timing::Deadline::start(timing::DEFAULT_ARP_RESOLVE_TIMEOUT_TICKS, 200000);
    for _ in 0..200000 {
        unsafe {
            (peers.poll)(64);
        }
        if lookup(ip).is_some() {
            return true;
        }
        if deadline.expired_now() {
            return false;
        }
        unsafe {
            (peers.idle)();
        }
    }
    lookup(ip).is_some()
}
/// # Safety
/// Readable exact IP and immutable resident peer table through bounded waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_wait(
    ip: *const [u8; 4],
    peers: *const ArpPeers,
) -> bool {
    let ip = unsafe { *ip };
    wait_arp(ip, unsafe { &*peers })
}
/// # Safety
/// Readable IP and resident typed peers; writable MAC through I/O/waits. No
/// resident reference spans any peer or caller copy. Preserve retry/drop order.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_resolve(
    index: usize,
    target: *const [u8; 4],
    out: *mut [u8; 6],
    peers: *const ArpPeers,
) -> u8 {
    let target = unsafe { *target };
    let peers = unsafe { &*peers };
    let route = core_bytes::route_ipv4_target(target);
    if route.result != 0 {
        arp_error(route.last_error);
        return route.result;
    }
    if !route.needs_arp {
        unsafe {
            out.write(route.dest_mac);
        }
        arp_error(route.last_error);
        return 0;
    }
    if let Some(mac) = lookup(route.next_hop_ip) {
        unsafe {
            out.write(mac);
        }
        metadata(|s| {
            s.arp.cache_hits += 1;
            s.arp.last_error = Text::literal(b"cache-hit");
        });
        return 0;
    }
    metadata(|s| {
        s.arp.pending_packets += 1;
        s.arp.pending_queue_limit = 0;
    });
    for attempt in 0..3 {
        metadata(|s| s.arp.resolve_attempts += 1);
        let result = unsafe { (peers.request)(index, &route.next_hop_ip) };
        if result != 0 {
            metadata(|s| s.arp.pending_drops += 1);
            return result;
        }
        if wait_arp(route.next_hop_ip, peers) {
            if let Some(mac) = lookup(route.next_hop_ip) {
                unsafe {
                    out.write(mac);
                }
                return 0;
            }
            metadata(|s| {
                s.arp.resolve_misses += 1;
                s.arp.pending_drops += 1;
                s.arp.last_error = Text::literal(b"resolve-miss");
            });
            return 6;
        }
        metadata(|s| {
            s.arp.resolve_timeouts += 1;
            if attempt + 1 < 3 {
                s.arp.resolve_retries += 1;
                s.arp.last_error = Text::literal(b"resolve-retry");
            }
        });
    }
    metadata(|s| {
        s.arp.last_error = Text::literal(b"resolve-timeout");
        s.arp.pending_timeouts += 1;
        s.arp.pending_drops += 1;
    });
    6
}
/// # Safety
/// Exact fixed addresses are staged before short metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_remember(ip: *const [u8; 4], mac: *const [u8; 6]) {
    let (ip, mac) = unsafe { (*ip, *mac) };
    metadata(|s| unsafe {
        cache::r4os_rust_arp_cache_remember(&mut s.arp, &ip, &mac);
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_arp_remember_stats() {
    metadata(|s| unsafe {
        cache::r4os_rust_arp_cache_remember_stats(&s.arp);
    });
}
/// # Safety
/// Readable IP and writable exact output; no admission spans the output copy.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_arp_lookup(ip: *const [u8; 4], out: *mut [u8; 6]) -> bool {
    let ip = unsafe { *ip };
    if let Some(mac) = lookup(ip) {
        unsafe {
            out.write(mac);
        }
        true
    } else {
        false
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_arp_cache_count_owned() -> u32 {
    metadata(|s| unsafe { cache::r4os_rust_arp_cache_count(&mut s.arp) })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_arp_cache_expire_owned() {
    metadata(|s| unsafe {
        cache::r4os_rust_arp_cache_expire(&mut s.arp);
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_arp_cache_flush_owned() {
    metadata(|s| unsafe {
        cache::r4os_rust_arp_cache_flush(&mut s.arp);
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_arp_cache_age_owned() -> u64 {
    metadata(|s| unsafe {
        cache::r4os_rust_arp_cache_expire(&mut s.arp);
        cache::r4os_rust_arp_cache_age(s.arp.cache_valid)
    })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_arp_negative_timeout() -> bool {
    metadata(|s| {
        let s = &mut s.arp;
        let before = (
            s.resolve_attempts,
            s.resolve_timeouts,
            s.pending_timeouts,
            s.pending_drops,
        );
        s.pending_packets += 1;
        s.pending_queue_limit = 0;
        s.resolve_attempts += 1;
        s.resolve_timeouts += 1;
        s.pending_timeouts += 1;
        s.pending_drops += 1;
        s.last_error = Text::literal(b"resolve-timeout");
        s.resolve_attempts == before.0 + 1
            && s.resolve_timeouts == before.1 + 1
            && s.pending_timeouts == before.2 + 1
            && s.pending_drops == before.3 + 1
            && unsafe { core::slice::from_raw_parts(s.last_error.pointer, s.last_error.len) }
                == b"resolve-timeout"
    })
}
/// # Safety
/// Readable exact target and stable original kernel reason label.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_ipv4_reject_large(
    protocol: u8,
    target: *const [u8; 4],
    reason: Text,
) {
    let target = unsafe { *target };
    let local = config::settings().local_ip;
    metadata(|s| {
        s.ipv4.dropped_tx_too_large += 1;
        s.ipv4.last_protocol = protocol;
        s.ipv4.last_source = local;
        s.ipv4.last_dest = target;
        s.ipv4.last_error = reason;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_buffer_small_note() {
    metadata(|s| s.buffer_small += 1);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_buffer_small_count() -> u64 {
    metadata(|s| s.buffer_small)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_icmp_sequence() -> u16 {
    metadata(|s| s.icmp_sequence)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_icmp_sequence_advance() {
    metadata(|s| s.icmp_sequence = s.icmp_sequence.wrapping_add(1));
}
const _: () = {
    assert!(core::mem::size_of::<ArpPeers>() == 24);
    assert!(core::mem::align_of::<ArpPeers>() == 8);
    assert!(core::mem::offset_of!(ArpPeers, request) == 0);
    assert!(core::mem::offset_of!(ArpPeers, poll) == 8);
    assert!(core::mem::offset_of!(ArpPeers, idle) == 16);
};
