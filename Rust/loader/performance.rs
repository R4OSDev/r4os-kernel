//! Resident bounded loader observations. The existing diagnostic owner
//! serializes snapshots/updates; clock leaves cannot wait or call modules.
use crate::{
    boot::owner::Guard,
    platform::{monotonic as clock, timer},
};
use core::{cell::UnsafeCell, ptr};
pub const SERVICE_BOOT_NOT_ATTEMPTED: u32 = 0;
pub const SERVICE_BOOT_RAN: u32 = 1;
pub const SERVICE_BOOT_NOT_FOUND: u32 = 2;
pub const SERVICE_BOOT_FAILED: u32 = 3;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Summary {
    pub initialized: u32,
    pub loader_started: u32,
    pub loader_completed: u32,
    pub r4p_runtime_started: u32,
    pub r4p_runtime_completed: u32,
    pub service_boot_status: u32,
    pub boot_critical_count: u32,
    pub lazy_candidate_count: u32,
    pub loader_total_ticks: u64,
    pub r4p_runtime_total_ticks: u64,
    pub service_boot_ticks: u64,
    pub config_load_ticks: u64,
    pub config_bytes: u64,
    pub config_driver_count: u32,
    pub config_disabled_count: u32,
    pub config_option_count: u32,
    pub config_reserved0: u32,
    pub r4l_scan_entries: u64,
    pub r4l_candidates: u64,
    pub r4l_loaded: u64,
    pub r4l_failed: u64,
    pub r4l_scan_ticks: u64,
    pub r4l_read_ticks: u64,
    pub r4l_resolve_ticks: u64,
    pub r4l_load_ticks: u64,
    pub r4d_scan_entries: u64,
    pub r4d_candidates: u64,
    pub r4d_discovered: u64,
    pub r4d_failed: u64,
    pub r4d_scan_ticks: u64,
    pub r4d_read_ticks: u64,
    pub r4d_probe_ticks: u64,
    pub r4p_scan_entries: u64,
    pub r4p_candidates: u64,
    pub r4p_discovered: u64,
    pub r4p_active: u64,
    pub r4p_blocked: u64,
    pub r4p_failed: u64,
    pub r4p_scan_ticks: u64,
    pub r4p_read_ticks: u64,
    pub r4p_resolve_ticks: u64,
    pub r4p_init_ticks: u64,
    pub clock_flags: u32,
    pub clock_source: u32,
    pub clock_generation: u32,
    pub timing_valid_spans: u32,
    pub timing_unavailable_spans: u32,
    pub timing_reserved0: u32,
    pub clock_resolution_ns: u64,
    pub loader_total_ns: u64,
    pub r4p_runtime_total_ns: u64,
    pub service_boot_ns: u64,
    pub config_load_ns: u64,
}

