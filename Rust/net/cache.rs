//! Original ARP/DNS cache tables. Admission covers only bounded resident
//! metadata; caller names are staged before admission and output copies follow it.
use super::{arp, core_bytes, dns, timing};
use crate::{memory::owner::Guard, sched::sync_types::Text};
use core::{cell::UnsafeCell, ptr};
const ARP_CAPACITY: usize = 8;
const DNS_CAPACITY: usize = 4;
const DNS_TTL: u32 = 60;
const DNS_NEGATIVE_TTL: u32 = 30;
#[derive(Clone, Copy)]
struct ArpEntry {
    valid: bool,
    ip: [u8; 4],
    mac: [u8; 6],
    updated_tick: u64,
    hits: u64,
}
impl ArpEntry {
    const EMPTY: Self = Self {
        valid: false,
        ip: [0; 4],
        mac: [0; 6],
        updated_tick: 0,
        hits: 0,
    };
}
#[derive(Clone, Copy)]
struct DnsEntry {
    valid: bool,
    negative: bool,
    name: [u8; 64],
    server: [u8; 4],
    answer: [u8; 4],
    result: i32,
    updated_tick: u64,
    ttl_seconds: u32,
    hits: u64,
}
impl DnsEntry {
    const EMPTY: Self = Self {
        valid: false,
        negative: false,
        name: [0; 64],
        server: [0; 4],
        answer: [0; 4],
        result: dns::RESULT_OK,
        updated_tick: 0,
        ttl_seconds: DNS_TTL,
        hits: 0,
    };
}
struct State {
    arp: [ArpEntry; ARP_CAPACITY],
    arp_next: usize,
    arp_updated: u64,
    dns: [DnsEntry; DNS_CAPACITY],
    dns_next: usize,
}
impl State {
    const EMPTY: Self = Self {
        arp: [ArpEntry::EMPTY; ARP_CAPACITY],
        arp_next: 0,
        arp_updated: 0,
        dns: [DnsEntry::EMPTY; DNS_CAPACITY],
        dns_next: 0,
    };
}
struct Storage(UnsafeCell<State>);
// SAFETY: all table access uses the short common network metadata admission.
// No module callback, wait, pageable input or caller output spans that admission.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State::EMPTY));
fn state() -> *mut State {
    STATE.0.get()
}
fn admit() -> Guard {
    Guard::acquire(2)
}
fn now() -> u64 {
    timing::now_ticks()
}
fn arp_primary(stats: &mut arp::Stats, s: &mut State, entry: ArpEntry) {
    stats.cache_valid = entry.valid;
    stats.cache_ip = entry.ip;
    stats.cache_mac = entry.mac;
    s.arp_updated = entry.updated_tick;
}
fn arp_sync(stats: &mut arp::Stats, s: &mut State) {
    let mut best = None;
    let mut tick = 0;
    for (i, e) in s.arp.iter().enumerate() {
        if e.valid && (best.is_none() || e.updated_tick >= tick) {
            best = Some(i);
            tick = e.updated_tick;
        }
    }
    if let Some(i) = best {
        arp_primary(stats, s, s.arp[i]);
    } else {
        stats.cache_valid = false;
        stats.cache_ip = [0; 4];
        stats.cache_mac = [0; 6];
        s.arp_updated = 0;
    }
}
fn arp_expire(stats: &mut arp::Stats, s: &mut State) {
    let now = now();
    let mut expired = false;
    for e in &mut s.arp {
        if e.valid
            && e.updated_tick != 0
            && now >= e.updated_tick
            && now - e.updated_tick > timing::DEFAULT_ARP_CACHE_TTL_TICKS
        {
            e.valid = false;
            expired = true;
        }
    }
    if stats.cache_valid
        && s.arp_updated != 0
        && now >= s.arp_updated
        && now - s.arp_updated > timing::DEFAULT_ARP_CACHE_TTL_TICKS
    {
        expired = true;
    }
    if expired {
        arp_sync(stats, s);
        if !stats.cache_valid {
            stats.last_error = Text::literal(b"expired");
        }
    }
}
fn arp_store(s: &mut State, ip: [u8; 4], mac: [u8; 6], tick: u64) {
    let mut free = None;
    let mut oldest = 0;
    let mut oldest_tick = u64::MAX;
    for (i, e) in s.arp.iter_mut().enumerate() {
        if e.valid && e.ip == ip {
            e.mac = mac;
            e.updated_tick = tick;
            return;
        }
        if !e.valid && free.is_none() {
            free = Some(i);
        }
        if e.updated_tick < oldest_tick {
            oldest_tick = e.updated_tick;
            oldest = i;
        }
    }
    let slot = if let Some(i) = free {
        i
    } else {
        let chosen = s.arp_next;
        s.arp_next = (s.arp_next + 1) % ARP_CAPACITY;
        if oldest_tick != 0 { oldest } else { chosen }
    };
    s.arp[slot] = ArpEntry {
        valid: true,
        ip,
        mac,
        updated_tick: tick,
        hits: 0,
    };
}
fn dns_fresh(e: &DnsEntry) -> bool {
    e.valid
        && e.updated_tick != 0
        && e.ttl_seconds != 0
        && timing::elapsed_seconds_since(e.updated_tick) < e.ttl_seconds
}
fn dns_primary(stats: &mut dns::Stats, e: &DnsEntry) {
    stats.cache_valid = e.valid;
    stats.cache_name = e.name;
    stats.cache_server = e.server;
    stats.cache_answer = e.answer;
    stats.cache_updated_tick = e.updated_tick;
    stats.cache_ttl_seconds = e.ttl_seconds;
}
fn dns_best(s: &State) -> Option<usize> {
    let mut best = None;
    let mut tick = 0;
    for (i, e) in s.dns.iter().enumerate() {
        if dns_fresh(e) && (best.is_none() || e.updated_tick >= tick) {
            best = Some(i);
            tick = e.updated_tick;
        }
    }
    best
}
fn dns_sync(stats: &mut dns::Stats, s: &State) {
    if let Some(i) = dns_best(s) {
        dns_primary(stats, &s.dns[i]);
        return;
    }
    stats.cache_valid = false;
    stats.cache_name = [0; 64];
    stats.cache_server = [0; 4];
    stats.cache_answer = [0; 4];
    stats.cache_updated_tick = 0;
    stats.cache_ttl_seconds = DNS_TTL;
}
fn dns_expire(stats: &mut dns::Stats, s: &mut State) {
    let mut changed = false;
    for e in &mut s.dns {
        if e.valid && !dns_fresh(e) {
            *e = DnsEntry::EMPTY;
            changed = true;
        }
    }
    if changed {
        dns_sync(stats, s);
    }
}
struct Name {
    bytes: [u8; 64],
    len: usize,
}
impl Name {
    // SAFETY: caller retains the readable name prefix. Stage it before admitting
    // the cache; comparison keeps the full original length, including >64 names.
    unsafe fn read(data: *const u8, len: usize) -> Self {
        let mut n = Self {
            bytes: [0; 64],
            len,
        };
        if len != 0 {
            unsafe {
                ptr::copy_nonoverlapping(data, n.bytes.as_mut_ptr(), len.min(64));
            }
        }
        n
    }
    fn matches(&self, fixed: &[u8; 64]) -> bool {
        self.len <= 64 && core_bytes::fixed_text_equals_ignore_case(fixed, &self.bytes[..self.len])
    }
}
fn dns_find(s: &mut State, name: &Name, server: [u8; 4]) -> Option<usize> {
    for (i, e) in s.dns.iter_mut().enumerate() {
        if !e.valid {
            continue;
        }
        if !dns_fresh(e) {
            *e = DnsEntry::EMPTY;
            continue;
        }
        if e.server == server && name.matches(&e.name) {
            return Some(i);
        }
    }
    None
}
fn dns_replace(s: &mut State) -> usize {
    for (i, e) in s.dns.iter_mut().enumerate() {
        if !e.valid || !dns_fresh(e) {
            *e = DnsEntry::EMPTY;
            s.dns_next = (i + 1) % DNS_CAPACITY;
            return i;
        }
    }
    let slot = s.dns_next;
    s.dns_next = (s.dns_next + 1) % DNS_CAPACITY;
    slot
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DnsStatus {
    pub valid: bool,
    pub negative: bool,
    pub age_seconds: u32,
    pub ttl_seconds: u32,
    pub remaining_seconds: u32,
}
impl DnsStatus {
    const EMPTY: Self = Self {
        valid: false,
        negative: false,
        age_seconds: 0,
        ttl_seconds: 0,
        remaining_seconds: 0,
    };
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_cache_reset() {
    let _guard = admit();
    unsafe {
        state().write(State::EMPTY);
    }
}
/// # Safety
/// Exact paired resident/stack statistics, readable fixed addresses and writable
/// outputs. Only lookup name and answer may be pageable; neither spans admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_arp_cache_remember(
    stats: *mut arp::Stats,
    ip: *const [u8; 4],
    mac: *const [u8; 6],
) {
    let (ip, mac) = unsafe { (*ip, *mac) };
    if ip == [0; 4] || mac == [0; 6] {
        return;
    }
    let _guard = admit();
    unsafe {
        let s = &mut *state();
        let stats = &mut *stats;
        let tick = now();
        arp_store(s, ip, mac, tick);
        stats.cache_ip = ip;
        stats.cache_mac = mac;
        stats.cache_valid = true;
        stats.cache_updates += 1;
        s.arp_updated = tick;
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_arp_cache_remember_stats(stats: *const arp::Stats) {
    let _guard = admit();
    unsafe {
        let stats = &*stats;
        if !stats.cache_valid || stats.cache_ip == [0; 4] || stats.cache_mac == [0; 6] {
            return;
        }
        let tick = now();
        let s = &mut *state();
        arp_store(s, stats.cache_ip, stats.cache_mac, tick);
        s.arp_updated = tick;
    }
}
/// # Safety
/// Exact paired resident/stack statistics and readable fixed IP; writable MAC.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_arp_cache_lookup(
    stats: *mut arp::Stats,
    ip: *const [u8; 4],
    out: *mut [u8; 6],
) -> bool {
    let ip = unsafe { *ip };
    let value = {
        let _guard = admit();
        unsafe {
            let s = &mut *state();
            let stats = &mut *stats;
            arp_expire(stats, s);
            let found = s.arp.iter().position(|e| e.valid && e.ip == ip);
            if let Some(i) = found {
                s.arp[i].hits += 1;
                let e = s.arp[i];
                arp_primary(stats, s, e);
                Some(e.mac)
            } else {
                None
            }
        }
    };
    if let Some(mac) = value {
        unsafe { out.write(mac) };
        true
    } else {
        false
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_arp_cache_expire(stats: *mut arp::Stats) {
    let _guard = admit();
    unsafe {
        arp_expire(&mut *stats, &mut *state());
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_arp_cache_count(stats: *mut arp::Stats) -> u32 {
    let _guard = admit();
    unsafe {
        let s = &mut *state();
        arp_expire(&mut *stats, s);
        s.arp.iter().filter(|e| e.valid).count() as u32
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_arp_cache_age(valid: bool) -> u64 {
    let _guard = admit();
    unsafe {
        let tick = (*state()).arp_updated;
        if !valid || tick == 0 {
            0
        } else {
            now().saturating_sub(tick)
        }
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_arp_cache_flush(stats: *mut arp::Stats) {
    let _guard = admit();
    unsafe {
        let s = &mut *state();
        s.arp = [ArpEntry::EMPTY; ARP_CAPACITY];
        s.arp_next = 0;
        s.arp_updated = 0;
        let stats = &mut *stats;
        stats.cache_valid = false;
        stats.cache_ip = [0; 4];
        stats.cache_mac = [0; 6];
        stats.last_error = Text::literal(b"flushed");
    }
}
/// # Safety
/// Exact paired resident/stack statistics, readable name/server and separate
/// writable outputs. Caller memory is never accessed under cache admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_cache_lookup(
    stats: *mut dns::Stats,
    data: *const u8,
    len: usize,
    server: *const [u8; 4],
    out: *mut [u8; 4],
    result: *mut i32,
) -> bool {
    let name = unsafe { Name::read(data, len) };
    let server = unsafe { *server };
    let value = {
        let _guard = admit();
        unsafe { dns_lookup_record(&mut *stats, &name, server) }
    };
    if let Some((answer, value)) = value {
        unsafe {
            out.write(answer);
            result.write(value);
        }
        true
    } else {
        false
    }
}
/// # Safety
/// Exact paired resident/stack statistics and readable name/fixed addresses.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_cache_store(
    stats: *mut dns::Stats,
    data: *const u8,
    len: usize,
    server: *const [u8; 4],
    answer: *const [u8; 4],
    result: i32,
) {
    let name = unsafe { Name::read(data, len) };
    let (server, answer) = unsafe { (*server, *answer) };
    let _guard = admit();
    unsafe {
        dns_store_record(&mut *stats, &name, server, answer, result);
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_cache_expire(stats: *mut dns::Stats) {
    let _guard = admit();
    unsafe {
        dns_expire_owned(&mut *stats);
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_cache_sync(stats: *mut dns::Stats) {
    let _guard = admit();
    unsafe {
        dns_sync_owned(&mut *stats);
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics and writable status output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_cache_status(stats: *mut dns::Stats, out: *mut DnsStatus) {
    let value = {
        let _guard = admit();
        unsafe { dns_status_owned(&mut *stats) }
    };
    unsafe {
        out.write(value);
    }
}
/// # Safety
/// Exact exclusive paired resident/stack statistics.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_dns_cache_count(stats: *mut dns::Stats, negative: bool) -> u32 {
    let _guard = admit();
    unsafe { dns_count_owned(&mut *stats, negative) }
}

const _: () = {
    assert!(core::mem::size_of::<DnsStatus>() == 16);
    assert!(core::mem::align_of::<DnsStatus>() == 4);
    assert!(core::mem::offset_of!(DnsStatus, valid) == 0);
    assert!(core::mem::offset_of!(DnsStatus, negative) == 1);
    assert!(core::mem::offset_of!(DnsStatus, age_seconds) == 4);
    assert!(core::mem::offset_of!(DnsStatus, ttl_seconds) == 8);
    assert!(core::mem::offset_of!(DnsStatus, remaining_seconds) == 12);
};

// SAFETY: these entries are internal to the network metadata owner. The
// caller holds admission class 2, statistics are exclusive, and all names,
// addresses and return values are resident owned values. No I/O or callback.
unsafe fn dns_lookup_record(
    stats: &mut dns::Stats,
    name: &Name,
    server: [u8; 4],
) -> Option<([u8; 4], i32)> {
    unsafe {
        let s = &mut *state();
        dns_expire(stats, s);
        dns_find(s, name, server).map(|i| {
            s.dns[i].hits += 1;
            stats.cache_hits += 1;
            let e = s.dns[i];
            dns_primary(stats, &e);
            (e.answer, e.result)
        })
    }
}
unsafe fn dns_store_record(
    stats: &mut dns::Stats,
    name: &Name,
    server: [u8; 4],
    answer: [u8; 4],
    result: i32,
) {
    let mut stored_name = name.bytes;
    stored_name[63] = 0;
    unsafe {
        let s = &mut *state();
        let i = dns_find(s, name, server).unwrap_or_else(|| dns_replace(s));
        s.dns[i] = DnsEntry {
            valid: true,
            negative: result == dns::RESULT_NXDOMAIN,
            name: stored_name,
            server,
            answer,
            result,
            updated_tick: now(),
            ttl_seconds: if result == dns::RESULT_NXDOMAIN {
                DNS_NEGATIVE_TTL
            } else {
                DNS_TTL
            },
            hits: 0,
        };
        stats.cache_stores += 1;
        dns_primary(stats, &s.dns[i]);
    }
}
pub(super) unsafe fn dns_lookup_owned(
    stats: &mut dns::Stats,
    name: &[u8],
    server: [u8; 4],
) -> Option<([u8; 4], i32)> {
    let name = unsafe { Name::read(name.as_ptr(), name.len()) };
    unsafe { dns_lookup_record(stats, &name, server) }
}
pub(super) unsafe fn dns_store_owned(
    stats: &mut dns::Stats,
    name: &[u8],
    server: [u8; 4],
    answer: [u8; 4],
    result: i32,
) {
    let name = unsafe { Name::read(name.as_ptr(), name.len()) };
    unsafe {
        dns_store_record(stats, &name, server, answer, result);
    }
}
pub(super) unsafe fn dns_expire_owned(stats: &mut dns::Stats) {
    unsafe {
        dns_expire(stats, &mut *state());
    }
}
pub(super) unsafe fn dns_sync_owned(stats: &mut dns::Stats) {
    unsafe {
        dns_sync(stats, &*state());
    }
}
pub(super) unsafe fn dns_status_owned(stats: &mut dns::Stats) -> DnsStatus {
    unsafe {
        let s = &mut *state();
        dns_expire(stats, s);
        dns_sync(stats, s);
        let mut value = DnsStatus {
            ttl_seconds: stats.cache_ttl_seconds,
            ..DnsStatus::EMPTY
        };
        if stats.cache_valid
            && stats.cache_updated_tick != 0
            && let Some(i) = dns_best(s)
        {
            let e = s.dns[i];
            let age = timing::elapsed_seconds_since(e.updated_tick);
            value = DnsStatus {
                valid: age < e.ttl_seconds,
                negative: e.negative,
                age_seconds: age,
                ttl_seconds: e.ttl_seconds,
                remaining_seconds: timing::seconds_remaining(e.ttl_seconds, age),
            };
        }
        value
    }
}
pub(super) unsafe fn dns_count_owned(stats: &mut dns::Stats, negative: bool) -> u32 {
    let s = unsafe { &mut *state() };
    dns_expire(stats, s);
    s.dns
        .iter()
        .filter(|e| e.valid && (!negative || e.negative))
        .count() as u32
}
