//! Dedicated R4D threads: task/FPU construction, exact callback lifetime,
//! wait leases, synchronous self-abort and confirmed retirement/free.
use crate::{
    arch::{callback_abort as abort, percpu, runtime::RuntimeGuard},
    contract as a,
    driver::{
        semaphores, thread_owner as m,
        treap::{Node, Tree},
    },
    memory::heap,
    platform::timer,
    sched::{
        scheduler,
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait,
        task::{
            task_registry as registry,
            task_resources::{self, CreateArgs},
            task_types::{Bytes, CreateFailure, ExecutionOwnerKind, Role, State as TaskState},
        },
        task_context,
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{MaybeUninit, size_of},
    ptr,
};
const CAP: usize = crate::driver::registry::MAX_DRIVERS;
#[repr(C)]
struct State {
    owners: [m::Owner; CAP],
    next_handle: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: canonical metadata and enrollment use the existing runtime owner.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    owners: [const {
        m::Owner {
            stats: m::Stats {
                epoch: 0,
                records: 0,
                active: 0,
                completed: 0,
                private: 0,
                starts: 0,
                start_failures: 0,
                releases: 0,
                release_failures: 0,
                pending_creates: 0,
                pending_releases: 0,
                leases: 0,
                closing: false,
            },
            tree: Tree {
                root: ptr::null_mut(),
                xorshift: 0,
            },
        }
    }; CAP],
    next_handle: 1,
}));
#[repr(C)]
struct Payload {
    handler: abort::Handler,
    context: usize,
    task_ptr: *mut c_void,
    task_generation: u64,
    completion: WaitQueue,
    sleep_queue: WaitQueue,
    waiting_on: *mut WaitQueue,
    abort_frame: *mut abort::Frame,
    flags: u32,
    task_id: u32,
    cpu_index: u32,
    published: bool,
    abandoned: bool,
}
#[repr(C)]
struct Record {
    header: m::Record,
    payload: Payload,
}
fn owners() -> *mut m::Owner {
    unsafe { ptr::addr_of_mut!((*STATE.0.get()).owners).cast() }
}
fn code(c: u8) -> i32 {
    match c {
        0 => a::driver_thread_ok,
        1 => a::driver_thread_error_invalid,
        2 => a::driver_thread_error_owner,
        3 => a::driver_thread_error_stale,
        4 => a::driver_thread_error_closed,
        5 => a::driver_thread_error_busy,
        6 => a::driver_thread_error_exhausted,
        _ => unreachable!(),
    }
}
struct Operation(task_context::Unwind);
impl Operation {
    fn enter() -> Option<Self> {
        let _g = RuntimeGuard::acquire();
        let t = unsafe { task_context::enter() };
        t.admitted().then_some(Self(t))
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _g = RuntimeGuard::acquire();
        assert!(unsafe { task_context::leave(self.0) })
    }
}
unsafe fn valid<T>(out: *const T) -> bool {
    let head = out.cast::<u32>();
    unsafe { *head == 1 && *head.add(1) >= size_of::<T>() as u32 }
}
unsafe extern "C" {
    fn r4os_rust_task_identity(
        op: u8,
        id: u32,
        generation: u64,
        context: *mut c_void,
        argument: u64,
    ) -> u64;
}
fn snapshot(owner: u32) -> Result<m::Stats, i32> {
    let mut out = MaybeUninit::uninit();
    let c = unsafe { m::r4os_rust_thread_owner_stats(owners(), CAP, owner, out.as_mut_ptr()) };
    if c == 0 {
        Ok(unsafe { out.assume_init() })
    } else {
        Err(code(c))
    }
}
fn record(owner: u32, handle: u64, op: u8) -> Result<*mut Record, i32> {
    let mut out = ptr::null_mut();
    let c = unsafe { m::r4os_rust_thread_owner_lookup(owners(), CAP, owner, handle, op, &mut out) };
    if c == 0 { Ok(out.cast()) } else { Err(code(c)) }
}
fn public_record(owner: u32, handle: u64) -> Result<*mut Record, i32> {
    let r = record(owner, handle, 0)?;
    if unsafe { (*r).header.private } {
        Err(a::driver_thread_error_stale)
    } else {
        Ok(r)
    }
}
fn current_record() -> *mut Record {
    let task = scheduler::current();
    if task.is_null() {
        return ptr::null_mut();
    }
    let c = unsafe { registry::core(task.cast()) };
    if unsafe {
        (*c).execution_owner_kind != ExecutionOwnerKind::DriverThread
            || (*c).execution_owner_context.is_null()
    } {
        return ptr::null_mut();
    }
    let r = unsafe { (*c).execution_owner_context.cast::<Record>() };
    if unsafe {
        (*r).header.phase != 2
            || (*r).payload.task_id != (*c).id
            || (*r).payload.task_generation != (*c).generation
    } {
        return ptr::null_mut();
    }
    r
}
fn stopping(r: *mut Record) -> bool {
    let mut value = false;
    assert_eq!(
        unsafe {
            m::r4os_rust_thread_owner_record(owners(), CAP, r.cast(), 4, false, 0, &mut value)
        },
        0
    );
    value
}
fn record_effect(r: *mut Record, op: u8, created: bool, result: i32) -> bool {
    let mut value = false;
    assert_eq!(
        unsafe {
            m::r4os_rust_thread_owner_record(
                owners(),
                CAP,
                r.cast(),
                op,
                created,
                result,
                &mut value,
            )
        },
        0
    );
    value
}
fn cancel_wait(r: *mut Record) {
    unsafe {
        if (*r).header.phase != 1 && (*r).header.phase != 2 {
            return;
        }
        let q = (*r).payload.waiting_on;
        if q.is_null() {
            return;
        }
        sync_wait::r4os_rust_sync_queue(
            q,
            7,
            (*r).payload.task_ptr,
            (*r).payload.task_generation,
            ptr::null_mut(),
        );
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_bind(owner: u32, epoch: u64) -> bool {
    let _g = RuntimeGuard::acquire();
    unsafe { m::r4os_rust_thread_owner_mutate(owners(), CAP, owner, epoch, 0) == 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_discard(owner: u32, epoch: u64) -> bool {
    let _g = RuntimeGuard::acquire();
    let Ok(s) = snapshot(owner) else { return false };
    if s.epoch != epoch || s.records != 0 || s.pending_creates != 0 {
        return false;
    }
    unsafe {
        m::r4os_rust_thread_owner_mutate(owners(), CAP, owner, 0, 1);
        m::r4os_rust_thread_owner_mutate(owners(), CAP, owner, 0, 2) == 0
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_query(owner: u32) -> i32 {
    let _g = RuntimeGuard::acquire();
    match snapshot(owner) {
        Err(c) => c,
        Ok(s) => {
            if s.closing {
                a::driver_thread_error_closed
            } else {
                a::driver_thread_ok
            }
        }
    }
}
// 0 current owner, 1 current normal-Work owner admission, 2 current handle.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_current(op: u8, owner: u32) -> u64 {
    let _g = RuntimeGuard::acquire();
    let r = current_record();
    if r.is_null() {
        return 0;
    }
    unsafe {
        match op {
            0 => u64::from((*r).header.owner),
            1 => {
                let Ok(s) = snapshot((*r).header.owner) else {
                    return 0;
                };
                if s.epoch == (*r).header.epoch && !s.closing && !(*r).header.stop_requested {
                    u64::from((*r).header.owner)
                } else {
                    0
                }
            }
            2 => {
                if (*r).header.owner == owner {
                    (*r).header.handle
                } else {
                    0
                }
            }
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Caller storage and canonical R4D identity stay valid through the exact
/// admitted operation. Foreign callbacks and waits retain real task/unwind lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_thread_request(
    owner: u32,
    out: *mut a::DriverThreadRequest,
) -> i32 {
    if !unsafe { valid(out) } {
        return a::driver_thread_error_invalid;
    }
    unsafe { ptr::write(out, a::DriverThreadRequest::default()) };
    let _g = RuntimeGuard::acquire();
    let r = current_record();
    if r.is_null() {
        return a::driver_thread_error_context;
    }
    if unsafe { (*r).header.owner != owner } {
        return a::driver_thread_error_owner;
    }
    unsafe {
        ptr::write(
            out,
            a::DriverThreadRequest {
                handler: (*r).payload.handler as usize as u64,
                context: (*r).payload.context as u64,
                flags: (*r).payload.flags
                    & (a::driver_thread_flag_parallel | a::driver_thread_flag_abortable),
                ..a::DriverThreadRequest::default()
            },
        )
    };
    a::driver_thread_ok
}
/// # Safety
/// Caller storage and canonical R4D identity stay valid through the exact
/// admitted operation. Foreign callbacks and waits retain real task/unwind lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_thread_start(
    owner: u32,
    input: *const a::DriverThreadRequest,
    out: *mut u64,
) -> i32 {
    unsafe { *out = 0 };
    let request = unsafe { ptr::read(input) };
    if !unsafe { valid(&request) }
        || request.handler == 0
        || request.reserved != 0
        || request.flags & !(a::driver_thread_flag_parallel | a::driver_thread_flag_abortable) != 0
    {
        return a::driver_thread_error_invalid;
    }
    if !semaphores::r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_thread_error_context;
    }
    let Some(_operation) = Operation::enter() else {
        return a::driver_thread_error_busy;
    };
    let mut create = MaybeUninit::uninit();
    {
        let _g = RuntimeGuard::acquire();
        let c = unsafe {
            m::r4os_rust_thread_owner_begin(
                owners(),
                CAP,
                ptr::addr_of_mut!((*STATE.0.get()).next_handle),
                owner,
                create.as_mut_ptr(),
            )
        };
        if c != 0 {
            return code(c);
        }
    }
    let mut create = unsafe { create.assume_init() };
    let memory = heap::r4os_rust_heap_alloc(size_of::<Record>(), core::mem::align_of::<Record>());
    if memory.is_null() {
        let _g = RuntimeGuard::acquire();
        assert_eq!(
            unsafe { m::r4os_rust_thread_owner_abort(owners(), CAP, &mut create) },
            0
        );
        return a::driver_thread_error_memory;
    }
    let r = memory.cast::<Record>();
    unsafe {
        ptr::write(
            r,
            Record {
                header: m::Record {
                    node: Node {
                        key: 0,
                        priority: 0,
                        parent: ptr::null_mut(),
                        children: [ptr::null_mut(); 2],
                    },
                    epoch: 0,
                    handle: 0,
                    owner: 0,
                    phase: 0,
                    references: 0,
                    result: 0,
                    private: true,
                    stop_requested: false,
                },
                payload: Payload {
                    handler: core::mem::transmute::<usize, abort::Handler>(
                        request.handler as usize,
                    ),
                    context: request.context as usize,
                    flags: request.flags,
                    task_ptr: ptr::null_mut(),
                    task_id: 0,
                    task_generation: 0,
                    published: false,
                    abandoned: false,
                    cpu_index: u32::MAX,
                    completion: WaitQueue::EMPTY,
                    sleep_queue: WaitQueue::EMPTY,
                    waiting_on: ptr::null_mut(),
                    abort_frame: ptr::null_mut(),
                },
            },
        )
    };
    {
        let _g = RuntimeGuard::acquire();
        assert_eq!(
            unsafe { m::r4os_rust_thread_owner_adopt(owners(), CAP, &mut create, r.cast()) },
            0
        )
    }
    let mut failure = CreateFailure::None;
    let parallel = request.flags & a::driver_thread_flag_parallel != 0;
    let created = unsafe {
        task_resources::create(
            CreateArgs {
                name: Bytes {
                    pointer: b"r4d-thread".as_ptr(),
                    len: b"r4d-thread".len(),
                },
                state: TaskState::Blocked,
                needs_fpu: true,
                entry: Some(thread_main),
                role: Role::Interactive,
                smp_eligible: parallel,
                smp_r4x: parallel,
                owner_kind: ExecutionOwnerKind::DriverThread,
                owner_context: r.cast(),
            },
            &mut failure,
        )
    };
    let result = {
        let _g = RuntimeGuard::acquire();
        if !created.is_null() {
            let c = unsafe { registry::core(created) };
            unsafe {
                (*r).payload.task_ptr = created.cast();
                (*r).payload.task_id = (*c).id;
                (*r).payload.task_generation = (*c).generation;
            }
        }
        let fail = if created.is_null() {
            a::driver_thread_error_memory
        } else {
            a::driver_thread_error_closed
        };
        if !record_effect(r, 0, !created.is_null(), fail) {
            fail
        } else if unsafe {
            scheduler::r4os_rust_scheduler_task(3, created.cast(), 0, 0, ptr::null(), 0)
        } == 0
        {
            record_effect(r, 1, false, a::driver_thread_error_busy);
            a::driver_thread_error_busy
        } else {
            unsafe {
                (*r).payload.published = true;
                *out = (*r).header.handle
            };
            a::driver_thread_ok
        }
    };
    if result != a::driver_thread_ok {
        remove(owner, create.handle, true);
    }
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_stop(owner: u32, handle: u64) -> i32 {
    let _g = RuntimeGuard::acquire();
    let r = match public_record(owner, handle) {
        Ok(r) => r,
        Err(c) => return c,
    };
    assert!(record(owner, handle, 1).is_ok());
    cancel_wait(r);
    a::driver_thread_ok
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_abort(owner: u32, result: i32) -> i32 {
    if result >= 0 {
        return a::driver_thread_error_invalid;
    }
    if !semaphores::r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_thread_error_context;
    }
    let frame = {
        let _g = RuntimeGuard::acquire();
        let r = current_record();
        if r.is_null() {
            return a::driver_thread_error_context;
        }
        if unsafe { (*r).header.owner != owner } {
            return a::driver_thread_error_owner;
        }
        let c = unsafe { registry::core(scheduler::current().cast()) };
        if unsafe { (*c).unwind_guard_count != 1 || !(*r).payload.waiting_on.is_null() } {
            return a::driver_thread_error_busy;
        }
        let frame = unsafe { (*r).payload.abort_frame };
        if frame.is_null() {
            return a::driver_thread_error_context;
        }
        if unsafe { (*r).payload.flags & a::driver_thread_flag_abortable == 0 || (*frame).rsp == 0 }
        {
            return a::driver_thread_error_context;
        }
        unsafe { (*r).payload.flags |= a::driver_thread_flag_aborted };
        frame
    };
    unsafe { abort::r4os_rust_callback_resume(frame, result) }
}
/// # Safety
/// Caller storage and canonical R4D identity stay valid through the exact
/// admitted operation. Foreign callbacks and waits retain real task/unwind lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_thread_status(
    owner: u32,
    handle: u64,
    out: *mut a::DriverThreadStatus,
) -> i32 {
    if !unsafe { valid(out) } {
        return a::driver_thread_error_invalid;
    }
    unsafe { ptr::write(out, a::DriverThreadStatus::default()) };
    let _g = RuntimeGuard::acquire();
    let r = match public_record(owner, handle) {
        Ok(r) => r,
        Err(c) => return c,
    };
    let p = unsafe { ptr::addr_of_mut!((*r).payload) };
    let sleeping = unsafe {
        (*r).header.phase == 2
            && (*p).waiting_on == ptr::addr_of_mut!((*p).sleep_queue)
            && !(*p).task_ptr.is_null()
            && (*registry::core((*p).task_ptr.cast())).state == TaskState::Blocked
    };
    unsafe {
        ptr::write(
            out,
            a::DriverThreadStatus {
                handle: (*r).header.handle,
                owner_epoch: (*r).header.epoch,
                task_generation: (*p).task_generation,
                task_id: (*p).task_id,
                cpu_index: (*p).cpu_index,
                state: (*r).header.phase,
                stop_requested: u32::from(stopping(r)),
                result: (*r).header.result,
                flags: (*p).flags
                    | if sleeping {
                        a::driver_thread_flag_sleeping
                    } else {
                        0
                    },
                waiters: (*r).header.references,
                ..a::DriverThreadStatus::default()
            },
        )
    };
    a::driver_thread_ok
}
/// # Safety
/// Caller storage and canonical R4D identity stay valid through the exact
/// admitted operation. Foreign callbacks and waits retain real task/unwind lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_thread_stats(
    owner: u32,
    out: *mut a::DriverThreadStats,
) -> i32 {
    if !unsafe { valid(out) } {
        return a::driver_thread_error_invalid;
    }
    unsafe { ptr::write(out, a::DriverThreadStats::default()) };
    let _g = RuntimeGuard::acquire();
    let s = match snapshot(owner) {
        Ok(s) => s,
        Err(c) => return c,
    };
    unsafe {
        ptr::write(
            out,
            a::DriverThreadStats {
                owner_epoch: s.epoch,
                records: s.records,
                active: s.active,
                completed: s.completed,
                private_records: s.private,
                starts: s.starts,
                start_failures: s.start_failures,
                releases: s.releases,
                release_retries: s.release_failures,
                pending_creates: s.pending_creates,
                pending_releases: s.pending_releases,
                waiters: s.leases,
                closing: u32::from(s.closing),
                ..a::DriverThreadStats::default()
            },
        )
    };
    a::driver_thread_ok
}
#[repr(C)]
struct JoinWait {
    target: *mut Record,
    caller: *mut Record,
}
unsafe extern "C" fn join_needed(context: *mut c_void) -> bool {
    let value = context.cast::<JoinWait>();
    unsafe {
        if (*(*value).target).header.phase == 3 {
            return false;
        }
        (*value).caller.is_null() || !stopping((*value).caller)
    }
}
/// # Safety
/// Caller storage and canonical R4D identity stay valid through the exact
/// admitted operation. Foreign callbacks and waits retain real task/unwind lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_thread_join(
    owner: u32,
    handle: u64,
    ticks: u64,
    out: *mut i32,
) -> i32 {
    unsafe { *out = 0 };
    if ticks == u64::MAX {
        return a::driver_thread_error_invalid;
    }
    if !semaphores::r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_thread_error_context;
    }
    let Some(_operation) = Operation::enter() else {
        return a::driver_thread_error_busy;
    };
    let mut wait = {
        let _g = RuntimeGuard::acquire();
        let target = match public_record(owner, handle) {
            Ok(r) => r,
            Err(c) => return c,
        };
        let caller = current_record();
        if caller == target {
            return a::driver_thread_error_self_join;
        }
        if let Err(c) = record(owner, handle, 2) {
            return c;
        }
        if !caller.is_null() {
            unsafe {
                assert!((*caller).payload.waiting_on.is_null());
                (*caller).payload.waiting_on = ptr::addr_of_mut!((*target).payload.completion)
            }
        }
        JoinWait { target, caller }
    };
    let request = WaitRequest::predicate(
        ticks,
        Text::literal(b"r4d-join"),
        join_needed,
        ptr::addr_of_mut!(wait).cast(),
    );
    let result = unsafe {
        sync_wait::r4os_rust_sync_wait(
            ptr::addr_of_mut!((*wait.target).payload.completion),
            &request,
        )
    };
    let _g = RuntimeGuard::acquire();
    if !wait.caller.is_null() {
        unsafe { (*wait.caller).payload.waiting_on = ptr::null_mut() }
    }
    let completed = unsafe { (*wait.target).header.phase == 3 };
    let value = unsafe { (*wait.target).header.result };
    let stopped = !completed && !wait.caller.is_null() && stopping(wait.caller);
    record_effect(wait.target, 5, false, 0);
    if completed {
        unsafe { *out = value };
        return a::driver_thread_ok;
    }
    if stopped {
        return a::driver_thread_error_cancelled;
    }
    match result {
        2 => a::driver_thread_error_timeout,
        3 => a::driver_thread_error_cancelled,
        _ => a::driver_thread_error_context,
    }
}
unsafe extern "C" fn sleep_needed(context: *mut c_void) -> bool {
    !stopping(context.cast())
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_sleep(owner: u32, ticks: u64) -> i32 {
    if !semaphores::r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_thread_error_context;
    }
    let r = {
        let _g = RuntimeGuard::acquire();
        let r = current_record();
        if r.is_null() {
            return a::driver_thread_error_context;
        }
        if unsafe { (*r).header.owner != owner } {
            return a::driver_thread_error_owner;
        }
        if stopping(r) {
            return a::driver_thread_error_cancelled;
        }
        unsafe {
            assert!((*r).payload.waiting_on.is_null());
            (*r).payload.waiting_on = ptr::addr_of_mut!((*r).payload.sleep_queue)
        };
        r
    };
    let result = if ticks == 0 {
        scheduler::yield_task();
        2
    } else {
        let request =
            WaitRequest::predicate(ticks, Text::literal(b"r4d-sleep"), sleep_needed, r.cast());
        unsafe {
            sync_wait::r4os_rust_sync_wait(ptr::addr_of_mut!((*r).payload.sleep_queue), &request)
        }
    };
    let _g = RuntimeGuard::acquire();
    unsafe {
        (*r).payload.waiting_on = ptr::null_mut();
        (*r).payload.cpu_index = percpu::current_index()
    };
    if stopping(r) {
        return a::driver_thread_error_cancelled;
    }
    match result {
        2 => a::driver_thread_ok,
        3 => a::driver_thread_error_cancelled,
        _ => a::driver_thread_error_context,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_release(owner: u32, handle: u64) -> i32 {
    if !semaphores::r4os_rust_driver_semaphore_can_sleep() {
        return a::driver_thread_error_context;
    }
    let Some(_operation) = Operation::enter() else {
        return a::driver_thread_error_busy;
    };
    remove(owner, handle, false)
}
fn release_backing(r: *mut Record) -> i32 {
    unsafe {
        let p = ptr::addr_of_mut!((*r).payload);
        if (*p).task_id != 0 {
            if !(*p).published && !(*p).abandoned {
                if r4os_rust_task_identity(8, (*p).task_id, (*p).task_generation, r.cast(), 0) == 0
                {
                    return a::driver_thread_error_busy;
                }
                (*p).abandoned = true
            }
            if r4os_rust_task_identity(6, (*p).task_id, (*p).task_generation, ptr::null_mut(), 0)
                == 1
            {
                return a::driver_thread_error_busy;
            }
        }
        if heap::r4os_rust_heap_free(r.cast(), size_of::<Record>()) == 0 {
            a::driver_thread_ok
        } else {
            a::driver_thread_error_release
        }
    }
}
fn remove(owner: u32, handle: u64, allow_private: bool) -> i32 {
    let mut ticket = MaybeUninit::uninit();
    {
        let _g = RuntimeGuard::acquire();
        if !allow_private && let Err(c) = public_record(owner, handle) {
            return c;
        }
        let caller = current_record();
        if !caller.is_null() && unsafe { (*caller).header.handle == handle } {
            return a::driver_thread_error_self_join;
        }
        let c = unsafe {
            m::r4os_rust_thread_owner_remove(owners(), CAP, owner, handle, ticket.as_mut_ptr())
        };
        if c != 0 {
            return code(c);
        }
    }
    let mut ticket = unsafe { ticket.assume_init() };
    let result = release_backing(ticket.record.cast());
    let _g = RuntimeGuard::acquire();
    assert_eq!(
        unsafe {
            m::r4os_rust_thread_owner_finish_remove(
                owners(),
                CAP,
                &mut ticket,
                result == a::driver_thread_ok,
            )
        },
        0
    );
    result
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_stop_existing(owner: u32) {
    let last = {
        let _g = RuntimeGuard::acquire();
        let next = unsafe { (*STATE.0.get()).next_handle };
        if next == 0 { u64::MAX } else { next - 1 }
    };
    let mut cursor = 0;
    loop {
        let _g = RuntimeGuard::acquire();
        let mut r = ptr::null_mut();
        if unsafe { m::r4os_rust_thread_owner_after(owners(), CAP, owner, cursor, &mut r) } != 0
            || r.is_null()
        {
            return;
        }
        let r = r.cast::<Record>();
        let handle = unsafe { (*r).header.handle };
        if handle > last {
            return;
        }
        cursor = handle;
        if record(owner, handle, 1).is_err() {
            return;
        }
        cancel_wait(r)
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_close(owner: u32) {
    {
        let _g = RuntimeGuard::acquire();
        unsafe { m::r4os_rust_thread_owner_mutate(owners(), CAP, owner, 0, 1) };
    }
    r4os_rust_driver_thread_stop_existing(owner)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_thread_quiesced(owner: u32) -> bool {
    let _g = RuntimeGuard::acquire();
    match snapshot(owner) {
        Err(c) => c == a::driver_thread_error_stale,
        Ok(s) => s.closing && s.active == 0 && s.pending_creates == 0,
    }
}
#[repr(C)]
pub struct Cleanup {
    pub released: u64,
    pub quiesced: bool,
}
fn cleanup(owner: u32) -> Cleanup {
    let mut result = Cleanup {
        released: 0,
        quiesced: true,
    };
    let count = {
        let _g = RuntimeGuard::acquire();
        let s = match snapshot(owner) {
            Ok(s) => s,
            Err(c) => {
                result.quiesced = c == a::driver_thread_error_stale;
                return result;
            }
        };
        if !s.closing
            || s.active != 0
            || s.pending_creates != 0
            || s.pending_releases != 0
            || s.leases != 0
        {
            result.quiesced = false;
            return result;
        }
        if s.records == 0 {
            assert_eq!(
                unsafe { m::r4os_rust_thread_owner_mutate(owners(), CAP, owner, 0, 2) },
                0
            );
            return result;
        }
        s.records
    };
    if !semaphores::r4os_rust_driver_semaphore_can_sleep() {
        result.quiesced = false;
        return result;
    }
    let Some(_operation) = Operation::enter() else {
        result.quiesced = false;
        return result;
    };
    let deadline = timer::r4os_rust_timer_control(7, 1000, 0);
    while result.released < count {
        let handle = {
            let _g = RuntimeGuard::acquire();
            let mut r = ptr::null_mut();
            assert_eq!(
                unsafe { m::r4os_rust_thread_owner_after(owners(), CAP, owner, 0, &mut r) },
                0
            );
            if r.is_null() {
                break;
            }
            unsafe { (*r).handle }
        };
        let removed = remove(owner, handle, true);
        if removed == a::driver_thread_error_busy
            && timer::r4os_rust_timer_control(8, timer::tick_count(), deadline) != 0
        {
            unsafe {
                scheduler::r4os_rust_scheduler_task(
                    4,
                    ptr::null_mut(),
                    1,
                    0,
                    b"r4d-retire".as_ptr(),
                    b"r4d-retire".len(),
                )
            };
            continue;
        }
        if removed != a::driver_thread_ok {
            result.quiesced = false;
            return result;
        }
        result.released += 1;
    }
    let _g = RuntimeGuard::acquire();
    if unsafe { m::r4os_rust_thread_owner_mutate(owners(), CAP, owner, 0, 2) } != 0 {
        result.quiesced = false
    }
    result
}
/// # Safety
/// Caller storage and canonical R4D identity stay valid through the exact
/// admitted operation. Foreign callbacks and waits retain real task/unwind lifetimes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_thread_cleanup(owner: u32, out: *mut Cleanup) {
    unsafe { ptr::write(out, cleanup(owner)) };
}
unsafe extern "C" fn thread_main() {
    let current = scheduler::current();
    assert!(!current.is_null());
    let c = unsafe { registry::core(current.cast()) };
    let r = unsafe { (*c).execution_owner_context.cast::<Record>() };
    assert!(!r.is_null());
    let admitted = {
        let _g = RuntimeGuard::acquire();
        unsafe {
            assert!(
                (*r).payload.task_id == (*c).id && (*r).payload.task_generation == (*c).generation
            );
            assert_eq!((*c).unwind_guard_count, 1);
            (*r).payload.cpu_index = percpu::current_index()
        };
        record_effect(r, 2, false, 0)
    };
    let mut frame = abort::Frame::default();
    let result = if !admitted {
        a::driver_thread_error_cancelled
    } else if unsafe { (*r).payload.flags & a::driver_thread_flag_abortable == 0 } {
        unsafe { ((*r).payload.handler)((*r).payload.context) }
    } else {
        unsafe { (*r).payload.abort_frame = ptr::addr_of_mut!(frame) };
        let value = unsafe {
            abort::r4os_rust_callback_invoke((*r).payload.handler, (*r).payload.context, &mut frame)
        };
        unsafe { (*r).payload.abort_frame = ptr::null_mut() };
        value
    };
    {
        let _g = RuntimeGuard::acquire();
        unsafe {
            assert!((*c).held_lock_count == 0 && (*c).unwind_guard_count == 1);
            assert!((*r).payload.waiting_on.is_null())
        };
        record_effect(r, 3, false, result);
    }
    while unsafe {
        sync_wait::r4os_rust_sync_queue(
            ptr::addr_of_mut!((*r).payload.completion),
            0,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
        )
    } != 0
    {}
    let _g = RuntimeGuard::acquire();
    unsafe { (*c).unwind_guard_count -= 1 };
    // No access to record after dropping its final exact execution guard.
}
const _: () = {
    assert!(core::mem::offset_of!(Record, payload) == 80);
};
