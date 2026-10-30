//! Program-scoped asynchronous I/O. Request slots, resident workers, caller
//! identities and cancellation/retirement anchors have one native owner. No
//! request mutex spans a filesystem/service wait or dynamic Task retirement.
mod dispatch;
mod service;
use super::{context, registry_state, registry_types::ProgramInstance, thread_retire, threads};
use crate::{
    boot::foundation,
    contract::{
        self as c, ProgramIoInfo as Info, ProgramProcessHandle as Handle,
        ServiceMessageHeader as Header,
    },
    fs::vfs_types::Bytes,
    memory::owner::Guard,
    platform::timer,
    sched::{
        scheduler, sync_locks, sync_objects,
        sync_types::{Completion, Event, Mutex, Text, WaitQueue},
        sync_wait,
        task::{
            task_api, task_registry,
            task_types::{ExecutionOwnerKind, Task},
        },
        task_context,
    },
};
use core::{
    cell::UnsafeCell,
    mem, ptr, slice,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
use dispatch::Lane;
const CAPACITY: usize = dispatch::REQUESTS;
const WORKERS: usize = 2 * dispatch::WIDTH;
const PATH: usize = c::file_path_max_bytes as usize + 1;
const RETRIES: u32 = 64;
const INVALID: i32 = -1;
const NO_INSTANCE: i32 = -2;
const NO_SLOTS: i32 = -3;
const SPAWN_FAILED: i32 = -4;
const NOT_FOUND: i32 = -5;
const TIMEOUT: i32 = -6;
const BUSY: i32 = -7;
const UNSUPPORTED: i32 = -8;
const CANCELLED: i32 = -9;
const TOO_LARGE: i32 = -10;
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    None = 0,
    Read = 1,
    ReadAt = 2,
    Write = 3,
    Append = 4,
    StreamBegin = 5,
    StreamWrite = 6,
    StreamFinish = 7,
    StreamAbort = 8,
    Service = 9,
    WriteAt = 10,
    Info = 11,
    Lock = 12,
}
impl Kind {
    fn decode(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Read,
            2 => Self::ReadAt,
            3 => Self::Write,
            4 => Self::Append,
            5 => Self::StreamBegin,
            6 => Self::StreamWrite,
            7 => Self::StreamFinish,
            8 => Self::StreamAbort,
            9 => Self::Service,
            10 => Self::WriteAt,
            11 => Self::Info,
            12 => Self::Lock,
            _ => unreachable!("Private async I/O kind"),
        }
    }
    fn stream(self) -> bool {
        matches!(
            self,
            Self::StreamBegin | Self::StreamWrite | Self::StreamFinish | Self::StreamAbort
        )
    }
}
struct Request {
    used: bool,
    enqueued: bool,
    id: u32,
    owner_id: u32,
    owner_generation: u64,
    owner_instance: *mut ProgramInstance,
    caller_id: u32,
    caller_generation: u64,
    task_id: u32,
    task_generation: u64,
    last_worker_id: u32,
    kind: Kind,
    state: u32,
    cancel_requested: bool,
    close_pending: bool,
    waiters: u32,
    flags: u32,
    status: i32,
    result: i32,
    requested_bytes: u64,
    processed_bytes: u64,
    submitted_tick: u64,
    completed_tick: u64,
    completion: Completion,
    path: [u8; PATH],
    path_len: usize,
    offset: u64,
    data: usize,
    data_len: u64,
    out: usize,
    out_len: u64,
    service_handle: u32,
    service_op: u16,
    service_request: usize,
    service_request_len: u32,
    service_response_header: usize,
    service_response: usize,
    service_response_capacity: u32,
    service_timeout: u64,
    service_deadline: u64,
    service_epoch: u64,
    service_interested: bool,
    service_request_id: u32,
}
impl Request {
    const EMPTY: Self = Self {
        used: false,
        enqueued: false,
        id: 0,
        owner_id: 0,
        owner_generation: 0,
        owner_instance: ptr::null_mut(),
        caller_id: 0,
        caller_generation: 0,
        task_id: 0,
        task_generation: 0,
        last_worker_id: 0,
        kind: Kind::None,
        state: 0,
        cancel_requested: false,
        close_pending: false,
        waiters: 0,
        flags: 0,
        status: 0,
        result: 0,
        requested_bytes: 0,
        processed_bytes: 0,
        submitted_tick: 0,
        completed_tick: 0,
        completion: Completion {
            completed: 0,
            queue: WaitQueue::EMPTY,
        },
        path: [0; PATH],
        path_len: 0,
        offset: 0,
        data: 0,
        data_len: 0,
        out: 0,
        out_len: 0,
        service_handle: 0,
        service_op: 0,
        service_request: 0,
        service_request_len: 0,
        service_response_header: 0,
        service_response: 0,
        service_response_capacity: 0,
        service_timeout: 0,
        service_deadline: 0,
        service_epoch: 0,
        service_interested: false,
        service_request_id: 0,
    };
}
struct Worker {
    task_id: u32,
    task_generation: u64,
    request: AtomicPtr<Request>,
    retiring: bool,
    service: bool,
    event: Event,
}
impl Worker {
    const fn empty() -> Self {
        Self {
            task_id: 0,
            task_generation: 0,
            request: AtomicPtr::new(ptr::null_mut()),
            retiring: false,
            service: false,
            event: Event::AUTO,
        }
    }
}
struct Range {
    used: bool,
    owner_id: u32,
    owner_generation: u64,
    offset: u64,
    length: u64,
    path: [u8; PATH],
    path_len: usize,
}
impl Range {
    const EMPTY: Self = Self {
        used: false,
        owner_id: 0,
        owner_generation: 0,
        offset: 0,
        length: 0,
        path: [0; PATH],
        path_len: 0,
    };
}
struct State {
    requests: [Request; CAPACITY],
    workers: [Worker; WORKERS],
    cursor: [usize; 2],
    lanes: [Lane; 2],
    creations: u64,
    processed: u64,
    ranges: [Range; 64],
    next_id: u32,
    armed: bool,
    consumed: bool,
    remaining: u32,
    retry_id: u32,
}
struct Cell<T>(UnsafeCell<T>);
// SAFETY: original async/range mutexes and program-state diagnostic owner admit
// mutation. Stable worker/request addresses retain the original execution life.
unsafe impl<T> Sync for Cell<T> {}
static STATE: Cell<State> = Cell(UnsafeCell::new(State {
    requests: [const { Request::EMPTY }; CAPACITY],
    workers: [const { Worker::empty() }; WORKERS],
    cursor: [0; 2],
    lanes: [Lane::EMPTY; 2],
    creations: 0,
    processed: 0,
    ranges: [const { Range::EMPTY }; 64],
    next_id: 1,
    armed: false,
    consumed: false,
    remaining: 0,
    retry_id: 0,
}));
static LOCK: Cell<Mutex> = Cell(UnsafeCell::new(Mutex {
    rank: 100,
    mode: 0,
    name: Text::literal(b"r4x-async-io"),
    ..Mutex::EMPTY
}));
static RANGE_LOCK: Cell<Mutex> = Cell(UnsafeCell::new(Mutex {
    rank: 100,
    mode: 0,
    name: Text::literal(b"r4x-file-range-lock"),
    ..Mutex::EMPTY
}));
fn state() -> *mut State {
    STATE.0.get()
}
struct Locked(*mut Mutex);
impl Locked {
    fn acquire(lock: *mut Mutex) -> Option<Self> {
        if unsafe { sync_locks::r4os_rust_sync_mutex(lock, 1, u64::MAX) } {
            Some(Self(lock))
        } else {
            None
        }
    }
}
impl Drop for Locked {
    fn drop(&mut self) {
        unsafe { sync_locks::r4os_rust_sync_mutex(self.0, 2, 0) };
    }
}
fn lock() -> Option<Locked> {
    Locked::acquire(LOCK.0.get())
}
// All three operations require the existing Async owner. Unpublished failure
// paths have enqueued=false; terminal reset cannot silently lose a count.
unsafe fn enqueue(req: *mut Request) {
    unsafe {
        assert!((*req).used && (*req).state == 1 && !(*req).enqueued);
        (*state()).lanes[usize::from((*req).kind == Kind::Service)].enqueue();
        (*req).enqueued = true;
    }
}
unsafe fn unqueue(req: *mut Request) {
    unsafe {
        if (*req).enqueued {
            assert!((*req).used && (*req).state == 1);
            (*state()).lanes[usize::from((*req).kind == Kind::Service)].remove_request();
            (*req).enqueued = false;
        }
    }
}
unsafe fn clear_request(req: *mut Request) {
    unsafe {
        unqueue(req);
        service::finished(req);
        req.write(Request::EMPTY);
    }
}

