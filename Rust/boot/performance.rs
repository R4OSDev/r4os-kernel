//! Bounded boot observations and functional shell-completion state. All shared
//! records use the existing rank-70 diagnostic owner; it is released before
//! presentation/callback work. Clock capture/resolve here never waits or logs.
//! Raw field views retain reentrant fatal admission without a state reference.
use crate::{
    boot::owner::Guard,
    platform::{clock_math::OrderedDurations, monotonic as clock, timer},
};
use core::{cell::UnsafeCell, ptr};
pub const MAX_PHASES: usize = 32;
const MAX_SPANS: usize = 128;
pub const READY_RESULT_COMPLETED: i32 = 0;
pub const READY_RESULT_ALREADY_COMPLETED: i32 = 1;
pub const READY_ERROR_NOT_EXPECTED_SHELL: i32 = -1;
pub const READY_ERROR_BOOT_FAILED: i32 = -2;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PhaseInfo {
    pub phase: u8,
    pub first_tick: u64,
    pub last_tick: u64,
    pub total_ticks: u64,
    pub transitions: u32,
    pub first_ns: u64,
    pub last_ns: u64,
    pub total_ns: u64,
    pub timing_valid: bool,
    pub timing_unavailable_spans: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Summary {
    pub initialized: bool,
    pub state: u32,
    pub completion_reason: u32,
    pub boot_start_tick: u64,
    pub now_tick: u64,
    pub total_ticks: u64,
    pub phase_count: u32,
    pub transition_count: u64,
    pub current_phase: u8,
    pub now_ns: u64,
    pub total_ns: u64,
    pub clock_flags: u32,
    pub clock_source: u32,
    pub clock_generation: u32,
    pub clock_resolution_ns: u64,
    pub timing_valid: bool,
    pub timing_span_count: u32,
    pub timing_unavailable_spans: u32,
    pub timing_dropped_spans: u32,
    pub configured_attempts: u32,
    pub fallback_attempts: u32,
    pub launch_failures: u32,
    pub shell_instance_id: u32,
}

#[derive(Clone, Copy)]
struct Span {
    phase: u8,
    start: clock::Stamp,
    end: clock::Stamp,
}
struct State {
    initialized: bool,
    boot_start_tick: u64,
    phase_count: usize,
    transition_count: u64,
    current_phase: u8,
    current_enter_tick: u64,
    boot_start_stamp: clock::Stamp,
    current_enter_stamp: clock::Stamp,
    completion_state: u32,
    completion_reason: u32,
    boot_end_tick: u64,
    boot_end_stamp: clock::Stamp,
    completion_clock_flags: u32,
    completion_clock_source: u32,
    completion_clock_generation: u32,
    completion_clock_resolution_ns: u64,
    active_shell_kind: u32,
    expected_shell_instance_id: u32,
    configured_attempts: u32,
    fallback_attempts: u32,
    launch_failures: u32,
    phases: [PhaseInfo; MAX_PHASES],
    spans: [Span; MAX_SPANS],
    span_count: usize,
    timing_dropped_spans: u32,
}
struct Storage(UnsafeCell<State>);
// SAFETY: only the exact existing diagnostic owner admits field access. No
// reference escapes, and the bounded clock leaves cannot wait or call modules.
unsafe impl Sync for Storage {}
// SAFETY: State contains only integer/bool/stamp records; all-zero is the
// original uninitialized state, including unknown phases and empty spans.
static STATE: Storage = Storage(UnsafeCell::new(unsafe { core::mem::zeroed() }));
fn initialize(s: *mut State) {
    unsafe {
        ptr::write_bytes(s, 0, 1);
        (*s).initialized = true;
        (*s).boot_start_tick = timer::tick_count();
        (*s).boot_start_stamp = clock::capture();
        (*s).current_enter_tick = (*s).boot_start_tick;
        (*s).current_enter_stamp = (*s).boot_start_stamp;
        (*s).completion_state = 1;
        record_at(s, 1);
    }
}
fn ensure(s: *mut State) {
    if !unsafe { (*s).initialized } {
        initialize(s)
    }
}
fn find(s: *const State, phase: u8) -> Option<usize> {
    unsafe { (0..(*s).phase_count).find(|&i| ptr::addr_of!((*s).phases[i].phase).read() == phase) }
}
fn slot(s: *mut State, phase: u8) -> Option<usize> {
    unsafe {
        if let Some(i) = find(s, phase) {
            return Some(i);
        }
        if (*s).phase_count >= MAX_PHASES {
            return None;
        }
        let i = (*s).phase_count;
        (*s).phase_count += 1;
        Some(i)
    }
}
fn finish_current(s: *mut State, now: u64, stamp: clock::Stamp) {
    unsafe {
        if (*s).current_phase == 0 {
            return;
        }
        let Some(i) = find(s, (*s).current_phase) else {
            return;
        };
        let p = ptr::addr_of_mut!((*s).phases[i]);
        if now >= (*s).current_enter_tick {
            (*p).total_ticks = (*p).total_ticks.wrapping_add(now - (*s).current_enter_tick)
        }
        (*p).last_tick = now;
        if (*s).span_count < MAX_SPANS {
            let span = ptr::addr_of_mut!((*s).spans[(*s).span_count]);
            span.write(Span {
                phase: (*s).current_phase,
                start: (*s).current_enter_stamp,
                end: stamp,
            });
            (*s).span_count += 1;
        } else {
            (*s).timing_dropped_spans = (*s).timing_dropped_spans.saturating_add(1)
        }
    }
}
fn record_at(s: *mut State, phase: u8) {
    unsafe {
        if !(*s).initialized {
            initialize(s)
        }
        if (*s).completion_state != 1 {
            return;
        }
        let now = timer::tick_count();
        let stamp = clock::capture();
        finish_current(s, now, stamp);
        (*s).transition_count = (*s).transition_count.wrapping_add(1);
        (*s).current_phase = phase;
        (*s).current_enter_tick = now;
        (*s).current_enter_stamp = stamp;
        let Some(i) = slot(s, phase) else { return };
        let p = ptr::addr_of_mut!((*s).phases[i]);
        if (*p).transitions == 0 {
            (*p).phase = phase;
            (*p).first_tick = now
        }
        (*p).last_tick = now;
        (*p).transitions = (*p).transitions.wrapping_add(1);
    }
}
fn freeze(s: *mut State, state: u32, reason: u32) {
    unsafe {
        let now = timer::tick_count();
        let stamp = clock::capture();
        finish_current(s, now, stamp);
        let c = clock::snapshot();
        (*s).boot_end_tick = now;
        (*s).boot_end_stamp = stamp;
        (*s).completion_clock_flags = c.flags;
        (*s).completion_clock_source = c.source;
        (*s).completion_clock_generation = c.generation;
        (*s).completion_clock_resolution_ns = c.resolution_ns;
        (*s).completion_state = state;
        (*s).completion_reason = reason;
    }
}
pub fn init() {
    let _g = Guard::acquire(0);
    initialize(STATE.0.get())
}
pub fn record(phase: u8) {
    let _g = Guard::acquire(0);
    record_at(STATE.0.get(), phase)
}
pub fn begin_shell_attempt(kind: u32) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        if (*s).completion_state != 1 {
            return;
        }
        (*s).active_shell_kind = kind;
        (*s).expected_shell_instance_id = 0;
        match kind {
            1 => (*s).configured_attempts = (*s).configured_attempts.saturating_add(1),
            2 | 3 => (*s).fallback_attempts = (*s).fallback_attempts.saturating_add(1),
            0 => {}
            _ => unreachable!(),
        }
    }
}
pub fn note_shell_launched(id: u32) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    unsafe {
        if (*s).completion_state == 1 && (*s).active_shell_kind != 0 && id != 0 {
            (*s).expected_shell_instance_id = id
        }
    }
}
pub fn note_shell_launch_failure() {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    unsafe {
        if (*s).completion_state == 1 {
            (*s).launch_failures = (*s).launch_failures.saturating_add(1);
            (*s).expected_shell_instance_id = 0
        }
    }
}
pub fn complete_ready(id: u32) -> i32 {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    unsafe {
        match (*s).completion_state {
            2 | 3 => {
                return if id != 0 && id == (*s).expected_shell_instance_id {
                    READY_RESULT_ALREADY_COMPLETED
                } else {
                    READY_ERROR_NOT_EXPECTED_SHELL
                };
            }
            4 => return READY_ERROR_BOOT_FAILED,
            0 | 1 => {}
            _ => unreachable!(),
        }
        if (*s).active_shell_kind == 0 || id == 0 || id != (*s).expected_shell_instance_id {
            return READY_ERROR_NOT_EXPECTED_SHELL;
        }
        let (state, reason) = match (*s).active_shell_kind {
            1 => (2, 1),
            2 => (3, 2),
            3 => (3, 3),
            _ => unreachable!(),
        };
        freeze(s, state, reason);
        READY_RESULT_COMPLETED
    }
}
fn fail(reason: u32) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    ensure(s);
    unsafe {
        if (*s).completion_state == 1 {
            freeze(s, 4, reason)
        }
    }
}
pub fn fail_no_shell() {
    fail(4)
}
pub fn fail_fatal() {
    fail(5)
}
pub fn fail_shell_exited(id: u32) {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    unsafe {
        if (*s).completion_state == 1 && id != 0 && id == (*s).expected_shell_instance_id {
            freeze(s, 4, 6)
        }
    }
}
fn unavailable(s: *const State, stamp: clock::Stamp, running: bool) -> u32 {
    unsafe {
        let mut count = 0u32;
        for i in 0..(*s).span_count {
            let span = ptr::addr_of!((*s).spans[i]).read();
            if clock::elapsed(span.start, span.end).is_none() {
                count = count.saturating_add(1)
            }
        }
        if running
            && (*s).current_phase != 0
            && clock::elapsed((*s).current_enter_stamp, stamp).is_none()
        {
            count = count.saturating_add(1)
        }
        count
    }
}
pub fn snapshot() -> Summary {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    unsafe {
        let running = (*s).completion_state == 1;
        let now = if running {
            timer::tick_count()
        } else {
            (*s).boot_end_tick
        };
        let stamp = if running {
            clock::capture()
        } else {
            (*s).boot_end_stamp
        };
        let c = clock::snapshot();
        let elapsed = clock::elapsed((*s).boot_start_stamp, stamp);
        Summary {
            initialized: (*s).initialized,
            state: (*s).completion_state,
            completion_reason: (*s).completion_reason,
            boot_start_tick: (*s).boot_start_tick,
            now_tick: now,
            total_ticks: now.saturating_sub((*s).boot_start_tick),
            phase_count: (*s).phase_count.min(u32::MAX as usize) as u32,
            transition_count: (*s).transition_count,
            current_phase: (*s).current_phase,
            now_ns: clock::resolve(stamp).unwrap_or(0),
            total_ns: elapsed.unwrap_or(0),
            clock_flags: if running {
                c.flags
            } else {
                (*s).completion_clock_flags
            },
            clock_source: if running {
                c.source
            } else {
                (*s).completion_clock_source
            },
            clock_generation: if running {
                c.generation
            } else {
                (*s).completion_clock_generation
            },
            clock_resolution_ns: if running {
                c.resolution_ns
            } else {
                (*s).completion_clock_resolution_ns
            },
            timing_valid: elapsed.is_some(),
            timing_span_count: ((*s).span_count + usize::from(running && (*s).current_phase != 0))
                .min(u32::MAX as usize) as u32,
            timing_unavailable_spans: unavailable(s, stamp, running),
            timing_dropped_spans: (*s).timing_dropped_spans,
            configured_attempts: (*s).configured_attempts,
            fallback_attempts: (*s).fallback_attempts,
            launch_failures: (*s).launch_failures,
            shell_instance_id: (*s).expected_shell_instance_id,
        }
    }
}
pub fn phase_at(index: u32) -> Option<PhaseInfo> {
    let _g = Guard::acquire(0);
    let s = STATE.0.get();
    unsafe {
        let i = index as usize;
        if i >= (*s).phase_count {
            return None;
        }
        let mut out = ptr::addr_of!((*s).phases[i]).read();
        let running = (*s).completion_state == 1;
        if out.phase == (*s).current_phase && running {
            let now = timer::tick_count();
            if now >= (*s).current_enter_tick {
                out.total_ticks = out.total_ticks.wrapping_add(now - (*s).current_enter_tick)
            }
            out.last_tick = now;
        }
        let stamp = if running {
            clock::capture()
        } else {
            (*s).boot_end_stamp
        };
        let mut timing = OrderedDurations::default();
        for n in 0..(*s).span_count {
            let span = ptr::addr_of!((*s).spans[n]).read();
            if span.phase == out.phase {
                timing.add(clock::resolve(span.start), clock::resolve(span.end))
            }
        }
        if out.phase == (*s).current_phase && running {
            timing.add(
                clock::resolve((*s).current_enter_stamp),
                clock::resolve(stamp),
            )
        }
        out.first_ns = if timing.has_first { timing.first } else { 0 };
        out.last_ns = if timing.has_last { timing.last } else { 0 };
        out.total_ns = timing.total;
        out.timing_unavailable_spans = timing.unavailable;
        out.timing_valid = timing.valid();
        Some(out)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_performance_control(op: u8, value: u32) -> i32 {
    match op {
        0 => init(),
        1 => record(value.try_into().expect("bootphase")),
        2 => begin_shell_attempt(value),
        3 => note_shell_launched(value),
        4 => note_shell_launch_failure(),
        5 => return complete_ready(value),
        6 => fail_no_shell(),
        7 => fail_fatal(),
        8 => fail_shell_exited(value),
        _ => unreachable!(),
    }
    0
}
/// # Safety
/// Caller retains one writable private C Summary for this bounded snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_performance_snapshot(out: *mut Summary) {
    unsafe { out.write(snapshot()) }
}
/// # Safety
/// Caller owns one writable PhaseInfo; false never publishes uninitialized data.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_performance_phase(index: u32, out: *mut PhaseInfo) -> bool {
    if let Some(value) = phase_at(index) {
        unsafe { out.write(value) };
        true
    } else {
        false
    }
}