struct State {
    summary: Summary,
    filesystem_start: u64,
    r4p_start: u64,
    filesystem_stamp: clock::Stamp,
    r4p_stamp: clock::Stamp,
    service_stamp: clock::Stamp,
    config_stamp: clock::Stamp,
}
struct Storage(UnsafeCell<State>);
// SAFETY: every field view is bounded by the existing diagnostic owner. No
// reference to this state crosses a wait, callback, module or I/O operation.
unsafe impl Sync for Storage {}
// SAFETY: State contains only integer/stamp records with valid all-zero values.
static STATE: Storage = Storage(UnsafeCell::new(unsafe { core::mem::zeroed() }));
fn initialize(s: *mut State) {
    unsafe {
        ptr::write_bytes(s, 0, 1);
        (*s).summary.initialized = 1;
        (*s).summary.boot_critical_count = 8;
        (*s).summary.lazy_candidate_count = 9
    }
}
fn ensure(s: *mut State) {
    if unsafe { (*s).summary.initialized } == 0 {
        initialize(s)
    }
}
pub fn init() {
    let _g = Guard::acquire(0);
    initialize(STATE.0.get())
}
pub fn snapshot() -> Summary {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        let mut out = ptr::addr_of!((*s).summary).read();
        let c = clock::snapshot();
        out.clock_flags = c.flags;
        out.clock_source = c.source;
        out.clock_generation = c.generation;
        out.clock_resolution_ns = c.resolution_ns;
        out
    }
}
pub fn now() -> u64 {
    timer::tick_count()
}
pub fn elapsed_since(start: u64) -> u64 {
    now().saturating_sub(start)
}
fn finish_span(s: *mut State, start: clock::Stamp, destination: *mut u64) {
    unsafe {
        let c = clock::snapshot();
        if c.flags & crate::contract::monotonic_clock_flag_high_resolution == 0 {
            (*s).summary.timing_unavailable_spans =
                (*s).summary.timing_unavailable_spans.saturating_add(1);
            return;
        }
        let Some(elapsed) = clock::elapsed(start, clock::capture()) else {
            (*s).summary.timing_unavailable_spans =
                (*s).summary.timing_unavailable_spans.saturating_add(1);
            return;
        };
        destination.write(destination.read().saturating_add(elapsed));
        (*s).summary.timing_valid_spans = (*s).summary.timing_valid_spans.saturating_add(1);
    }
}
pub fn begin_filesystem_loader() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.loader_started = 1;
        (*s).filesystem_start = now();
        (*s).filesystem_stamp = clock::capture();
    }
}
pub fn finish_filesystem_loader() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.loader_completed = 1;
        (*s).summary.loader_total_ticks = (*s)
            .summary
            .loader_total_ticks
            .wrapping_add(elapsed_since((*s).filesystem_start));
        finish_span(
            s,
            (*s).filesystem_stamp,
            ptr::addr_of_mut!((*s).summary.loader_total_ns),
        );
    }
}
pub fn begin_r4p_runtime() -> u64 {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4p_runtime_started = 1;
        (*s).r4p_start = now();
        (*s).r4p_stamp = clock::capture();
        (*s).r4p_start
    }
}
pub fn finish_r4p_runtime(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4p_runtime_completed = 1;
        (*s).summary.r4p_runtime_total_ticks = (*s)
            .summary
            .r4p_runtime_total_ticks
            .wrapping_add(elapsed_since(start));
        finish_span(
            s,
            (*s).r4p_stamp,
            ptr::addr_of_mut!((*s).summary.r4p_runtime_total_ns),
        );
    }
}
pub fn begin_service_boot() -> u64 {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.service_boot_status = SERVICE_BOOT_NOT_ATTEMPTED;
        (*s).service_stamp = clock::capture();
        now()
    }
}
pub fn finish_service_boot(start: u64, status: u32) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.service_boot_ticks = (*s)
            .summary
            .service_boot_ticks
            .wrapping_add(elapsed_since(start));
        finish_span(
            s,
            (*s).service_stamp,
            ptr::addr_of_mut!((*s).summary.service_boot_ns),
        );
        (*s).summary.service_boot_status = status;
    }
}
pub fn begin_config_load() -> u64 {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).config_stamp = clock::capture();
        now()
    }
}
pub fn record_config_load(
    start: u64,
    bytes: usize,
    drivers: usize,
    disabled: usize,
    options: usize,
) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.config_load_ticks = (*s)
            .summary
            .config_load_ticks
            .wrapping_add(elapsed_since(start));
        finish_span(
            s,
            (*s).config_stamp,
            ptr::addr_of_mut!((*s).summary.config_load_ns),
        );
        (*s).summary.config_bytes = bytes as u64;
        (*s).summary.config_driver_count = clamp32(drivers);
        (*s).summary.config_disabled_count = clamp32(disabled);
        (*s).summary.config_option_count = clamp32(options);
    }
}
fn clamp32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}
pub fn record_r4l_scan_entry() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe { (*s).summary.r4l_scan_entries = (*s).summary.r4l_scan_entries.wrapping_add(1) }
}
pub fn record_r4l_candidate() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe { (*s).summary.r4l_candidates = (*s).summary.r4l_candidates.wrapping_add(1) }
}
pub fn add_r4l_scan_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4l_scan_ticks = (*s)
            .summary
            .r4l_scan_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4l_read_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4l_read_ticks = (*s)
            .summary
            .r4l_read_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4l_resolve_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4l_resolve_ticks = (*s)
            .summary
            .r4l_resolve_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4l_load_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4l_load_ticks = (*s)
            .summary
            .r4l_load_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn record_r4d_scan_entry() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe { (*s).summary.r4d_scan_entries = (*s).summary.r4d_scan_entries.wrapping_add(1) }
}
pub fn record_r4d_candidate() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe { (*s).summary.r4d_candidates = (*s).summary.r4d_candidates.wrapping_add(1) }
}
pub fn add_r4d_scan_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4d_scan_ticks = (*s)
            .summary
            .r4d_scan_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4d_read_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4d_read_ticks = (*s)
            .summary
            .r4d_read_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4d_probe_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4d_probe_ticks = (*s)
            .summary
            .r4d_probe_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn record_r4p_scan_entry() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe { (*s).summary.r4p_scan_entries = (*s).summary.r4p_scan_entries.wrapping_add(1) }
}
pub fn record_r4p_candidate() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe { (*s).summary.r4p_candidates = (*s).summary.r4p_candidates.wrapping_add(1) }
}
pub fn add_r4p_scan_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4p_scan_ticks = (*s)
            .summary
            .r4p_scan_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4p_read_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4p_read_ticks = (*s)
            .summary
            .r4p_read_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4p_resolve_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4p_resolve_ticks = (*s)
            .summary
            .r4p_resolve_ticks
            .wrapping_add(elapsed_since(start))
    }
}
pub fn add_r4p_init_ticks(start: u64) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4p_init_ticks = (*s)
            .summary
            .r4p_init_ticks
            .wrapping_add(elapsed_since(start))
    }
}