fn retry_allowed() -> bool {
    unsafe {
        let mut value = Bytes {
            data: ptr::null(),
            len: 0,
        };
        if !program_async_option(false, &mut value)
            || !slice::from_raw_parts(value.data, value.len).eq_ignore_ascii_case(b"yes")
        {
            return false;
        }
        !program_async_option(true, &mut value)
            || slice::from_raw_parts(value.data, value.len).eq_ignore_ascii_case(b"yes")
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_async_initialize() {
    unsafe {
        (*state()).armed = retry_allowed();
        (*state()).consumed = false;
        (*state()).remaining = if (*state()).armed { RETRIES + 1 } else { 0 };
        (*state()).retry_id = 0;
    }
}
unsafe fn allocate_id() -> u32 {
    unsafe {
        let id = (*state()).next_id;
        (*state()).next_id = id.wrapping_add(1);
        if (*state()).next_id == 0 {
            (*state()).next_id = 1;
        }
        if id == 0 { 1 } else { id }
    }
}
unsafe fn slot() -> *mut Request {
    unsafe {
        for i in 0..CAPACITY {
            let req = ptr::addr_of_mut!((*state()).requests[i]);
            if !(*req).used {
                return req;
            }
        }
        ptr::null_mut()
    }
}
unsafe fn worker_for(task: *mut Task) -> *mut Worker {
    unsafe {
        if task.is_null() {
            return ptr::null_mut();
        }
        let core = task_registry::core(task);
        if (*core).execution_owner_kind != ExecutionOwnerKind::AsyncIo
            || (*core).execution_owner_context.is_null()
        {
            return ptr::null_mut();
        }
        let worker = (*core).execution_owner_context.cast::<Worker>();
        if !(0..WORKERS).any(|i| ptr::eq(ptr::addr_of!((*state()).workers[i]), worker)) {
            return ptr::null_mut();
        }
        if (*worker).task_id != (*core).id || (*worker).task_generation != (*core).generation {
            return ptr::null_mut();
        }
        worker
    }
}
/// # Safety
/// Runtime retains this exact live Task/generation. A worker slot is reset
/// only after release_task has retired that Task. Its existing atomic request
/// binding spans every foreign operation/abort; null therefore admits only
/// the guard-free idle or returned-callback boundary, never a live backend.
pub(crate) unsafe fn idle_transfer(task: *mut Task) -> bool {
    unsafe {
        let worker = worker_for(task);
        !worker.is_null() && (*worker).request.load(Ordering::Acquire).is_null()
    }
}
unsafe fn request_for(task: *mut Task) -> *mut Request {
    unsafe {
        let worker = worker_for(task);
        if worker.is_null() {
            return ptr::null_mut();
        }
        let req = (*worker).request.load(Ordering::Acquire);
        if req.is_null() {
            return req;
        }
        let core = task_registry::core(task);
        if !(*req).used
            || (*req).task_id != (*core).id
            || (*req).task_generation != (*core).generation
        {
            return ptr::null_mut();
        }
        req
    }
}
unsafe fn current() -> *mut Request {
    unsafe { request_for(scheduler::current().cast()) }
}
#[repr(C)]
pub struct RequestView {
    pub instance: *mut ProgramInstance,
    pub path: Bytes,
}
/// # Safety
/// Exact retained Task lifetime and exclusive resident bounded view output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_async_view(
    task: *mut Task,
    out: *mut RequestView,
) -> bool {
    unsafe {
        let req = request_for(task);
        if req.is_null() {
            return false;
        }
        out.write(RequestView {
            instance: (*req).owner_instance,
            path: Bytes {
                data: (*req).path.as_ptr(),
                len: (*req).path_len,
            },
        });
        true
    }
}
pub(super) unsafe fn execution() -> Option<super::context::AsyncExecution> {
    unsafe {
        let req = current();
        if req.is_null() {
            None
        } else {
            Some(super::context::AsyncExecution {
                instance: (*req).owner_instance,
                id: (*req).owner_id,
                reserved: 0,
                generation: (*req).owner_generation,
            })
        }
    }
}
unsafe fn ensure_workers(service: bool) -> bool {
    unsafe {
        let mut available = false;
        for index in 0..WORKERS {
            if (index >= WORKERS / 2) != service {
                continue;
            }
            let worker = ptr::addr_of_mut!((*state()).workers[index]);
            if (*worker).retiring {
                continue;
            }
            if (*worker).task_id == 0 {
                // A reused slot is reached only after exact Task retirement.
                (*state()).lanes[usize::from(service)].withdraw(index % dispatch::WIDTH);
                if service {
                    service::withdraw(index % dispatch::WIDTH);
                }
                sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*worker).event), 1, 0);
                let name = b"r4x-async-io";
                let request = task_api::CreateRequest {
                    name: name.as_ptr(),
                    name_len: name.len(),
                    state: 3,
                    role: if service { 1 } else { 3 },
                    needs_fpu: true,
                    smp_eligible: true,
                    smp_r4x: false,
                    owner_kind: 0,
                    owner_context: ptr::null_mut(),
                    entry: Some(worker_main),
                };
                let mut failure = 0;
                let task = task_api::r4os_rust_task_create(&request, false, &mut failure);
                if task.is_null() {
                    continue;
                }
                assert!(
                    task_api::r4os_rust_task_mutate(
                        23,
                        task,
                        ExecutionOwnerKind::AsyncIo as u64,
                        worker.addr() as u64,
                        ptr::null(),
                        0
                    ) != 0,
                    "async worker owner binding"
                );
                let core = task_registry::core(task.cast());
                (*worker).task_id = (*core).id;
                (*worker).task_generation = (*core).generation;
                (*worker).service = service;
                (*state()).creations = (*state()).creations.wrapping_add(1);
                assert!(
                    crate::sched::scheduler::publish_created(task),
                    "owned async worker first publication"
                );
            }
            available = true;
        }
        available
    }
}
unsafe fn wake_workers(service: bool) {
    unsafe {
        let lane = usize::from(service);
        let base = lane * dispatch::WIDTH;
        let mut active = 0;
        for slot in 0..dispatch::WIDTH {
            let worker = ptr::addr_of!((*state()).workers[base + slot]);
            if (*worker).task_id != 0
                && !(*worker).retiring
                && (*worker).service == service
                && (*worker).request.load(Ordering::Acquire).is_null()
            {
                active |= 1 << slot;
            }
        }
        while let Some(slot) = (*state()).lanes[lane].reserve(active) {
            let worker = ptr::addr_of_mut!((*state()).workers[base + slot]);
            sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*worker).event), 0, 0);
        }
    }
}
unsafe fn take_request(worker: *mut Worker) -> *mut Request {
    unsafe {
        let lane = usize::from((*worker).service);
        let queued = (*state()).lanes[lane].unreserved();
        if !(*worker).service && !queued {
            return ptr::null_mut();
        }
        let now = if (*worker).service {
            timer::tick_count()
        } else {
            0
        };
        let start = (*state()).cursor[lane];
        for offset in 0..CAPACITY {
            let index = (start + offset) % CAPACITY;
            let req = ptr::addr_of_mut!((*state()).requests[index]);
            if !(*req).used
                || (*req).cancel_requested
                || ((*req).kind == Kind::Service) != (*worker).service
            {
                continue;
            }
            let first = (*req).state == 1 && queued;
            let suspended = (*worker).service && (*req).state == 2 && (*req).task_id == 0;
            if !first && !suspended {
                continue;
            }
            let epoch = if (*worker).service {
                service::epoch((*req).service_handle)
            } else {
                0
            };
            let resume = suspended && ((*req).service_epoch != epoch || service::expired(req, now));
            if !first && !resume {
                continue;
            }
            if first {
                unqueue(req);
                (*state()).processed = (*state()).processed.wrapping_add(1);
            }
            (*req).task_id = (*worker).task_id;
            (*req).task_generation = (*worker).task_generation;
            (*req).last_worker_id = (*worker).task_id;
            (*req).state = 2;
            // Snapshot BEFORE the bounded attempt: a concurrent endpoint change
            // must remain visible after this worker releases the continuation.
            if (*worker).service {
                (*req).service_epoch = epoch;
            }
            (*worker).request.store(req, Ordering::Release);
            (*state()).cursor[lane] = (index + 1) % CAPACITY;
            return req;
        }
        ptr::null_mut()
    }
}
unsafe extern "C" fn worker_main() {
    unsafe {
        let task = scheduler::current();
        if task.is_null() {
            return;
        }
        let core = task_registry::core(task.cast());
        let worker = (*core).execution_owner_context.cast::<Worker>();
        if worker.is_null() {
            return;
        }
        let Some(index) =
            (0..WORKERS).find(|&i| ptr::eq(ptr::addr_of!((*state()).workers[i]), worker))
        else {
            return;
        };
        let lane = usize::from((*worker).service);
        let slot = index % dispatch::WIDTH;
        assert_eq!(index / dispatch::WIDTH, lane);
        loop {
            let Some(guard) = lock() else {
                threads::exit_task()
            };
            (*state()).lanes[lane].withdraw(slot);
            if (*worker).retiring {
                drop(guard);
                threads::exit_task();
            }
            let notified = (*worker).service && service::enroll(slot);
            let req = take_request(worker);
            if req.is_null() {
                (*state()).lanes[lane].park(slot);
                let timeout = if (*worker).service {
                    service::wait_ticks()
                } else {
                    u64::MAX
                };
                drop(guard);
                sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*worker).event), 2, timeout);
                continue;
            }
            if (*worker).service {
                let during_check = service::withdraw(slot);
                if notified || during_check {
                    service::wake_pending();
                }
            }
            let observation = crate::sched::worker_observation::begin(
                if (*worker).service {
                    crate::sched::worker_observation::SERVICE
                } else {
                    crate::sched::worker_observation::FILE
                },
                (*req).owner_id,
                (*req).owner_generation,
                u64::from((*req).id),
            );
            drop(guard);
            let outcome = if (*worker).service {
                service::step(req)
            } else {
                service::Step::Done(perform(req))
            };
            let Some(mut guard) = lock() else {
                threads::exit_task()
            };
            if (*worker).retiring {
                drop(guard);
                threads::exit_task();
            }
            if matches!(outcome, service::Step::Pending) && !(*req).cancel_requested {
                crate::sched::worker_observation::finish(observation, 0, false);
                (*worker).request.store(ptr::null_mut(), Ordering::Release);
                (*req).task_id = 0;
                (*req).task_generation = 0;
                wake_workers(true);
                drop(guard);
                scheduler::safe_reschedule();
                continue;
            }
            if (*worker).service && (*req).cancel_requested {
                // Cancellation cannot publish a terminal completion while an
                // endpoint still owns the exact submitted request. Never drop
                // that identity before the actual endpoint cancellation ack.
                drop(guard);
                service::abort(req);
                guard = match lock() {
                    Some(g) => g,
                    None => threads::exit_task(),
                };
                if (*worker).retiring {
                    drop(guard);
                    threads::exit_task();
                }
            }
            let result = match outcome {
                service::Step::Done(result) => result,
                service::Step::Pending => CANCELLED,
            };
            if (*req).cancel_requested {
                (*req).result = CANCELLED;
                (*req).status = CANCELLED;
                (*req).processed_bytes = 0;
                (*req).state = 4;
            } else {
                (*req).result = result;
                (*req).status = 0;
                (*req).processed_bytes = if result > 0 { result as u64 } else { 0 };
                (*req).state = 3;
            }
            service::finished(req);
            // Detach before completing: a woken caller can immediately close/recycle.
            (*worker).request.store(ptr::null_mut(), Ordering::Release);
            (*req).task_id = 0;
            (*req).task_generation = 0;
            (*req).completed_tick = timer::tick_count();
            // The actual terminal native result, before its existing waiter
            // publication; this observer owns no request or cancellation duty.
            crate::sched::worker_observation::finish(observation, result, true);
            sync_objects::r4os_rust_sync_completion(ptr::addr_of_mut!((*req).completion), 1, 0);
            drop(guard);
            scheduler::safe_reschedule();
        }
    }
}
unsafe fn base_request(
    req: *mut Request,
    kind: Kind,
    instance: *mut ProgramInstance,
    handle: Handle,
    thread: *mut super::threads::Thread,
    flags: u32,
    requested: u64,
) {
    unsafe {
        req.write(Request {
            used: true,
            id: allocate_id(),
            owner_id: (*instance).id,
            owner_generation: handle.generation,
            owner_instance: instance,
            caller_id: (*thread).task_id,
            caller_generation: (*thread).task_generation,
            kind,
            state: 1,
            flags,
            requested_bytes: requested,
            submitted_tick: timer::tick_count(),
            ..Request::EMPTY
        });
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn submit(
    kind: Kind,
    path: *const u8,
    offset: u64,
    data: usize,
    len: u64,
    out: usize,
    out_len: u64,
    flags: u32,
    id: *mut u32,
) -> i32 {
    unsafe {
        if id.is_null() || path.is_null() {
            return INVALID;
        }
        id.write(0);
        if kind == Kind::Lock {
            if flags & !c::io_file_lock_flag_unlock != 0 || len == 0 {
                return INVALID;
            }
        } else if flags != 0
            && !matches!(
                kind,
                Kind::StreamBegin | Kind::StreamWrite | Kind::StreamFinish
            )
        {
            return UNSUPPORTED;
        }
        if matches!(
            kind,
            Kind::Write | Kind::Append | Kind::WriteAt | Kind::StreamWrite
        ) && len != 0
            && data == 0
        {
            return INVALID;
        }
        if matches!(kind, Kind::Read | Kind::ReadAt) && out_len != 0 && out == 0 {
            return INVALID;
        }
        if kind != Kind::Lock
            && (!super::system::io::transfer_len_fits(len)
                || !super::system::io::transfer_len_fits(out_len))
        {
            return TOO_LARGE;
        }
        let instance = context::current();
        if instance.is_null()
            || AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire)
        {
            return NO_INSTANCE;
        }
        let Some(handle) = context::handle() else {
            return NO_INSTANCE;
        };
        let thread = threads::current();
        if thread.is_null() {
            return NO_INSTANCE;
        }
        let Some(guard) = lock() else {
            return BUSY;
        };
        let req = slot();
        if req.is_null() {
            return NO_SLOTS;
        }
        base_request(
            req,
            kind,
            instance,
            handle,
            thread,
            flags,
            if out_len != 0 { out_len } else { len },
        );
        (*req).offset = offset;
        (*req).data = data;
        (*req).data_len = len;
        (*req).out = out;
        (*req).out_len = out_len;
        let mut n = 0;
        while n + 1 < PATH && path.add(n).read() != 0 {
            (*req).path[n] = path.add(n).read();
            n += 1;
        }
        if path.add(n).read() != 0 {
            clear_request(req);
            return if kind == Kind::StreamBegin {
                c::file_stream_error_name_too_long
            } else {
                INVALID
            };
        }
        (*req).path[n] = 0;
        (*req).path_len = n;
        if !ensure_workers((*req).kind == Kind::Service) {
            clear_request(req);
            return SPAWN_FAILED;
        }
        id.write((*req).id);
        enqueue(req);
        wake_workers((*req).kind == Kind::Service);
        drop(guard);
        0
    }
}
/// # Safety
/// Original resident API buffers and exclusive request ID retain caller lifetime.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn r4os_rust_program_async_file(
    kind: u32,
    path: *const u8,
    offset: u64,
    data: usize,
    len: u64,
    out: usize,
    out_len: u64,
    flags: u32,
    synchronous: bool,
    id: *mut u32,
) -> i32 {
    unsafe {
        let mut local = 0;
        let target = if synchronous { &mut local } else { id };
        let result = submit(
            Kind::decode(kind),
            path,
            offset,
            data,
            len,
            out,
            out_len,
            flags,
            target,
        );
        if result != 0 || !synchronous {
            result
        } else {
            wait_close(local)
        }
    }
}
#[allow(clippy::too_many_arguments)]
unsafe fn submit_service(
    handle: u32,
    op: u16,
    request: *const u8,
    len: u32,
    header: *mut Header,
    response: *mut u8,
    capacity: u32,
    timeout: u64,
    flags: u32,
    id: *mut u32,
) -> i32 {
    unsafe {
        if id.is_null() {
            return INVALID;
        }
        id.write(0);
        if flags != 0 {
            return UNSUPPORTED;
        }
        if header.is_null() {
            return c::service_api_result_invalid;
        }
        header.write(Header {
            magic: 0,
            version: 0,
            ..Header::default()
        });
        if len as usize > c::service_api_max_payload {
            return c::service_api_result_payload_too_large;
        }
        if capacity as usize > c::service_api_max_payload
            || (len != 0 && request.is_null())
            || response.is_null()
        {
            return c::service_api_result_invalid;
        }
        let instance = context::current();
        if instance.is_null()
            || AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire)
        {
            return NO_INSTANCE;
        }
        let Some(owner) = context::handle() else {
            return NO_INSTANCE;
        };
        let thread = threads::current();
        if thread.is_null() {
            return NO_INSTANCE;
        }
        let Some(guard) = lock() else {
            return BUSY;
        };
        // Serialize publication against owner cancellation. A caller that
        // waited for this mutex cannot publish after the reaper's completed
        // cancellation pass merely because its earlier done snapshot was live.
        if AtomicBool::from_ptr(ptr::addr_of_mut!((*instance).done)).load(Ordering::Acquire) {
            return NO_INSTANCE;
        }
        let req = slot();
        if req.is_null() {
            return NO_SLOTS;
        }
        base_request(
            req,
            Kind::Service,
            instance,
            owner,
            thread,
            flags,
            u64::from(len),
        );
        (*req).service_handle = handle;
        (*req).service_op = op;
        (*req).service_request = request.addr();
        (*req).service_request_len = len;
        (*req).service_response_header = header.addr();
        (*req).service_response = response.addr();
        (*req).service_response_capacity = capacity;
        (*req).service_timeout = timeout;
        (*req).service_deadline =
            crate::platform::clock_math::finite_deadline((*req).submitted_tick, timeout);
        service::interested(req);
        if !ensure_workers(true) {
            clear_request(req);
            return SPAWN_FAILED;
        }
        id.write((*req).id);
        enqueue(req);
        wake_workers(true);
        drop(guard);
        0
    }
}
/// # Safety
/// Original canonical service buffers remain live through explicit completion.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn r4os_rust_program_async_service(
    handle: u32,
    op: u16,
    request: *const u8,
    len: u32,
    header: *mut Header,
    response: *mut u8,
    capacity: u32,
    timeout: u64,
    flags: u32,
    synchronous: bool,
    id: *mut u32,
) -> i32 {
    unsafe {
        let mut local = 0;
        let target = if synchronous { &mut local } else { id };
        let result = submit_service(
            handle, op, request, len, header, response, capacity, timeout, flags, target,
        );
        if result != 0 || !synchronous {
            result
        } else {
            wait_close(local)
        }
    }
}
unsafe fn for_caller(id: u32) -> *mut Request {
    unsafe {
        if id == 0 {
            return ptr::null_mut();
        }
        let instance = context::current();
        if instance.is_null() {
            return ptr::null_mut();
        }
        let Some(handle) = context::handle() else {
            return ptr::null_mut();
        };
        let thread = threads::current();
        for i in 0..CAPACITY {
            let req = ptr::addr_of_mut!((*state()).requests[i]);
            if !(*req).used
                || (*req).close_pending
                || (*req).id != id
                || (*req).owner_id != (*instance).id
                || (*req).owner_generation != handle.generation
            {
                continue;
            }
            if (*req).kind.stream()
                && (thread.is_null()
                    || (*req).caller_id != (*thread).task_id
                    || (*req).caller_generation != (*thread).task_generation)
            {
                continue;
            }
            return req;
        }
        ptr::null_mut()
    }
}
unsafe fn info(req: *const Request) -> Info {
    unsafe {
        Info {
            request_id: (*req).id,
            kind: (*req).kind as u32,
            state: (*req).state,
            flags: (*req).flags,
            status: (*req).status,
            result: (*req).result,
            requested_bytes: (*req).requested_bytes,
            processed_bytes: (*req).processed_bytes,
            submitted_tick: (*req).submitted_tick,
            completed_tick: (*req).completed_tick,
            owner_instance: (*req).owner_id,
            task_id: (*req).last_worker_id,
            ..Info::default()
        }
    }
}
unsafe fn status(id: u32, out: *mut Info) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let Some(guard) = lock() else {
            return BUSY;
        };
        let req = for_caller(id);
        if req.is_null() {
            return NOT_FOUND;
        }
        let value = info(req);
        drop(guard);
        out.write(value);
        0
    }
}
unsafe fn wait(id: u32, timeout: u64, out: *mut Info) -> i32 {
    unsafe {
        if out.is_null() {
            return INVALID;
        }
        let Some(guard) = lock() else {
            return BUSY;
        };
        let req = for_caller(id);
        if req.is_null() {
            return NOT_FOUND;
        }
        if !matches!((*req).state, 1 | 2) {
            let value = info(req);
            let result = (*req).status;
            drop(guard);
            out.write(value);
            return result;
        }
        (*req).waiters = (*req).waiters.saturating_add(1);
        let completion = ptr::addr_of_mut!((*req).completion);
        drop(guard);
        let waited = sync_objects::r4os_rust_sync_completion(completion, 4, timeout);
        let Some(guard) = lock() else {
            return BUSY;
        };
        let req = for_caller(id);
        if req.is_null() {
            return NOT_FOUND;
        }
        if (*req).waiters != 0 {
            (*req).waiters -= 1;
        }
        let complete = !matches!((*req).state, 1 | 2);
        if !complete {
            match waited {
                2 => return TIMEOUT,
                3 | 4 => return CANCELLED,
                1 => {}
                _ => return BUSY,
            }
        }
        let value = info(req);
        let result = (*req).status;
        drop(guard);
        out.write(value);
        result
    }
}
#[derive(Clone, Copy)]
struct Claim {
    slot: usize,
    id: u32,
    owner_id: u32,
    owner_generation: u64,
    caller_id: u32,
    caller_generation: u64,
    task_id: u32,
    task_generation: u64,
    service_handle: u32,
    service_request_id: u32,
}
unsafe fn claim(slot: usize, req: *const Request) -> Claim {
    unsafe {
        Claim {
            slot,
            id: (*req).id,
            owner_id: (*req).owner_id,
            owner_generation: (*req).owner_generation,
            caller_id: (*req).caller_id,
            caller_generation: (*req).caller_generation,
            task_id: (*req).task_id,
            task_generation: (*req).task_generation,
            service_handle: (*req).service_handle,
            service_request_id: if (*req).kind == Kind::Service {
                (*req).service_request_id
            } else {
                0
            },
        }
    }
}
unsafe fn claimed(value: Claim) -> *mut Request {
    unsafe {
        if value.slot >= CAPACITY {
            return ptr::null_mut();
        }
        let req = ptr::addr_of_mut!((*state()).requests[value.slot]);
        if !(*req).used
            || (*req).id != value.id
            || (*req).owner_id != value.owner_id
            || (*req).owner_generation != value.owner_generation
            || (*req).caller_id != value.caller_id
            || (*req).caller_generation != value.caller_generation
            || (*req).task_id != value.task_id
            || (*req).task_generation != value.task_generation
        {
            ptr::null_mut()
        } else {
            req
        }
    }
}
unsafe fn index(wanted: *const Request) -> Option<usize> {
    unsafe { (0..CAPACITY).find(|&i| ptr::eq(ptr::addr_of!((*state()).requests[i]), wanted)) }
}
unsafe fn retry_injection(req: *const Request) -> u8 {
    unsafe {
        if (*req).kind != Kind::Write
            || !(&(*req).path)[..(*req).path_len].eq_ignore_ascii_case(b"C:\\TEMP\\ASYNIOR.TXT")
            || !retry_allowed()
        {
            return 0;
        }
        let _guard = Guard::acquire(0);
        if !(*state()).armed {
            if (*state()).consumed {
                return 0;
            }
            (*state()).armed = true;
            (*state()).remaining = RETRIES + 1;
        }
        if (*state()).retry_id == 0 {
            (*state()).retry_id = (*req).id;
        }
        if (*state()).retry_id != (*req).id || (*state()).remaining == 0 {
            return 0;
        }
        (*state()).remaining -= 1;
        let first = (*state()).remaining == RETRIES;
        if (*state()).remaining == 0 {
            (*state()).armed = false;
            (*state()).consumed = true;
        }
        if first { 1 } else { 2 }
    }
}
fn finish_retry(id: u32) -> bool {
    let _guard = Guard::acquire(0);
    unsafe {
        if id == 0 || (*state()).retry_id != id {
            return false;
        }
        (*state()).retry_id = 0;
        true
    }
}
fn recovered(id: u32) {
    foundation::puts(b"ASYNCIO05910 retire retry: recovered request=");
    foundation::dec(u64::from(id));
    foundation::puts(b"\r\n");
}
fn signal_reaper() {
    unsafe { sync_objects::r4os_rust_sync_event((*registry_state::state()).event, 0, 0) };
}
unsafe fn retire(value: Claim) -> bool {
    unsafe {
        if value.task_id == 0 {
            return true;
        }
        let Some(guard) = lock() else {
            return false;
        };
        let req = claimed(value);
        if req.is_null() {
            return true;
        }
        let mut target = ptr::null_mut();
        for i in 0..WORKERS {
            let worker = ptr::addr_of_mut!((*state()).workers[i]);
            if (*worker).task_id != value.task_id
                || (*worker).task_generation != value.task_generation
            {
                continue;
            }
            if !(*req).cancel_requested {
                return false;
            }
            (*worker).retiring = true;
            (*state()).lanes[usize::from((*worker).service)].withdraw(i % dispatch::WIDTH);
            if (*worker).service {
                service::withdraw(i % dispatch::WIDTH);
                service::wake_pending();
            }
            wake_workers((*worker).service);
            target = worker;
            break;
        }
        drop(guard);
        if target.is_null() {
            return true;
        }
        let current = scheduler::current();
        let (id, generation) = if current.is_null() {
            (0, 0)
        } else {
            let core = task_registry::core(current.cast());
            ((*core).id, (*core).generation)
        };
        if !thread_retire::release_task(value.task_id, value.task_generation, id, generation) {
            return false;
        }
        let Some(_guard) = lock() else {
            return false;
        };
        if (*target).task_id == value.task_id && (*target).task_generation == value.task_generation
        {
            (*target).request.store(ptr::null_mut(), Ordering::Release);
            (*target).task_id = 0;
            (*target).task_generation = 0;
            (*target).retiring = false;
            if !ensure_workers((*target).service) {
                return false;
            }
            wake_workers((*target).service);
        }
        true
    }
}
// 0 busy,1 already closed,2 durably transferred to the reaper.
unsafe fn defer_close(value: Claim) -> u8 {
    unsafe {
        let Some(guard) = lock() else {
            return 0;
        };
        let req = claimed(value);
        if req.is_null() {
            return 1;
        }
        if matches!((*req).state, 1 | 2) || (*req).waiters != 0 {
            return 0;
        }
        (*req).close_pending = true;
        drop(guard);
        signal_reaper();
        2
    }
}
unsafe fn mark_pending(id: u32) -> bool {
    unsafe {
        let Some(guard) = lock() else {
            return false;
        };
        let req = for_caller(id);
        if req.is_null() || matches!((*req).state, 1 | 2) || (*req).waiters != 0 {
            return false;
        }
        (*req).close_pending = true;
        drop(guard);
        signal_reaper();
        true
    }
}
unsafe fn close(id: u32, defer_on_busy: bool) -> i32 {
    unsafe {
        let Some(guard) = lock() else {
            return BUSY;
        };
        let req = for_caller(id);
        if req.is_null() {
            return NOT_FOUND;
        }
        if matches!((*req).state, 1 | 2) || (*req).waiters != 0 {
            return BUSY;
        }
        let Some(slot) = index(req) else {
            return NOT_FOUND;
        };
        let value = claim(slot, req);
        let injection = retry_injection(req);
        drop(guard);
        if injection != 0 {
            if injection == 1 {
                foundation::puts(b"ASYNCIO05910 retire retry: injected request=");
                foundation::dec(u64::from(value.id));
                foundation::puts(b" attempts=");
                foundation::dec(u64::from(RETRIES + 1));
                foundation::puts(b"\r\n");
            }
            return BUSY;
        }
        if !retire(value) {
            if !defer_on_busy {
                return BUSY;
            }
            return if defer_close(value) == 0 { BUSY } else { 0 };
        }
        let Some(guard) = lock() else {
            return BUSY;
        };
        let req = claimed(value);
        if !req.is_null() {
            clear_request(req);
        }
        drop(guard);
        if finish_retry(value.id) {
            recovered(value.id);
        }
        0
    }
}
unsafe fn wait_close(id: u32) -> i32 {
    unsafe {
        let mut info = Info::default();
        let waited = wait(id, u64::MAX, &mut info);
        let mut retries = 0;
        let mut closed = close(id, false);
        while closed == BUSY && retries < RETRIES {
            retries += 1;
            let reason = b"async-io-close-retry";
            scheduler::r4os_rust_scheduler_task(
                4,
                ptr::null_mut(),
                1,
                0,
                reason.as_ptr(),
                reason.len(),
            );
            closed = close(id, false);
        }
        if closed == BUSY {
            if !mark_pending(id) {
                return BUSY;
            }
            foundation::puts(b"ASYNCIO05910 sync close: deferred request=");
            foundation::dec(u64::from(id));
            foundation::puts(b" retries=");
            foundation::dec(u64::from(RETRIES));
            foundation::puts(b"\r\n");
            return if waited != 0 { waited } else { info.result };
        }
        if closed != 0 {
            closed
        } else if waited != 0 {
            waited
        } else {
            info.result
        }
    }
}
/// # Safety
/// Status/wait output is canonical resident exclusive storage for the caller.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_async_request(
    op: u8,
    id: u32,
    timeout: u64,
    out: *mut Info,
) -> i32 {
    unsafe {
        match op {
            0 => status(id, out),
            1 => wait(id, timeout, out),
            2 => close(id, true),
            3 => wait_close(id),
            _ => unreachable!(),
        }
    }
}
unsafe fn cancel_queued(req: *mut Request) {
    unsafe {
        (*req).cancel_requested = true;
        if (*req).state != 1 {
            return;
        }
        unqueue(req);
        (*req).status = CANCELLED;
        (*req).result = CANCELLED;
        (*req).state = 4;
        service::finished(req);
        (*req).completed_tick = timer::tick_count();
        sync_objects::r4os_rust_sync_completion(ptr::addr_of_mut!((*req).completion), 1, 0);
    }
}
unsafe fn owned(req: *const Request, handle: Handle, caller: Option<(u32, u64)>) -> bool {
    unsafe {
        (*req).used
            && (*req).owner_id == handle.instance_id
            && (*req).owner_generation == handle.generation
            && caller.is_none_or(|(id, generation)| {
                (*req).caller_id == id && (*req).caller_generation == generation
            })
    }
}
unsafe fn queue(req: *mut Request, op: u8) -> u64 {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            ptr::addr_of_mut!((*req).completion.queue),
            op,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
        )
    }
}
pub(super) unsafe fn cancel(handle: Handle, caller: Option<(u32, u64)>) -> bool {
    unsafe {
        loop {
            let Some(guard) = lock() else {
                return false;
            };
            for i in 0..CAPACITY {
                let req = ptr::addr_of_mut!((*state()).requests[i]);
                if owned(req, handle, caller) {
                    cancel_queued(req);
                }
            }
            let mut pending = None;
            for i in 0..CAPACITY {
                let req = ptr::addr_of_mut!((*state()).requests[i]);
                if !owned(req, handle, caller) {
                    continue;
                }
                cancel_queued(req);
                if (*req).task_id == 0 && !((*req).kind == Kind::Service && (*req).state == 2) {
                    continue;
                }
                pending = Some(claim(i, req));
                break;
            }
            drop(guard);
            let Some(value) = pending else {
                return true;
            };
            if value.service_request_id != 0 {
                program_async_service_control(
                    false,
                    value.service_handle,
                    value.service_request_id,
                    0,
                );
            }
            if !retire(value) {
                return false;
            }
            let Some(_guard) = lock() else {
                return false;
            };
            let req = claimed(value);
            if !req.is_null() {
                (*req).task_id = 0;
                (*req).task_generation = 0;
                (*req).service_request_id = 0;
                if matches!((*req).state, 1 | 2) {
                    (*req).status = CANCELLED;
                    (*req).result = CANCELLED;
                    (*req).processed_bytes = 0;
                    (*req).state = 4;
                    service::finished(req);
                    (*req).completed_tick = timer::tick_count();
                    sync_objects::r4os_rust_sync_completion(
                        ptr::addr_of_mut!((*req).completion),
                        1,
                        0,
                    );
                }
                if caller.is_none() && (*req).waiters == 0 && queue(req, 6) == 0 {
                    clear_request(req);
                }
            }
        }
    }
}
pub(super) unsafe fn purge(handle: Handle, caller: Option<(u32, u64)>) -> bool {
    unsafe {
        if caller.is_some() {
            let Some(_guard) = lock() else {
                return false;
            };
            for i in 0..CAPACITY {
                let req = ptr::addr_of_mut!((*state()).requests[i]);
                if !owned(req, handle, caller) {
                    continue;
                }
                if !(*req).cancel_requested
                    || (*req).task_id != 0
                    || (*req).task_generation != 0
                    || ((*req).kind == Kind::Service && matches!((*req).state, 1 | 2))
                {
                    return false;
                }
                queue(req, 3);
                (*req).waiters = 0;
                clear_request(req);
            }
            return true;
        }
        loop {
            let Some(guard) = lock() else {
                return false;
            };
            let mut pending = None;
            for i in 0..CAPACITY {
                let req = ptr::addr_of_mut!((*state()).requests[i]);
                if !owned(req, handle, None) {
                    continue;
                }
                // A detached Service continuation owns client pointers even
                // without a worker Task. Only cancel's real endpoint/drain
                // acknowledgment can make it purgeable.
                if (*req).kind == Kind::Service && matches!((*req).state, 1 | 2) {
                    return false;
                }
                (*req).cancel_requested = true;
                unqueue(req);
                pending = Some(claim(i, req));
                break;
            }
            drop(guard);
            let Some(value) = pending else {
                return true;
            };
            if !retire(value) {
                return false;
            }
            let Some(_guard) = lock() else {
                return false;
            };
            let req = claimed(value);
            if !req.is_null() {
                queue(req, 3);
                (*req).waiters = 0;
                clear_request(req);
            }
        }
    }
}
pub(super) unsafe fn reap() -> u8 {
    unsafe {
        let Some(guard) = lock() else {
            return 2;
        };
        let mut pending = None;
        for i in 0..CAPACITY {
            let req = ptr::addr_of_mut!((*state()).requests[i]);
            if !(*req).used
                || !(*req).close_pending
                || (*req).waiters != 0
                || matches!((*req).state, 1 | 2)
            {
                continue;
            }
            pending = Some(claim(i, req));
            break;
        }
        drop(guard);
        let Some(value) = pending else {
            return 0;
        };
        if !retire(value) {
            return 2;
        }
        let Some(guard) = lock() else {
            return 2;
        };
        let req = claimed(value);
        let mut reclaimed = false;
        if !req.is_null() && (*req).close_pending {
            clear_request(req);
            reclaimed = true;
        }
        drop(guard);
        if reclaimed && finish_retry(value.id) {
            recovered(value.id);
        }
        1
    }
}
fn path_byte(value: u8) -> u8 {
    if value == b'/' {
        b'\\'
    } else {
        value.to_ascii_uppercase()
    }
}
unsafe fn path_equal(range: *const Range, req: *const Request) -> bool {
    unsafe {
        (*range).path_len == (*req).path_len
            && (0..(*range).path_len)
                .all(|i| path_byte((*range).path[i]) == path_byte((*req).path[i]))
    }
}
fn overlap(a: u64, an: u64, b: u64, bn: u64) -> bool {
    a < b.saturating_add(bn) && b < a.saturating_add(an)
}
unsafe fn file_lock(req: *const Request) -> i32 {
    unsafe {
        if (*req).path_len == 0
            || (*req).data_len == 0
            || (*req).flags & !c::io_file_lock_flag_unlock != 0
        {
            return INVALID;
        }
        let Some(_guard) = Locked::acquire(RANGE_LOCK.0.get()) else {
            return BUSY;
        };
        if (*req).flags & c::io_file_lock_flag_unlock != 0 {
            for i in 0..64 {
                let entry = ptr::addr_of_mut!((*state()).ranges[i]);
                if !(*entry).used
                    || (*entry).owner_id != (*req).owner_id
                    || (*entry).owner_generation != (*req).owner_generation
                    || (*entry).offset != (*req).offset
                    || (*entry).length != (*req).data_len
                    || !path_equal(entry, req)
                {
                    continue;
                }
                entry.write(Range::EMPTY);
                return 0;
            }
            return c::io_error_lock_violation;
        }
        for i in 0..64 {
            let entry = ptr::addr_of!((*state()).ranges[i]);
            if !(*entry).used
                || !path_equal(entry, req)
                || ((*entry).owner_id == (*req).owner_id
                    && (*entry).owner_generation == (*req).owner_generation)
            {
                continue;
            }
            if overlap(
                (*entry).offset,
                (*entry).length,
                (*req).offset,
                (*req).data_len,
            ) {
                return c::io_error_lock_violation;
            }
        }
        for i in 0..64 {
            let entry = ptr::addr_of_mut!((*state()).ranges[i]);
            if (*entry).used {
                continue;
            }
            entry.write(Range {
                used: true,
                owner_id: (*req).owner_id,
                owner_generation: (*req).owner_generation,
                offset: (*req).offset,
                length: (*req).data_len,
                path_len: (*req).path_len,
                ..Range::EMPTY
            });
            ptr::copy_nonoverlapping(
                (*req).path.as_ptr(),
                (*entry).path.as_mut_ptr(),
                (*req).path_len,
            );
            (*entry).path[(*req).path_len] = 0;
            return 0;
        }
        NO_SLOTS
    }
}
pub(super) unsafe fn release_ranges(handle: Handle) -> bool {
    unsafe {
        let Some(_guard) = Locked::acquire(RANGE_LOCK.0.get()) else {
            return false;
        };
        for i in 0..64 {
            let entry = ptr::addr_of_mut!((*state()).ranges[i]);
            if (*entry).used
                && (*entry).owner_id == handle.instance_id
                && (*entry).owner_generation == handle.generation
            {
                entry.write(Range::EMPTY);
            }
        }
        true
    }
}
unsafe fn file_info(path: *const u8) -> i32 {
    unsafe {
        let mut info = c::FileInfo::default();
        let result = program_async_file_info(path, &mut info);
        if result == 0 {
            return NOT_FOUND;
        }
        if result < 0 {
            return match result {
                -1 => INVALID,
                -2 => NOT_FOUND,
                _ => BUSY,
            };
        }
        if info.is_dir != 0 {
            return INVALID;
        }
        if info.size > i32::MAX as u64 {
            TOO_LARGE
        } else {
            info.size as i32
        }
    }
}
fn normalize_write(result: i32) -> i32 {
    if result >= 0 {
        result
    } else {
        match result {
            -1 => INVALID,
            -2 | -3 => NOT_FOUND,
            -7 => BUSY,
            -8 => TOO_LARGE,
            _ => BUSY,
        }
    }
}
unsafe fn perform(req: *mut Request) -> i32 {
    unsafe {
        if (*req).kind != Kind::Lock
            && (!super::system::io::transfer_len_fits((*req).data_len)
                || !super::system::io::transfer_len_fits((*req).out_len))
        {
            return TOO_LARGE;
        }
        let mut empty = 0u8;
        let path = (*req).path.as_ptr();
        let data = if (*req).data == 0 {
            ptr::addr_of!(empty)
        } else {
            ptr::with_exposed_provenance((*req).data)
        };
        let out = if (*req).out == 0 {
            ptr::addr_of_mut!(empty)
        } else {
            ptr::with_exposed_provenance_mut((*req).out)
        };
        match (*req).kind {
            Kind::None => INVALID,
            Kind::Info => file_info(path),
            Kind::Lock => file_lock(req),
            Kind::Service => unreachable!("service continuation has a bounded native owner"),
            kind => {
                // Admission happens outside async/FS locks. Pageable client
                // buffers remain resident through the actual backend return,
                // before publishing completion or allowing Close/Cancel reuse.
                let read = matches!(kind, Kind::Read | Kind::ReadAt);
                let write = matches!(
                    kind,
                    Kind::Write | Kind::Append | Kind::WriteAt | Kind::StreamWrite
                );
                let _buffer = if read || write {
                    let (address, bytes) = if read {
                        ((*req).out, (*req).out_len)
                    } else {
                        ((*req).data, (*req).data_len)
                    };
                    match crate::memory::vm_pager::file_buffer(
                        address as u64,
                        bytes,
                        read,
                        u64::from((*req).owner_id),
                        (*(*req).owner_instance).memory_limits.resident_limit,
                    ) {
                        Ok(lease) => Some(lease),
                        Err(_) => return BUSY,
                    }
                } else {
                    None
                };
                let result = program_async_file(
                    kind as u32,
                    path,
                    (*req).offset,
                    data,
                    (*req).data_len as u32,
                    out,
                    (*req).out_len as u32,
                    (*req).flags,
                );
                if kind == Kind::WriteAt {
                    normalize_write(result)
                } else {
                    result
                }
            }
        }
    }
}
unsafe fn publish_service(req: *mut Request, id: u32) -> bool {
    unsafe {
        let Some(_guard) = lock() else {
            return false;
        };
        let task = scheduler::current();
        if task.is_null() {
            return false;
        }
        let core = task_registry::core(task.cast());
        if !(*req).used
            || (*req).task_id != (*core).id
            || (*req).task_generation != (*core).generation
            || (*req).cancel_requested
        {
            return false;
        }
        (*req).service_request_id = id;
        true
    }
}
unsafe fn clear_service(req: *mut Request, id: u32) {
    unsafe {
        let Some(_guard) = lock() else {
            return;
        };
        let task = scheduler::current();
        if task.is_null() {
            return;
        }
        let core = task_registry::core(task.cast());
        if !(*req).used
            || (*req).task_id != (*core).id
            || (*req).task_generation != (*core).generation
            || (*req).service_request_id != id
        {
            return;
        }
        (*req).service_request_id = 0;
    }
}
/// Endpoint notifications never acquire the Async mutex. Coalesced resident
/// receiver enrollment, bucket epochs and AUTO Events bridge the final check,
/// physical wait and continuation release. No Task/Request pointer is published.
pub(crate) fn service_changed(handle: u32) {
    service::changed(handle);
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StreamOwner {
    pub task: u32,
    pub program: u32,
    pub task_generation: u64,
    pub program_generation: u64,
}
pub(crate) unsafe fn stream_owner() -> Option<StreamOwner> {
    unsafe {
        let thread = threads::current();
        let owner = if !thread.is_null() {
            StreamOwner {
                task: (*thread).task_id,
                program: (*thread).instance_id,
                task_generation: (*thread).task_generation,
                program_generation: (*thread).instance_generation,
            }
        } else {
            let req = current();
            if req.is_null() {
                return None;
            }
            StreamOwner {
                task: (*req).caller_id,
                program: (*req).owner_id,
                task_generation: (*req).caller_generation,
                program_generation: (*req).owner_generation,
            }
        };
        if owner.task == 0
            || owner.program == 0
            || owner.task_generation == 0
            || owner.program_generation == 0
        {
            None
        } else {
            Some(owner)
        }
    }
}
/// # Safety
/// Exclusive resident copied value; the precise caller namespace is not lent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_async_stream_owner(out: *mut StreamOwner) -> bool {
    unsafe {
        let Some(owner) = stream_owner() else {
            return false;
        };
        out.write(owner);
        true
    }
}
const _: () = {
    assert!(mem::size_of::<RequestView>() == 24);
    assert!(mem::offset_of!(RequestView, instance) == 0);
    assert!(mem::offset_of!(RequestView, path) == 8);
    assert!(mem::size_of::<StreamOwner>() == 24);
    assert!(mem::offset_of!(StreamOwner, task) == 0);
    assert!(mem::offset_of!(StreamOwner, program) == 4);
    assert!(mem::offset_of!(StreamOwner, task_generation) == 8);
    assert!(mem::offset_of!(StreamOwner, program_generation) == 16);
};

