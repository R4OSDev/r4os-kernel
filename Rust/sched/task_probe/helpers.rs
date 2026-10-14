//! Fixed C calls reach the real native owners, never a diagnostic model.
use crate::{
    arch::runtime,
    platform::timer,
    sched::{
        scheduler, sync_stats,
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait,
        task::{
            task_registry as registry,
            task_types::{Bytes, CreateFailure, Entry, Role, State, Task},
        },
    },
};
use core::{ffi::c_void, ptr};
unsafe extern "C" {
    fn r4os_zig_kernel_write(text: *const u8, len: usize);
    fn r4os_zig_kernel_dec(value: u64);
    pub fn r4os_rust_task_control(op: u8, a: u64, b: u64) -> u64;
    fn r4os_rust_task_mutate(
        op: u8,
        task: *mut c_void,
        a: u64,
        b: u64,
        text: *const u8,
        len: usize,
    ) -> u64;
    fn r4os_rust_task_identity(
        op: u8,
        id: u32,
        generation: u64,
        context: *mut c_void,
        value: u64,
    ) -> u64;
    fn r4os_rust_task_snapshot(op: u8, argument: u64, output: *mut c_void, len: usize) -> u64;
}
pub fn puts(text: &[u8]) {
    unsafe { r4os_zig_kernel_write(text.as_ptr(), text.len()) };
}
pub fn dec(value: u64) {
    unsafe { r4os_zig_kernel_dec(value) };
}
pub fn fail(reason: &[u8]) -> bool {
    puts(b"TASKREG05910 result: FAILED reason=");
    puts(reason);
    puts(b"\r\n");
    false
}
pub fn phase(name: &[u8]) {
    puts(b"TASKREG05910 phase=");
    puts(name);
    puts(b"\r\n");
}
pub fn current() -> *mut Task {
    scheduler::current().cast()
}
pub fn now() -> u64 {
    timer::tick_count()
}
pub fn yield_task() {
    scheduler::yield_task();
}
pub fn sleep(ticks: u64, reason: &[u8]) {
    unsafe {
        scheduler::r4os_rust_scheduler_task(
            4,
            ptr::null_mut(),
            ticks,
            0,
            reason.as_ptr(),
            reason.len(),
        );
    }
}
pub unsafe fn mutate(op: u8, t: *mut Task, a: u64, b: u64) -> u64 {
    unsafe { r4os_rust_task_mutate(op, t.cast(), a, b, ptr::null(), 0) }
}
pub unsafe fn pin(t: *mut Task) -> bool {
    unsafe { mutate(17, t, 0, 0) != 0 }
}
pub unsafe fn unpin(t: *mut Task) -> bool {
    unsafe { mutate(18, t, 0, 0) != 0 }
}
pub unsafe fn identity(op: u8, id: u32, generation: u64) -> u64 {
    unsafe { r4os_rust_task_identity(op, id, generation, ptr::null_mut(), 0) }
}
pub unsafe fn alive(id: u32, generation: u64) -> bool {
    unsafe { r4os_rust_task_control(19, u64::from(id), generation) != 0 }
}
pub unsafe fn release(id: u32, generation: u64) -> bool {
    unsafe { identity(5, id, generation) != 0 }
}
pub unsafe fn kill(id: u32, generation: u64) -> bool {
    unsafe { identity(3, id, generation) != 0 }
}
pub unsafe fn retire(id: u32, generation: u64) -> u64 {
    unsafe { identity(6, id, generation) }
}
pub fn drain_resources() {
    unsafe {
        r4os_rust_task_control(16, 0, 0);
        r4os_rust_task_control(17, 0, 0);
    }
}
pub unsafe fn queue(q: *mut WaitQueue, op: u8) -> u32 {
    unsafe { sync_wait::r4os_rust_sync_queue(q, op, ptr::null_mut(), 0, ptr::null_mut()) as u32 }
}
pub unsafe fn wait(q: *mut WaitQueue, ticks: u64, reason: &[u8]) -> u32 {
    unsafe {
        let req = WaitRequest {
            timeout: ticks,
            reason: Text {
                pointer: reason.as_ptr(),
                len: reason.len(),
            },
            predicate: None,
            context: ptr::null_mut(),
            release: None,
            release_context: ptr::null_mut(),
            flags: 0,
            consume_runtime: false,
        };
        sync_wait::r4os_rust_sync_wait(q, &req)
    }
}
#[derive(Clone, Copy)]
pub struct View {
    pub id: u32,
    pub generation: u64,
    pub state: State,
    pub role: Role,
    pub priority: u8,
    pub reason: Bytes,
    pub role_exhaustions: u64,
    pub ready_latency: u64,
    pub run_ticks: u64,
    pub switches: u64,
    pub fpu: Option<Bytes>,
}
pub unsafe fn view(t: *mut Task) -> View {
    let _owner = runtime::RuntimeGuard::acquire();
    unsafe {
        let c = registry::core(t);
        View {
            id: (*c).id,
            generation: (*c).generation,
            state: (*c).state,
            role: (*c).role,
            priority: (*c).priority as u8,
            reason: (*c).wait_reason,
            role_exhaustions: (*c).role_budget_exhaustions,
            ready_latency: (*c).last_ready_latency_ticks,
            run_ticks: (*c).run_ticks,
            switches: (*c).switches_in,
            fpu: (*c).fpu_state,
        }
    }
}
pub fn reason_is(value: Bytes, wanted: &[u8]) -> bool {
    if value.len != wanted.len() {
        return false;
    }
    value.len == 0 || unsafe { core::slice::from_raw_parts(value.pointer, value.len) == wanted }
}
pub unsafe fn create(name: &'static [u8], entry: Entry, state: State, role: Role) -> *mut Task {
    unsafe {
        let mut failure = CreateFailure::None;
        super::owner_snapshots::create(name, entry, state, role, &mut failure)
    }
}
pub fn wait_summary() -> crate::sched::sync_types::WaitSummary {
    let mut out = crate::sched::sync_types::WaitSummary::default();
    unsafe { sync_stats::r4os_rust_sync_wait_summary(&mut out) };
    out
}
pub unsafe fn queue_valid() -> bool {
    unsafe {
        let mut value = core::mem::MaybeUninit::<QueueView>::uninit();
        r4os_rust_task_snapshot(
            3,
            0,
            value.as_mut_ptr().cast(),
            core::mem::size_of::<QueueView>(),
        );
        value.assume_init().valid
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct QueueView {
    pub registry: usize,
    pub ready: usize,
    pub timed: usize,
    pub reap: usize,
    pub valid: bool,
}

pub unsafe fn queue_snapshot() -> QueueView {
    unsafe {
        let mut out = core::mem::MaybeUninit::<QueueView>::uninit();
        r4os_rust_task_snapshot(
            3,
            0,
            out.as_mut_ptr().cast(),
            core::mem::size_of::<QueueView>(),
        );
        out.assume_init()
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RoleStats {
    pub activations: [u64; 4],
    pub budget_exhaustions: [u64; 4],
    pub donation_requests: u64,
    pub donations_applied: u64,
    pub donations_released: u64,
    pub donation_budget_exhaustions: u64,
}
unsafe extern "C" {
    fn r4os_rust_task_roles(out: *mut RoleStats);
}
pub fn role_stats() -> RoleStats {
    let mut out = core::mem::MaybeUninit::uninit();
    unsafe {
        r4os_rust_task_roles(out.as_mut_ptr());
        out.assume_init()
    }
}
pub fn structure() -> crate::sched::scheduler_types::StructureStats {
    let mut out = core::mem::MaybeUninit::uninit();
    unsafe {
        scheduler::r4os_rust_scheduler_structure(out.as_mut_ptr());
        out.assume_init()
    }
}
pub fn lock_stats() -> crate::sched::sync_types::LockSummary {
    let mut out = crate::sched::sync_types::LockSummary::default();
    unsafe { sync_stats::r4os_rust_sync_lock_summary(&mut out) };
    out
}