pub fn record_r4l_results(loaded: usize, candidates: usize) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4l_loaded = loaded as u64;
        (*s).summary.r4l_failed = candidates.saturating_sub(loaded) as u64
    }
}
pub fn record_r4d_results(discovered: usize) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4d_discovered = discovered as u64;
        (*s).summary.r4d_failed = (*s)
            .summary
            .r4d_candidates
            .saturating_sub(discovered as u64)
    }
}
pub fn record_r4p_results(discovered: usize, active: usize, blocked: usize, failed: usize) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        (*s).summary.r4p_discovered = discovered as u64;
        (*s).summary.r4p_active = active as u64;
        (*s).summary.r4p_blocked = blocked as u64;
        (*s).summary.r4p_failed = failed as u64
    }
}
/// # Safety
/// Caller owns one aligned writable private C Summary until this call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_loader_performance_snapshot(out: *mut Summary) {
    unsafe { out.write(snapshot()) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_loader_performance_control(
    op: u8,
    a: u64,
    b: u64,
    c: u64,
    d: u64,
    e: u64,
) -> u64 {
    match op {
        0 => init(),
        1 => begin_filesystem_loader(),
        2 => finish_filesystem_loader(),
        3 => return begin_r4p_runtime(),
        4 => finish_r4p_runtime(a),
        5 => return begin_service_boot(),
        6 => finish_service_boot(a, b as u32),
        7 => return begin_config_load(),
        8 => record_config_load(a, b as usize, c as usize, d as usize, e as usize),
        9 => record_r4l_scan_entry(),
        10 => record_r4l_candidate(),
        11 => add_r4l_scan_ticks(a),
        12 => add_r4l_read_ticks(a),
        13 => add_r4l_resolve_ticks(a),
        14 => add_r4l_load_ticks(a),
        15 => record_r4d_scan_entry(),
        16 => record_r4d_candidate(),
        17 => add_r4d_scan_ticks(a),
        18 => add_r4d_read_ticks(a),
        19 => add_r4d_probe_ticks(a),
        20 => record_r4p_scan_entry(),
        21 => record_r4p_candidate(),
        22 => add_r4p_scan_ticks(a),
        23 => add_r4p_read_ticks(a),
        24 => add_r4p_resolve_ticks(a),
        25 => add_r4p_init_ticks(a),
        26 => record_r4l_results(a as usize, b as usize),
        27 => record_r4d_results(a as usize),
        28 => record_r4p_results(a as usize, b as usize, c as usize, d as usize),
        29 => return now(),
        30 => return elapsed_since(a),
        _ => unreachable!(),
    }
    0
}

const _: () = {
    assert!(core::mem::size_of::<Summary>() == 352);
    assert!(core::mem::align_of::<Summary>() == 8);
    assert!(core::mem::offset_of!(Summary, initialized) == 0);
    assert!(core::mem::offset_of!(Summary, loader_started) == 4);
    assert!(core::mem::offset_of!(Summary, loader_completed) == 8);
    assert!(core::mem::offset_of!(Summary, r4p_runtime_started) == 12);
    assert!(core::mem::offset_of!(Summary, r4p_runtime_completed) == 16);
    assert!(core::mem::offset_of!(Summary, service_boot_status) == 20);
    assert!(core::mem::offset_of!(Summary, boot_critical_count) == 24);
    assert!(core::mem::offset_of!(Summary, lazy_candidate_count) == 28);
    assert!(core::mem::offset_of!(Summary, loader_total_ticks) == 32);
    assert!(core::mem::offset_of!(Summary, r4p_runtime_total_ticks) == 40);
    assert!(core::mem::offset_of!(Summary, service_boot_ticks) == 48);
    assert!(core::mem::offset_of!(Summary, config_load_ticks) == 56);
    assert!(core::mem::offset_of!(Summary, config_bytes) == 64);
    assert!(core::mem::offset_of!(Summary, config_driver_count) == 72);
    assert!(core::mem::offset_of!(Summary, config_disabled_count) == 76);
    assert!(core::mem::offset_of!(Summary, config_option_count) == 80);
    assert!(core::mem::offset_of!(Summary, config_reserved0) == 84);
    assert!(core::mem::offset_of!(Summary, r4l_scan_entries) == 88);
    assert!(core::mem::offset_of!(Summary, r4l_candidates) == 96);
    assert!(core::mem::offset_of!(Summary, r4l_loaded) == 104);
    assert!(core::mem::offset_of!(Summary, r4l_failed) == 112);
    assert!(core::mem::offset_of!(Summary, r4l_scan_ticks) == 120);
    assert!(core::mem::offset_of!(Summary, r4l_read_ticks) == 128);
    assert!(core::mem::offset_of!(Summary, r4l_resolve_ticks) == 136);
    assert!(core::mem::offset_of!(Summary, r4l_load_ticks) == 144);
    assert!(core::mem::offset_of!(Summary, r4d_scan_entries) == 152);
    assert!(core::mem::offset_of!(Summary, r4d_candidates) == 160);
    assert!(core::mem::offset_of!(Summary, r4d_discovered) == 168);
    assert!(core::mem::offset_of!(Summary, r4d_failed) == 176);
    assert!(core::mem::offset_of!(Summary, r4d_scan_ticks) == 184);
    assert!(core::mem::offset_of!(Summary, r4d_read_ticks) == 192);
    assert!(core::mem::offset_of!(Summary, r4d_probe_ticks) == 200);
    assert!(core::mem::offset_of!(Summary, r4p_scan_entries) == 208);
    assert!(core::mem::offset_of!(Summary, r4p_candidates) == 216);
    assert!(core::mem::offset_of!(Summary, r4p_discovered) == 224);
    assert!(core::mem::offset_of!(Summary, r4p_active) == 232);
    assert!(core::mem::offset_of!(Summary, r4p_blocked) == 240);
    assert!(core::mem::offset_of!(Summary, r4p_failed) == 248);
    assert!(core::mem::offset_of!(Summary, r4p_scan_ticks) == 256);
    assert!(core::mem::offset_of!(Summary, r4p_read_ticks) == 264);
    assert!(core::mem::offset_of!(Summary, r4p_resolve_ticks) == 272);
    assert!(core::mem::offset_of!(Summary, r4p_init_ticks) == 280);
    assert!(core::mem::offset_of!(Summary, clock_flags) == 288);
    assert!(core::mem::offset_of!(Summary, clock_source) == 292);
    assert!(core::mem::offset_of!(Summary, clock_generation) == 296);
    assert!(core::mem::offset_of!(Summary, timing_valid_spans) == 300);
    assert!(core::mem::offset_of!(Summary, timing_unavailable_spans) == 304);
    assert!(core::mem::offset_of!(Summary, timing_reserved0) == 308);
    assert!(core::mem::offset_of!(Summary, clock_resolution_ns) == 312);
    assert!(core::mem::offset_of!(Summary, loader_total_ns) == 320);
    assert!(core::mem::offset_of!(Summary, r4p_runtime_total_ns) == 328);
    assert!(core::mem::offset_of!(Summary, service_boot_ns) == 336);
    assert!(core::mem::offset_of!(Summary, config_load_ns) == 344);
};