unsafe fn program_async_file_info(path: *const u8, out: *mut c::FileInfo) -> i32 {
    unsafe { crate::program::system::directory::r4os_rust_sys_file_info(path.cast(), out.cast()) }
}

unsafe fn program_async_service_control(wait: bool, handle: u32, id: u32, timeout: u64) -> i32 {
    unsafe {
        crate::service::ffi::r4os_rust_services_endpoint_control(
            if wait { 4 } else { 5 },
            handle,
            id,
            if wait { timeout } else { 0 },
        )
    }
}

unsafe fn program_async_service_submit(
    handle: u32,
    client: u32,
    op: u16,
    data: *const u8,
    len: u32,
    deadline: Option<u64>,
    token: *mut task_context::Unwind,
) -> i32 {
    unsafe {
        crate::service::endpoint::submit_async(
            handle,
            client,
            op,
            Bytes {
                data: if len == 0 { b"".as_ptr() } else { data },
                len: len as usize,
            },
            deadline,
            token,
        )
    }
}

unsafe fn program_async_service_take(
    handle: u32,
    id: u32,
    header: *mut Header,
    data: *mut u8,
    len: u32,
    deadline: Option<u64>,
) -> i32 {
    unsafe {
        crate::service::endpoint::take_async_response(
            handle,
            id,
            header,
            data,
            len as usize,
            deadline,
        )
    }
}