const _: () = {
    assert!(core::mem::size_of::<PhaseInfo>() == 72);
    assert!(core::mem::align_of::<PhaseInfo>() == 8);
    assert!(core::mem::offset_of!(PhaseInfo, phase) == 0);
    assert!(core::mem::offset_of!(PhaseInfo, first_tick) == 8);
    assert!(core::mem::offset_of!(PhaseInfo, last_tick) == 16);
    assert!(core::mem::offset_of!(PhaseInfo, total_ticks) == 24);
    assert!(core::mem::offset_of!(PhaseInfo, transitions) == 32);
    assert!(core::mem::offset_of!(PhaseInfo, first_ns) == 40);
    assert!(core::mem::offset_of!(PhaseInfo, last_ns) == 48);
    assert!(core::mem::offset_of!(PhaseInfo, total_ns) == 56);
    assert!(core::mem::offset_of!(PhaseInfo, timing_valid) == 64);
    assert!(core::mem::offset_of!(PhaseInfo, timing_unavailable_spans) == 68);
    assert!(core::mem::size_of::<Summary>() == 136);
    assert!(core::mem::align_of::<Summary>() == 8);
    assert!(core::mem::offset_of!(Summary, initialized) == 0);
    assert!(core::mem::offset_of!(Summary, state) == 4);
    assert!(core::mem::offset_of!(Summary, completion_reason) == 8);
    assert!(core::mem::offset_of!(Summary, boot_start_tick) == 16);
    assert!(core::mem::offset_of!(Summary, now_tick) == 24);
    assert!(core::mem::offset_of!(Summary, total_ticks) == 32);
    assert!(core::mem::offset_of!(Summary, phase_count) == 40);
    assert!(core::mem::offset_of!(Summary, transition_count) == 48);
    assert!(core::mem::offset_of!(Summary, current_phase) == 56);
    assert!(core::mem::offset_of!(Summary, now_ns) == 64);
    assert!(core::mem::offset_of!(Summary, total_ns) == 72);
    assert!(core::mem::offset_of!(Summary, clock_flags) == 80);
    assert!(core::mem::offset_of!(Summary, clock_source) == 84);
    assert!(core::mem::offset_of!(Summary, clock_generation) == 88);
    assert!(core::mem::offset_of!(Summary, clock_resolution_ns) == 96);
    assert!(core::mem::offset_of!(Summary, timing_valid) == 104);
    assert!(core::mem::offset_of!(Summary, timing_span_count) == 108);
    assert!(core::mem::offset_of!(Summary, timing_unavailable_spans) == 112);
    assert!(core::mem::offset_of!(Summary, timing_dropped_spans) == 116);
    assert!(core::mem::offset_of!(Summary, configured_attempts) == 120);
    assert!(core::mem::offset_of!(Summary, fallback_attempts) == 124);
    assert!(core::mem::offset_of!(Summary, launch_failures) == 128);
    assert!(core::mem::offset_of!(Summary, shell_instance_id) == 132);
};
