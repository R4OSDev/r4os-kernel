//! Bounded pressure orchestration. No table borrow or metadata lock is
//! held over cache drain, VM page-out, task-stack reclaim or a wait.
use super::phys;
use crate::{
    fs::{page_cache, page_cache_writeback},
    platform::timer,
    sched::task_context,
};
use core::sync::atomic::{
    AtomicBool, AtomicU32, AtomicU64,
    Ordering::{Acquire, Relaxed, Release},
};
pub const DIAGNOSTIC: u32 = 1;
const STACK_BYTES: u64 = 64 * 1024;
const STACK_FRAMES: u32 = (STACK_BYTES / 4096) as u32;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Result {
    pub reason: u32,
    pub requested_frames: u32,
    pub returned_frames: u32,
    pub returned_bytes: u64,
    pub fs_returned_frames: u32,
    pub fs_returned_bytes: u64,
    pub task_stack_returned_frames: u32,
    pub task_stack_returned_bytes: u64,
    pub vm_returned_frames: u32,
    pub vm_returned_bytes: u64,
    pub vm_page_outs: u64,
    pub vm_failures: u64,
    pub dirty_drains: u64,
    pub failed_drains: u64,
    pub before_free_frames: u64,
    pub after_free_frames: u64,
    pub elapsed_ticks: u64,
}
impl Result {
    pub const INIT: Self = Self {
        reason: DIAGNOSTIC,
        requested_frames: 0,
        returned_frames: 0,
        returned_bytes: 0,
        fs_returned_frames: 0,
        fs_returned_bytes: 0,
        task_stack_returned_frames: 0,
        task_stack_returned_bytes: 0,
        vm_returned_frames: 0,
        vm_returned_bytes: 0,
        vm_page_outs: 0,
        vm_failures: 0,
        dirty_drains: 0,
        failed_drains: 0,
        before_free_frames: 0,
        after_free_frames: 0,
        elapsed_ticks: 0,
    };
}
impl Default for Result {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SourceResult {
    pub returned_frames: u32,
    pub returned_bytes: u64,
    pub page_outs: u64,
    pub failures: u64,
}
impl SourceResult {
    pub const INIT: Self = Self {
        returned_frames: 0,
        returned_bytes: 0,
        page_outs: 0,
        failures: 0,
    };
}
impl Default for SourceResult {
    fn default() -> Self {
        Self::INIT
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Summary {
    pub enabled: bool,
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub requested_frames: u64,
    pub returned_frames: u64,
    pub returned_bytes: u64,
    pub fs_returned_frames: u64,
    pub fs_returned_bytes: u64,
    pub task_stack_returned_frames: u64,
    pub task_stack_returned_bytes: u64,
    pub vm_returned_frames: u64,
    pub vm_returned_bytes: u64,
    pub vm_page_outs: u64,
    pub vm_failures: u64,
    pub dirty_drains: u64,
    pub failed_drains: u64,
    pub total_ticks: u64,
    pub max_ticks: u64,
    pub last_ticks: u64,
    pub last_reason: u32,
    pub last_requested_frames: u32,
    pub last_returned_frames: u32,
    pub last_before_free_frames: u64,
    pub last_after_free_frames: u64,
}
impl Summary {
    pub const INIT: Self = Self {
        enabled: true,
        attempts: 0,
        successes: 0,
        failures: 0,
        requested_frames: 0,
        returned_frames: 0,
        returned_bytes: 0,
        fs_returned_frames: 0,
        fs_returned_bytes: 0,
        task_stack_returned_frames: 0,
        task_stack_returned_bytes: 0,
        vm_returned_frames: 0,
        vm_returned_bytes: 0,
        vm_page_outs: 0,
        vm_failures: 0,
        dirty_drains: 0,
        failed_drains: 0,
        total_ticks: 0,
        max_ticks: 0,
        last_ticks: 0,
        last_reason: 0,
        last_requested_frames: 0,
        last_returned_frames: 0,
        last_before_free_frames: 0,
        last_after_free_frames: 0,
    };
}
impl Default for Summary {
    fn default() -> Self {
        Self::INIT
    }
}
struct Counters {
    attempts: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
    requested_frames: AtomicU64,
    returned_frames: AtomicU64,
    returned_bytes: AtomicU64,
    fs_returned_frames: AtomicU64,
    fs_returned_bytes: AtomicU64,
    task_stack_returned_frames: AtomicU64,
    task_stack_returned_bytes: AtomicU64,
    vm_returned_frames: AtomicU64,
    vm_returned_bytes: AtomicU64,
    vm_page_outs: AtomicU64,
    vm_failures: AtomicU64,
    dirty_drains: AtomicU64,
    failed_drains: AtomicU64,
    total_ticks: AtomicU64,
    max_ticks: AtomicU64,
    last_ticks: AtomicU64,
    last_reason: AtomicU32,
    last_requested_frames: AtomicU32,
    last_returned_frames: AtomicU32,
    last_before_free_frames: AtomicU64,
    last_after_free_frames: AtomicU64,
}
static COUNTERS: Counters = Counters {
    attempts: AtomicU64::new(0),
    successes: AtomicU64::new(0),
    failures: AtomicU64::new(0),
    requested_frames: AtomicU64::new(0),
    returned_frames: AtomicU64::new(0),
    returned_bytes: AtomicU64::new(0),
    fs_returned_frames: AtomicU64::new(0),
    fs_returned_bytes: AtomicU64::new(0),
    task_stack_returned_frames: AtomicU64::new(0),
    task_stack_returned_bytes: AtomicU64::new(0),
    vm_returned_frames: AtomicU64::new(0),
    vm_returned_bytes: AtomicU64::new(0),
    vm_page_outs: AtomicU64::new(0),
    vm_failures: AtomicU64::new(0),
    dirty_drains: AtomicU64::new(0),
    failed_drains: AtomicU64::new(0),
    total_ticks: AtomicU64::new(0),
    max_ticks: AtomicU64::new(0),
    last_ticks: AtomicU64::new(0),
    last_reason: AtomicU32::new(0),
    last_requested_frames: AtomicU32::new(0),
    last_returned_frames: AtomicU32::new(0),
    last_before_free_frames: AtomicU64::new(0),
    last_after_free_frames: AtomicU64::new(0),
};
static IN_RECLAIM: AtomicBool = AtomicBool::new(false);
static VM_RECLAIMER: AtomicBool = AtomicBool::new(false);
static STACK_RECLAIMER: AtomicBool = AtomicBool::new(false);
fn free_frames() -> u64 {
    let mut stats = phys::Stats::default();
    unsafe { phys::r4os_rust_phys_stats(&mut stats) };
    stats.free_frames
}
fn elapsed(start: u64) -> u64 {
    timer::tick_count().saturating_sub(start)
}
unsafe fn reclaim(reason: u32, raw_requested: u32, vm_owner: Option<u64>) -> Result {
    let requested = if raw_requested == 0 { 1 } else { raw_requested };
    let before = free_frames();
    let start = timer::tick_count();
    let unwind = unsafe { task_context::enter() };
    if !unwind.admitted() {
        let result = Result {
            reason,
            requested_frames: requested,
            before_free_frames: before,
            after_free_frames: before,
            elapsed_ticks: elapsed(start),
            ..Result::INIT
        };
        record(result);
        return result;
    }
    // One bounded pressure attempt owns the original global recursion gate.
    // Atomic admission also makes concurrent BSP/AP attempts retain that
    // single owner; no mutable counter reference survives a callback.
    if IN_RECLAIM
        .compare_exchange(false, true, Acquire, Relaxed)
        .is_err()
    {
        let result = Result {
            reason,
            requested_frames: requested,
            before_free_frames: before,
            after_free_frames: before,
            elapsed_ticks: elapsed(start),
            ..Result::INIT
        };
        record(result);
        unsafe { task_context::leave(unwind) };
        return result;
    }
    let _ = unsafe { crate::fs::ntfs_adapter::reclaim(requested) };
    let mut stack = SourceResult::INIT;
    let stack_reclaimer = STACK_RECLAIMER.load(Acquire);
    if stack_reclaimer {
        let requested_stacks = requested.saturating_add(STACK_FRAMES - 1) / STACK_FRAMES;
        let returned_stacks = unsafe {
            crate::sched::task::task_api::r4os_rust_task_control(17, u64::from(requested_stacks), 0)
        } as u32;
        stack.returned_frames = returned_stacks.saturating_mul(STACK_FRAMES);
        stack.returned_bytes = u64::from(returned_stacks).saturating_mul(STACK_BYTES);
    }
    let mut cache = page_cache_writeback::Reclaim::default();
    if stack.returned_frames < requested {
        unsafe {
            page_cache::r4os_rust_pagecache_reclaim(
                requested - stack.returned_frames,
                true,
                &mut cache,
            )
        };
    }
    let mut vm = SourceResult::INIT;
    let non_vm = stack.returned_frames.saturating_add(cache.returned_frames);
    if non_vm < requested && VM_RECLAIMER.load(Acquire) {
        vm = super::vm_pager::reclaim_evict(reason, requested - non_vm, vm_owner);
    }
    let after = free_frames();
    let result = Result {
        reason,
        requested_frames: requested,
        returned_frames: stack
            .returned_frames
            .saturating_add(cache.returned_frames)
            .saturating_add(vm.returned_frames),
        returned_bytes: stack
            .returned_bytes
            .wrapping_add(cache.returned_bytes)
            .wrapping_add(vm.returned_bytes),
        fs_returned_frames: cache.returned_frames,
        fs_returned_bytes: cache.returned_bytes,
        task_stack_returned_frames: stack.returned_frames,
        task_stack_returned_bytes: stack.returned_bytes,
        vm_returned_frames: vm.returned_frames,
        vm_returned_bytes: vm.returned_bytes,
        vm_page_outs: vm.page_outs,
        vm_failures: vm.failures,
        dirty_drains: cache.dirty_drains,
        failed_drains: cache.failed_drains.wrapping_add(vm.failures),
        before_free_frames: before,
        after_free_frames: after,
        elapsed_ticks: elapsed(start),
    };
    record(result);
    IN_RECLAIM.store(false, Release);
    unsafe { task_context::leave(unwind) };
    result
}
fn record(result: Result) {
    COUNTERS.attempts.fetch_add(1, Relaxed);

    COUNTERS
        .requested_frames
        .fetch_add(u64::from(result.requested_frames), Relaxed);
    COUNTERS
        .returned_frames
        .fetch_add(u64::from(result.returned_frames), Relaxed);
    COUNTERS
        .returned_bytes
        .fetch_add(result.returned_bytes, Relaxed);
    COUNTERS
        .fs_returned_frames
        .fetch_add(u64::from(result.fs_returned_frames), Relaxed);
    COUNTERS
        .fs_returned_bytes
        .fetch_add(result.fs_returned_bytes, Relaxed);
    COUNTERS
        .task_stack_returned_frames
        .fetch_add(u64::from(result.task_stack_returned_frames), Relaxed);
    COUNTERS
        .task_stack_returned_bytes
        .fetch_add(result.task_stack_returned_bytes, Relaxed);
    COUNTERS
        .vm_returned_frames
        .fetch_add(u64::from(result.vm_returned_frames), Relaxed);
    COUNTERS
        .vm_returned_bytes
        .fetch_add(result.vm_returned_bytes, Relaxed);
    COUNTERS
        .vm_page_outs
        .fetch_add(result.vm_page_outs, Relaxed);
    COUNTERS.vm_failures.fetch_add(result.vm_failures, Relaxed);
    COUNTERS
        .dirty_drains
        .fetch_add(result.dirty_drains, Relaxed);
    COUNTERS
        .failed_drains
        .fetch_add(result.failed_drains, Relaxed);
    COUNTERS
        .total_ticks
        .fetch_add(result.elapsed_ticks, Relaxed);
    COUNTERS.last_ticks.store(result.elapsed_ticks, Relaxed);
    COUNTERS.last_reason.store(result.reason, Relaxed);
    COUNTERS
        .last_requested_frames
        .store(result.requested_frames, Relaxed);
    COUNTERS
        .last_returned_frames
        .store(result.returned_frames, Relaxed);
    COUNTERS
        .last_before_free_frames
        .store(result.before_free_frames, Relaxed);
    COUNTERS
        .last_after_free_frames
        .store(result.after_free_frames, Relaxed);
    COUNTERS.max_ticks.fetch_max(result.elapsed_ticks, Relaxed);
    if result.returned_frames > 0 {
        COUNTERS.successes.fetch_add(1, Relaxed);
    } else {
        COUNTERS.failures.fetch_add(1, Relaxed);
    }
}
fn snapshot() -> Summary {
    Summary {
        enabled: true,
        attempts: COUNTERS.attempts.load(Relaxed),
        successes: COUNTERS.successes.load(Relaxed),
        failures: COUNTERS.failures.load(Relaxed),
        requested_frames: COUNTERS.requested_frames.load(Relaxed),
        returned_frames: COUNTERS.returned_frames.load(Relaxed),
        returned_bytes: COUNTERS.returned_bytes.load(Relaxed),
        fs_returned_frames: COUNTERS.fs_returned_frames.load(Relaxed),
        fs_returned_bytes: COUNTERS.fs_returned_bytes.load(Relaxed),
        task_stack_returned_frames: COUNTERS.task_stack_returned_frames.load(Relaxed),
        task_stack_returned_bytes: COUNTERS.task_stack_returned_bytes.load(Relaxed),
        vm_returned_frames: COUNTERS.vm_returned_frames.load(Relaxed),
        vm_returned_bytes: COUNTERS.vm_returned_bytes.load(Relaxed),
        vm_page_outs: COUNTERS.vm_page_outs.load(Relaxed),
        vm_failures: COUNTERS.vm_failures.load(Relaxed),
        dirty_drains: COUNTERS.dirty_drains.load(Relaxed),
        failed_drains: COUNTERS.failed_drains.load(Relaxed),
        total_ticks: COUNTERS.total_ticks.load(Relaxed),
        max_ticks: COUNTERS.max_ticks.load(Relaxed),
        last_ticks: COUNTERS.last_ticks.load(Relaxed),
        last_reason: COUNTERS.last_reason.load(Relaxed),
        last_requested_frames: COUNTERS.last_requested_frames.load(Relaxed),
        last_returned_frames: COUNTERS.last_returned_frames.load(Relaxed),
        last_before_free_frames: COUNTERS.last_before_free_frames.load(Relaxed),
        last_after_free_frames: COUNTERS.last_after_free_frames.load(Relaxed),
    }
}
/// Register the exact existing callback address; C dispatch changes no state.
pub(crate) fn register_vm() {
    VM_RECLAIMER.store(true, Release)
}
pub(crate) fn register_stack() {
    STACK_RECLAIMER.store(true, Release)
}
/// # Safety
/// Caller permits original bounded, waiting reclaim and retains exact task
/// admission; output owns one C result. Some(0) remains distinct from None.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_reclaim_frames(
    reason: u32,
    frames: u32,
    owner: u64,
    owner_present: bool,
    output: *mut Result,
) {
    let result = unsafe { reclaim(reason, frames, owner_present.then_some(owner)) };
    unsafe { output.write(result) };
}
/// # Safety
/// output is one writable C summary. Counters are advisory atomic snapshots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_reclaim_summary(output: *mut Summary) {
    unsafe { output.write(snapshot()) };
}

const _: () = {
    assert!(core::mem::size_of::<Result>() == 128);
    assert!(core::mem::align_of::<Result>() == 8);
    assert!(core::mem::offset_of!(Result, reason) == 0);
    assert!(core::mem::offset_of!(Result, requested_frames) == 4);
    assert!(core::mem::offset_of!(Result, returned_frames) == 8);
    assert!(core::mem::offset_of!(Result, returned_bytes) == 16);
    assert!(core::mem::offset_of!(Result, fs_returned_frames) == 24);
    assert!(core::mem::offset_of!(Result, fs_returned_bytes) == 32);
    assert!(core::mem::offset_of!(Result, task_stack_returned_frames) == 40);
    assert!(core::mem::offset_of!(Result, task_stack_returned_bytes) == 48);
    assert!(core::mem::offset_of!(Result, vm_returned_frames) == 56);
    assert!(core::mem::offset_of!(Result, vm_returned_bytes) == 64);
    assert!(core::mem::offset_of!(Result, vm_page_outs) == 72);
    assert!(core::mem::offset_of!(Result, vm_failures) == 80);
    assert!(core::mem::offset_of!(Result, dirty_drains) == 88);
    assert!(core::mem::offset_of!(Result, failed_drains) == 96);
    assert!(core::mem::offset_of!(Result, before_free_frames) == 104);
    assert!(core::mem::offset_of!(Result, after_free_frames) == 112);
    assert!(core::mem::offset_of!(Result, elapsed_ticks) == 120);
    assert!(core::mem::size_of::<SourceResult>() == 32);
    assert!(core::mem::align_of::<SourceResult>() == 8);
    assert!(core::mem::offset_of!(SourceResult, returned_frames) == 0);
    assert!(core::mem::offset_of!(SourceResult, returned_bytes) == 8);
    assert!(core::mem::offset_of!(SourceResult, page_outs) == 16);
    assert!(core::mem::offset_of!(SourceResult, failures) == 24);
    assert!(core::mem::size_of::<Summary>() == 192);
    assert!(core::mem::align_of::<Summary>() == 8);
    assert!(core::mem::offset_of!(Summary, enabled) == 0);
    assert!(core::mem::offset_of!(Summary, attempts) == 8);
    assert!(core::mem::offset_of!(Summary, successes) == 16);
    assert!(core::mem::offset_of!(Summary, failures) == 24);
    assert!(core::mem::offset_of!(Summary, requested_frames) == 32);
    assert!(core::mem::offset_of!(Summary, returned_frames) == 40);
    assert!(core::mem::offset_of!(Summary, returned_bytes) == 48);
    assert!(core::mem::offset_of!(Summary, fs_returned_frames) == 56);
    assert!(core::mem::offset_of!(Summary, fs_returned_bytes) == 64);
    assert!(core::mem::offset_of!(Summary, task_stack_returned_frames) == 72);
    assert!(core::mem::offset_of!(Summary, task_stack_returned_bytes) == 80);
    assert!(core::mem::offset_of!(Summary, vm_returned_frames) == 88);
    assert!(core::mem::offset_of!(Summary, vm_returned_bytes) == 96);
    assert!(core::mem::offset_of!(Summary, vm_page_outs) == 104);
    assert!(core::mem::offset_of!(Summary, vm_failures) == 112);
    assert!(core::mem::offset_of!(Summary, dirty_drains) == 120);
    assert!(core::mem::offset_of!(Summary, failed_drains) == 128);
    assert!(core::mem::offset_of!(Summary, total_ticks) == 136);
    assert!(core::mem::offset_of!(Summary, max_ticks) == 144);
    assert!(core::mem::offset_of!(Summary, last_ticks) == 152);
    assert!(core::mem::offset_of!(Summary, last_reason) == 160);
    assert!(core::mem::offset_of!(Summary, last_requested_frames) == 164);
    assert!(core::mem::offset_of!(Summary, last_returned_frames) == 168);
    assert!(core::mem::offset_of!(Summary, last_before_free_frames) == 176);
    assert!(core::mem::offset_of!(Summary, last_after_free_frames) == 184);
};