unsafe fn program_async_option(retry: bool, out: *mut Bytes) -> bool {
    unsafe {
        let Some(value) = crate::boot::config::option_value(
            crate::boot::config::get(),
            b"TASKREGISTRY",
            if retry {
                b"async-retire-retry"
            } else {
                b"selftest"
            },
        ) else {
            return false;
        };
        out.write(Bytes {
            data: value.as_ptr(),
            len: value.len(),
        });
        true
    }
}

// Preserve the existing async file operation projection and its exact buffers.
#[allow(clippy::too_many_arguments)]
unsafe fn program_async_file(
    kind: u32,
    path: *const u8,
    offset: u64,
    data: *const u8,
    len: u32,
    out: *mut u8,
    out_len: u32,
    flags: u32,
) -> i32 {
    unsafe {
        use crate::program::system::{io, stream_api as stream};
        match kind {
            1 => io::r4os_rust_sys_file_read(path, out, out_len),
            2 => io::r4os_rust_sys_file_read_at(path, offset, out, out_len),
            3 => io::r4os_rust_sys_file_write(path, data, len),
            4 => io::r4os_rust_sys_file_append(path, data, len),
            5 => stream::r4os_rust_sys_stream_begin(path, flags),
            6 => stream::r4os_rust_sys_stream_write(path, offset, data, len, flags),
            7 => stream::r4os_rust_sys_stream_finish(path, offset, flags),
            8 => stream::r4os_rust_sys_stream_abort(path),
            10 => io::r4os_rust_sys_file_write_at(path, offset, data, len),
            _ => panic!("Invalid async filesystem operation"),
        }
    }
}
