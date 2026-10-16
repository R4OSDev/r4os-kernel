//! Sole original DNS resolver/statistics owner. Caller names are staged before
//! metadata admission; UDP/R4P work, waits and caller output never retain it.
//! The existing four-entry cache shares the same short network owner.
use super::{
    builders, cache, config, dns, protocol_runtime, readers, timing, transmit, udp_sockets::Info,
};
use crate::{
    memory::owner::Guard,
    sched::{scheduler, sync_types::Text},
};
use core::{cell::UnsafeCell, mem::MaybeUninit, ptr};
#[repr(C)]
pub struct Peers {
    pub bind: unsafe extern "C" fn(u16) -> i32,
    pub close: unsafe extern "C" fn(u32) -> i32,
    pub recv: unsafe extern "C" fn(u32, *mut Info, *mut u8, usize) -> i32,
    pub send: unsafe extern "C" fn(u32, *const [u8; 4], u16, *const u8, usize) -> u8,
    pub poll: unsafe extern "C" fn(usize),
}
struct Storage(UnsafeCell<dns::Stats>);
// SAFETY: every access holds the common short network metadata owner.
unsafe impl Sync for Storage {}
static STATS: Storage = Storage(UnsafeCell::new(dns::Stats::EMPTY));
struct PeerStorage(UnsafeCell<*const Peers>);
// SAFETY: one immutable kernel-lifetime table is installed before workers start.
unsafe impl Sync for PeerStorage {}
static PEERS: PeerStorage = PeerStorage(UnsafeCell::new(ptr::null()));
fn metadata<T>(f: impl FnOnce(&mut dns::Stats) -> T) -> T {
    let _guard = Guard::acquire(2);
    unsafe { f(&mut *STATS.0.get()) }
}
fn peers() -> &'static Peers {
    let p = {
        let _guard = Guard::acquire(2);
        unsafe { *PEERS.0.get() }
    };
    assert!(!p.is_null(), "DNS peers installed before use");
    unsafe { &*p }
}
fn raw_stats() -> dns::Stats {
    metadata(|s| *s)
}
fn fixed(name: &[u8]) -> [u8; 64] {
    let mut out = [0; 64];
    let n = name.len().min(63);
    out[..n].copy_from_slice(&name[..n]);
    out
}
fn failed(result: i32, reason: &'static [u8]) -> i32 {
    metadata(|s| {
        s.last_result = result;
        s.last_error = Text::literal(reason);
    });
    result
}
fn timeout(name: &[u8], server: [u8; 4]) -> i32 {
    let name = fixed(name);
    metadata(|s| {
        s.timeouts += 1;
        s.last_server = server;
        s.last_name = name;
        s.last_result = dns::RESULT_TIMEOUT;
        s.last_error = Text::literal(b"timeout");
    });
    dns::RESULT_TIMEOUT
}
fn pending(name: &[u8]) {
    let name = fixed(name);
    metadata(|s| {
        s.operation_pending = true;
        s.pending_name = name;
    });
}
struct OperationScope;
impl Drop for OperationScope {
    fn drop(&mut self) {
        metadata(|s| {
            s.operation_pending = false;
            s.pending_name = [0; 64];
        });
    }
}
struct Socket(u32);
impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            (peers().close)(self.0);
        }
    }
}
fn next_id(name: &[u8]) -> u16 {
    let mut value = timing::now_ticks() as u32;
    for &ch in name {
        value = value.wrapping_mul(33) ^ u32::from(ch.to_ascii_uppercase());
    }
    value ^= value >> 16;
    let id = value as u16;
    if id == 0 { 0x5244 } else { id }
}
fn build(id: u16, name: &[u8], out: &mut [u8]) -> Option<usize> {
    let mut report = MaybeUninit::<builders::WireBuildReport>::uninit();
    unsafe {
        builders::r4os_rust_wire_build_dns(
            id,
            name.as_ptr(),
            name.len(),
            out.as_mut_ptr(),
            out.len(),
            report.as_mut_ptr(),
        );
    }
    let report = unsafe { report.assume_init() };
    protocol_runtime::r4os_rust_net_note_dns_dispatch_failures(report.dispatch_failures);
    metadata(|s| {
        if report.reason.len != 0 {
            s.last_error = report.reason;
        }
        if report.length != usize::MAX {
            dns::apply_build_metadata(s, report.id);
        }
    });
    if report.length == usize::MAX {
        return None;
    }
    protocol_runtime::r4os_rust_net_note_dns_r4p_build(1);
    Some(report.length)
}
unsafe fn response(data: *const u8, len: usize) -> bool {
    let input = readers::WireReadInput {
        source_ip: [0; 4],
        dest_ip: [0; 4],
        own_mac: [0; 6],
        protocol: 0,
        l4_checksum_valid: false,
    };
    let mut op = MaybeUninit::uninit();
    let mut report = MaybeUninit::<readers::WireReadReport>::uninit();
    unsafe {
        readers::r4os_rust_wire_read_dns(&input, data, len, op.as_mut_ptr(), report.as_mut_ptr());
    }
    let report = unsafe { report.assume_init() };
    protocol_runtime::r4os_rust_net_note_dns_dispatch_failures(report.dispatch_failures);
    metadata(|s| {
        if report.reason.len != 0 {
            s.last_error = report.reason;
        }
        if report.completed {
            dns::apply_rx_result(s, unsafe { &*op.as_ptr() });
        }
    });
    if !report.completed {
        return false;
    }
    if report.accepted {
        protocol_runtime::r4os_rust_net_note_dns_r4p_rx(1);
    }
    report.accepted
}
// SAFETY: caller output remains writable throughout transport/waits. Only an
// owned result is copied to it, without a live resident state reference.
unsafe fn resolve_checked(name: &[u8], server: [u8; 4], out: *mut [u8; 4]) -> i32 {
    let id = next_id(name);
    let last_name = fixed(name);
    metadata(|s| {
        s.last_server = server;
        s.last_name = last_name;
    });
    if let Some((answer, result)) =
        metadata(|s| unsafe { cache::dns_lookup_owned(s, name, server) })
    {
        unsafe {
            out.write(answer);
        }
        metadata(|s| {
            s.last_result = result;
            s.last_error = Text::literal(if result == dns::RESULT_OK {
                b"cache"
            } else {
                b"cache-nxdomain"
            });
            s.last_answer = if result == dns::RESULT_OK {
                answer
            } else {
                [0; 4]
            };
        });
        return result;
    }
    pending(name);
    let _operation = OperationScope;
    let mut query = [0; 512];
    let Some(length) = build(id, name, &mut query) else {
        return failed(dns::RESULT_NAME, b"query-build");
    };
    let handle = unsafe { (peers().bind)(49152 + (id & 0x3fff)) };
    if handle <= 0 {
        metadata(|s| s.tx_errors += 1);
        return failed(dns::RESULT_TX, b"udp-bind");
    }
    let socket = Socket(handle as u32);
    let tx = unsafe { (peers().send)(socket.0, &server, dns::PORT, query.as_ptr(), length) };
    if tx != 0 {
        metadata(|s| s.tx_errors += 1);
        return failed(dns::RESULT_TX, transmit::result_name(tx));
    }
    let mut data = [0; 512];
    let deadline = timing::Deadline::start(timing::DEFAULT_DNS_TIMEOUT_TICKS, 200_000);
    for _ in 0..200_000 {
        if deadline.expired_now() {
            break;
        }
        unsafe {
            (peers().poll)(512);
        }
        let mut info = MaybeUninit::<Info>::uninit();
        let got =
            unsafe { (peers().recv)(socket.0, info.as_mut_ptr(), data.as_mut_ptr(), data.len()) };
        if got < 0 {
            return failed(dns::RESULT_TX, b"udp-recv");
        }
        if got > 0 {
            assert!(got as usize <= data.len(), "Exact DNS receive capacity");
            let info = unsafe { info.assume_init() };
            if info.source_port == dns::PORT && info.source_ip == server {
                unsafe {
                    response(data.as_ptr(), got as usize);
                }
                let s = raw_stats();
                if s.last_id == id && s.last_result != 1 {
                    if s.last_result == dns::RESULT_OK {
                        unsafe {
                            out.write(s.last_answer);
                        }
                        metadata(|stats| unsafe {
                            cache::dns_store_owned(
                                stats,
                                name,
                                server,
                                s.last_answer,
                                dns::RESULT_OK,
                            );
                        });
                        return dns::RESULT_OK;
                    }
                    if s.last_result == dns::RESULT_NXDOMAIN {
                        metadata(|stats| unsafe {
                            cache::dns_store_owned(
                                stats,
                                name,
                                server,
                                [0; 4],
                                dns::RESULT_NXDOMAIN,
                            );
                        });
                        return dns::RESULT_NXDOMAIN;
                    }
                    return s.last_result;
                }
            }
        }
        scheduler::yield_task();
    }
    timeout(name, server)
}
unsafe fn resolve(data: *const u8, len: usize, server: Option<[u8; 4]>, out: *mut [u8; 4]) -> i32 {
    metadata(|s| s.resolve_requests += 1);
    unsafe {
        out.write([0; 4]);
    }
    if len == 0 || len > 255 {
        return failed(dns::RESULT_NAME, b"name");
    }
    let server = if let Some(server) = server {
        server
    } else {
        let cfg = config::settings();
        if !cfg.dns_configured {
            return failed(dns::RESULT_NO_SERVER, b"no-server");
        }
        cfg.dns_ip
    };
    if server == [0; 4] {
        return failed(dns::RESULT_NO_SERVER, b"no-server");
    }
    let mut name = [0; 255];
    unsafe {
        ptr::copy_nonoverlapping(data, name.as_mut_ptr(), len);
        resolve_checked(&name[..len], server, out)
    }
}
/// # Safety
/// Quiescent singleton initialization with an immutable kernel-lifetime table.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_reset(input: *const Peers) {
    let _guard = Guard::acquire(2);
    unsafe {
        STATS.0.get().write(dns::Stats::EMPTY);
        PEERS.0.get().write(input);
    }
}
/// # Safety
/// Exact exclusive paired output, outside metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_stats(out: *mut dns::Stats, expire: bool) {
    let value = metadata(|s| {
        if expire {
            unsafe {
                cache::dns_expire_owned(s);
                cache::dns_sync_owned(s);
            }
        }
        *s
    });
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Exact exclusive paired output, outside metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_cache_status(out: *mut cache::DnsStatus) {
    let value = metadata(|s| unsafe { cache::dns_status_owned(s) });
    unsafe {
        out.write(value);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_dns_client_cache_count(negative: bool) -> u32 {
    metadata(|s| unsafe { cache::dns_count_owned(s, negative) })
}
/// # Safety
/// Readable retained name and writable fixed address through transport/waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_resolve(
    data: *const u8,
    len: usize,
    out: *mut [u8; 4],
) -> i32 {
    unsafe { resolve(data, len, None, out) }
}
/// # Safety
/// Readable retained name/server, writable fixed address through transport/waits.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_resolve_server(
    data: *const u8,
    len: usize,
    server: *const [u8; 4],
    out: *mut [u8; 4],
) -> i32 {
    let server = unsafe { *server };
    unsafe { resolve(data, len, Some(server), out) }
}
/// # Safety
/// Readable retained packet throughout R4P dispatch, no metadata admission held.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_response(data: *const u8, len: usize) -> bool {
    unsafe { response(data, len) }
}
/// # Safety
/// Resident immutable kernel-string label retains the original stored lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_cancel(reason: Text) -> bool {
    metadata(|s| {
        let was_pending = s.operation_pending;
        s.operation_pending = false;
        s.pending_name = [0; 64];
        if was_pending {
            s.last_result = dns::RESULT_TIMEOUT;
            s.last_error = reason;
        }
        was_pending
    })
}
/// # Safety
/// Readable diagnostic name. Stage its fixed prefix before metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_set_pending(data: *const u8, len: usize) {
    let mut name = [0; 63];
    let len = len.min(name.len());
    if len != 0 {
        unsafe {
            ptr::copy_nonoverlapping(data, name.as_mut_ptr(), len);
        }
    }
    pending(&name[..len]);
}
/// # Safety
/// Readable diagnostic name/server, staged before short metadata admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_client_timeout(
    data: *const u8,
    len: usize,
    server: *const [u8; 4],
) -> i32 {
    let server = unsafe { *server };
    let mut name = [0; 63];
    let len = len.min(name.len());
    if len != 0 {
        unsafe {
            ptr::copy_nonoverlapping(data, name.as_mut_ptr(), len);
        }
    }
    timeout(&name[..len], server)
}

const _: () = {
    assert!(core::mem::size_of::<Peers>() == 40);
    assert!(core::mem::align_of::<Peers>() == 8);
    assert!(core::mem::offset_of!(Peers, bind) == 0);
    assert!(core::mem::offset_of!(Peers, close) == 8);
    assert!(core::mem::offset_of!(Peers, recv) == 16);
    assert!(core::mem::offset_of!(Peers, send) == 24);
    assert!(core::mem::offset_of!(Peers, poll) == 32);
};
