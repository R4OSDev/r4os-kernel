//! Resident byte-channel mechanism. SDK owns transfer/launch policy. Existing
//! IPC message channels remain separate: their close is no reference/EOF ACK.
//! Runtime owns exact end references, bounded rings and native wait admission.
//! Queues retain no caller pointers; borrowed spans last through actual return.
//! Heap free and unwind pins never span a scheduler park.
#[path = "byte_pipes/policy.rs"]
mod policy;
use super::{context, deadline_policy, notifications, waitsets};
use crate::{
    arch::runtime,
    contract as c,
    driver::treap::{Node, Tree},
    memory::heap,
    platform::monotonic,
    sched::{
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait,
    },
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, align_of, size_of},
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
use deadline_policy::{Budget, ClockKey, Sample};
const ENDS: usize = c::byte_pipe_max_references as usize;
const STEP: usize = c::byte_pipe_max_step as usize;
// Bounds both Runtime wake traversals. Actual native queues own admission;
// there is no separate waiter count which could survive kill or timeout.
const MAX_WAITERS: usize = c::byte_pipe_max_waiters as usize;
#[repr(C)]
struct End {
    node: Node,
    pipe: *mut Pipe,
    handle: u64,
    owner: c::ProgramProcessHandle,
    epoch: u64,
    direction: u32,
    active: bool,
}
#[repr(C)]
struct Pipe {
    previous: *mut Pipe,
    next: *mut Pipe,
    generation: u64,
    capacity: usize,
    ring: policy::Ring,
    readers: WaitQueue,
    writers: WaitQueue,
    ends: [End; ENDS],
}
struct State {
    tree: Tree,
    first: *mut Pipe,
    serial: u64,
    pipes: u64,
    payload: u64,
}
struct Shared(UnsafeCell<State>);
// Every metadata/payload access holds the existing scheduler Runtime owner.
unsafe impl Sync for Shared {}
static STATE: Shared = Shared(UnsafeCell::new(State {
    tree: Tree {
        root: ptr::null_mut(),
        xorshift: 0,
    },
    first: ptr::null_mut(),
    serial: 0,
    pipes: 0,
    payload: 0,
}));
// Conservative empty fast path for ordinary process retirement. Reservation,
// OOM rollback and real final free update it under the canonical Runtime.
// It never admits resources or acknowledges a free. Cleanup's process threads
// have already physically retired, so that process cannot publish a late end.
static PRESENT: AtomicBool = AtomicBool::new(false);
unsafe fn release_budget(capacity: u64) {
    unsafe {
        let state = &mut *STATE.0.get();
        assert!(state.pipes != 0 && state.payload >= capacity);
        state.pipes -= 1;
        state.payload -= capacity;
        PRESENT.store(state.pipes != 0, Ordering::Release);
    }
}
fn same(a: c::ProgramProcessHandle, b: c::ProgramProcessHandle) -> bool {
    a.instance_id == b.instance_id && a.generation == b.generation
}
fn owner() -> Result<c::ProgramProcessHandle, i32> {
    if !notifications::can_sleep() {
        return Err(c::byte_pipe_error_context);
    }
    unsafe { context::handle().ok_or(c::byte_pipe_error_context) }
}
fn syntax(end: c::BytePipeEnd) -> Result<(), i32> {
    if end.handle == 0
        || end.generation == 0
        || end.reserved != 0
        || !matches!(end.direction, c::byte_pipe_reader | c::byte_pipe_writer)
    {
        Err(c::byte_pipe_error_invalid)
    } else {
        Ok(())
    }
}
unsafe fn find(end: c::BytePipeEnd, who: c::ProgramProcessHandle) -> Result<*mut End, i32> {
    syntax(end)?;
    unsafe {
        let r = (*STATE.0.get()).tree.find(end.handle).cast::<End>();
        if r.is_null() || (*(*r).pipe).generation != end.generation {
            return Err(c::byte_pipe_error_stale);
        }
        if !same((*r).owner, who) {
            return Err(c::byte_pipe_error_owner);
        }
        if (*r).direction != end.direction {
            return Err(c::byte_pipe_error_direction);
        }
        Ok(r)
    }
}
unsafe fn identity(r: *mut End) -> c::BytePipeEnd {
    unsafe {
        c::BytePipeEnd {
            handle: (*r).handle,
            generation: (*(*r).pipe).generation,
            direction: (*r).direction,
            reserved: 0,
        }
    }
}
unsafe fn link(p: *mut Pipe) {
    unsafe {
        (*p).previous = ptr::null_mut();
        (*p).next = (*STATE.0.get()).first;
        if !(*p).next.is_null() {
            (*(*p).next).previous = p;
        }
        (*STATE.0.get()).first = p;
    }
}
unsafe fn unlink(p: *mut Pipe) {
    unsafe {
        if (*p).previous.is_null() {
            (*STATE.0.get()).first = (*p).next;
        } else {
            (*(*p).previous).next = (*p).next;
        }
        if !(*p).next.is_null() {
            (*(*p).next).previous = (*p).previous;
        }
    }
}
unsafe fn add_end(
    p: *mut Pipe,
    slot: usize,
    handle: u64,
    direction: u32,
    who: c::ProgramProcessHandle,
) -> c::BytePipeEnd {
    unsafe {
        let r = ptr::addr_of_mut!((*p).ends[slot]);
        (*r).pipe = p;
        (*r).handle = handle;
        (*r).direction = direction;
        (*r).owner = who;
        (*r).active = true;
        (*r).epoch = 0;
        (*STATE.0.get())
            .tree
            .insert(handle, ptr::addr_of_mut!((*r).node));
        identity(r)
    }
}
unsafe fn wake(p: *mut Pipe, readers: bool, writers: bool) {
    unsafe {
        if readers {
            sync_wait::r4os_rust_sync_queue(
                ptr::addr_of_mut!((*p).readers),
                1,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
            );
        }
        if writers {
            sync_wait::r4os_rust_sync_queue(
                ptr::addr_of_mut!((*p).writers),
                1,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
            );
        }
    }
}
pub(super) unsafe extern "C" fn create(capacity: u64, output: *mut c::BytePipePair) -> i32 {
    if capacity == 0 || capacity > c::byte_pipe_max_capacity || !waitsets::valid(output, 1) {
        return c::byte_pipe_error_invalid;
    }
    let who = match owner() {
        Ok(x) => x,
        Err(e) => return e,
    };
    let Some(_operation) = notifications::Operation::enter() else {
        return c::byte_pipe_error_context;
    };
    let generation = unsafe {
        let _guard = runtime::RuntimeGuard::acquire();
        let state = &mut *STATE.0.get();
        if state.pipes >= c::byte_pipe_max_pipes
            || state.payload > c::byte_pipe_max_payload - capacity
        {
            return c::byte_pipe_error_capacity;
        }
        if state.serial > u64::MAX - 3 {
            return c::byte_pipe_error_exhausted;
        }
        state.serial += 3;
        state.pipes += 1;
        state.payload += capacity;
        PRESENT.store(true, Ordering::Release);
        state.serial - 2
    };
    let bytes = size_of::<Pipe>() + capacity as usize;
    let memory = heap::r4os_rust_heap_alloc(bytes, align_of::<Pipe>());
    if memory.is_null() {
        let _guard = runtime::RuntimeGuard::acquire();
        unsafe { release_budget(capacity) };
        return c::byte_pipe_error_memory;
    }
    unsafe {
        // In-place initialization: no multi-KB Pipe value on a caller stack.
        ptr::write_bytes(memory, 0, size_of::<Pipe>());
        let p = memory.cast::<Pipe>();
        (*p).generation = generation;
        (*p).capacity = capacity as usize;
        (*p).ring = policy::Ring::new();
        (*p).readers = WaitQueue::EMPTY;
        (*p).writers = WaitQueue::EMPTY;
        let result = {
            let _guard = runtime::RuntimeGuard::acquire();
            link(p);
            c::BytePipePair {
                reader: add_end(p, 0, generation + 1, c::byte_pipe_reader, who),
                writer: add_end(p, 1, generation + 2, c::byte_pipe_writer, who),
                ..Default::default()
            }
        };
        output.write(result);
    }
    0
}
/// All three selected references are preflighted under the one canonical
/// Runtime owner. The private loader owns a fresh unpublished full child
/// identity. No allocation, caller copy or fallible step occurs after commit.
/// This is the sole cross-process retain path; raw End copies confer none.
pub(super) unsafe fn bind_launch(
    parent: c::ProgramProcessHandle,
    child: c::ProgramProcessHandle,
    ends: &mut [c::BytePipeEnd; 3],
    selected: [bool; 3],
) -> Result<(), i32> {
    if parent.instance_id == 0
        || parent.generation == 0
        || parent.reserved != 0
        || child.instance_id == 0
        || child.generation == 0
        || child.reserved != 0
        || same(parent, child)
    {
        return Err(c::byte_pipe_error_invalid);
    }
    let _guard = runtime::RuntimeGuard::acquire();
    unsafe {
        let mut pipes = [ptr::null_mut::<Pipe>(); 3];
        let mut slots = [0usize; 3];
        let mut count = 0u64;
        for i in 0..3 {
            if !selected[i] {
                continue;
            }
            let r = find(ends[i], parent)?;
            if !(*r).active {
                return Err(c::byte_pipe_error_stale);
            }
            let direction = if i == 0 {
                c::byte_pipe_reader
            } else {
                c::byte_pipe_writer
            };
            if ends[i].direction != direction {
                return Err(c::byte_pipe_error_direction);
            }
            let p = (*r).pipe;
            let Some(slot) = (0..ENDS).find(|&s| {
                (*p).ends[s].handle == 0
                    && !(0..i).any(|j| selected[j] && pipes[j] == p && slots[j] == s)
            }) else {
                return Err(c::byte_pipe_error_capacity);
            };
            pipes[i] = p;
            slots[i] = slot;
            count += 1;
        }
        if count > u64::MAX - (*STATE.0.get()).serial {
            return Err(c::byte_pipe_error_exhausted);
        }
        for i in 0..3 {
            if !selected[i] {
                continue;
            }
            let p = pipes[i];
            (*STATE.0.get()).serial += 1;
            if ends[i].direction == c::byte_pipe_reader {
                (*p).ring.readers += 1;
            } else {
                (*p).ring.writers += 1;
            }
            ends[i] = add_end(
                p,
                slots[i],
                (*STATE.0.get()).serial,
                ends[i].direction,
                child,
            );
        }
    }
    Ok(())
}
pub(super) unsafe extern "C" fn duplicate(end: c::BytePipeEnd, output: *mut c::BytePipeEnd) -> i32 {
    if !waitsets::valid(output, 1) {
        return c::byte_pipe_error_invalid;
    }
    let who = match owner() {
        Ok(x) => x,
        Err(e) => return e,
    };
    let Some(_operation) = notifications::Operation::enter() else {
        return c::byte_pipe_error_context;
    };
    let result = unsafe {
        let _guard = runtime::RuntimeGuard::acquire();
        let r = match find(end, who) {
            Ok(x) => x,
            Err(e) => return e,
        };
        if !(*r).active {
            return c::byte_pipe_error_stale;
        }
        let p = (*r).pipe;
        let slot = (0..ENDS).find(|&i| (*p).ends[i].handle == 0);
        let Some(slot) = slot else {
            return c::byte_pipe_error_capacity;
        };
        if (*STATE.0.get()).serial == u64::MAX {
            return c::byte_pipe_error_exhausted;
        }
        (*STATE.0.get()).serial += 1;
        if end.direction == c::byte_pipe_reader {
            (*p).ring.readers += 1;
        } else {
            (*p).ring.writers += 1;
        }
        add_end(p, slot, (*STATE.0.get()).serial, end.direction, who)
    };
    unsafe {
        output.write(result);
    }
    0
}
unsafe fn remove(end: c::BytePipeEnd, who: c::ProgramProcessHandle) -> i32 {
    unsafe {
        let (p, r, capacity) = {
            let _guard = runtime::RuntimeGuard::acquire();
            let r = match find(end, who) {
                Ok(x) => x,
                Err(e) => return e,
            };
            let p = (*r).pipe;
            if (*r).active {
                (*r).active = false;
                (*p).ring.close(end.direction == c::byte_pipe_reader);
            }
            wake(p, true, true);
            (*STATE.0.get()).tree.remove(ptr::addr_of_mut!((*r).node));
            (*r).handle = 0;
            if (*p).ring.readers != 0 || (*p).ring.writers != 0 {
                return 0;
            }
            // No end or waiting Task can find this memory while free is in
            // progress. A resumed waiter resolves its non-reused handle anew.
            unlink(p);
            (p, r, (*p).capacity)
        };
        if heap::r4os_rust_heap_free(p.cast(), size_of::<Pipe>() + capacity) == 0 {
            let _guard = runtime::RuntimeGuard::acquire();
            release_budget(capacity as u64);
            return 0;
        }
        let _guard = runtime::RuntimeGuard::acquire();
        (*r).handle = end.handle; // closed exact retry record, not live writer
        (*STATE.0.get())
            .tree
            .insert(end.handle, ptr::addr_of_mut!((*r).node));
        link(p);
        c::byte_pipe_error_release
    }
}
pub(super) unsafe extern "C" fn close(end: c::BytePipeEnd) -> i32 {
    let who = match owner() {
        Ok(x) => x,
        Err(e) => return e,
    };
    let Some(_operation) = notifications::Operation::enter() else {
        return c::byte_pipe_error_context;
    };
    unsafe { remove(end, who) }
}
pub(super) unsafe extern "C" fn cancel(end: c::BytePipeEnd) -> i32 {
    let who = match owner() {
        Ok(x) => x,
        Err(e) => return e,
    };
    let _guard = runtime::RuntimeGuard::acquire();
    unsafe {
        let r = match find(end, who) {
            Ok(x) => x,
            Err(e) => return e,
        };
        if !(*r).active {
            return c::byte_pipe_error_stale;
        }
        if (*r).epoch == u64::MAX {
            return c::byte_pipe_error_exhausted;
        }
        (*r).epoch += 1;
        wake((*r).pipe, true, true);
    }
    0
}
pub(super) unsafe extern "C" fn info(end: c::BytePipeEnd, output: *mut c::BytePipeInfo) -> i32 {
    if !waitsets::valid(output, 1) {
        return c::byte_pipe_error_invalid;
    }
    let who = match owner() {
        Ok(x) => x,
        Err(e) => return e,
    };
    let Some(_operation) = notifications::Operation::enter() else {
        return c::byte_pipe_error_context;
    };
    let value = unsafe {
        let _guard = runtime::RuntimeGuard::acquire();
        let r = match find(end, who) {
            Ok(x) => x,
            Err(e) => return e,
        };
        if !(*r).active {
            return c::byte_pipe_error_stale;
        }
        let p = (*r).pipe;
        c::BytePipeInfo {
            capacity: (*p).capacity as u64,
            queued: (*p).ring.len as u64,
            readers: (*p).ring.readers,
            writers: (*p).ring.writers,
            atomic_limit: (*p).capacity.min(STEP) as u64,
            max_step: STEP as u64,
            ..Default::default()
        }
    };
    unsafe {
        output.write(value);
    }
    0
}
fn sample() -> Option<Sample> {
    let v = monotonic::snapshot();
    let key = ClockKey {
        source: v.source,
        generation: v.generation,
        event_backend: v.event_backend,
        tick_hz: v.event.requested_hz,
    };
    (v.flags & c::monotonic_clock_flag_valid != 0 && key.valid()).then_some(Sample {
        key,
        instant_ns: v.instant_ns,
    })
}
fn clock_error(error: deadline_policy::Error) -> i32 {
    match error {
        deadline_policy::Error::ClockUnavailable => c::byte_pipe_error_clock_unavailable,
        deadline_policy::Error::ClockChanged | deadline_policy::Error::ClockRegressed => {
            c::byte_pipe_error_clock_changed
        }
        _ => c::byte_pipe_error_invalid,
    }
}
unsafe fn io(
    end: c::BytePipeEnd,
    data: *mut u8,
    length: u64,
    deadline: *const c::WaitSetDeadline,
    output: *mut c::BytePipeProgress,
    writing: bool,
) -> i32 {
    let Ok(length) = usize::try_from(length) else {
        return c::byte_pipe_error_invalid;
    };
    if length == 0
        || !waitsets::valid(data, length)
        || !waitsets::valid(deadline, 1)
        || !waitsets::valid(output, 1)
    {
        return c::byte_pipe_error_invalid;
    }
    if waitsets::overlaps(
        data.addr(),
        length,
        deadline.addr(),
        size_of::<c::WaitSetDeadline>(),
    ) || waitsets::overlaps(
        data.addr(),
        length,
        output.addr(),
        size_of::<c::BytePipeProgress>(),
    ) || waitsets::overlaps(
        deadline.addr(),
        size_of::<c::WaitSetDeadline>(),
        output.addr(),
        size_of::<c::BytePipeProgress>(),
    ) {
        return c::byte_pipe_error_invalid;
    }
    let who = match owner() {
        Ok(x) => x,
        Err(e) => return e,
    };
    if let Err(error) = syntax(end) {
        return error;
    }
    if end.direction
        != if writing {
            c::byte_pipe_writer
        } else {
            c::byte_pipe_reader
        }
    {
        return c::byte_pipe_error_direction;
    }
    // Only copied/accepted prefixes are ever read. Avoid clearing a 4KB
    // staging buffer for a one-byte operation or an empty cancelled wait.
    let mut storage = MaybeUninit::<[u8; STEP]>::uninit();
    let staging = storage.as_mut_ptr().cast::<u8>();
    let (mut budget, epoch) = {
        let Some(_operation) = notifications::Operation::enter() else {
            return c::byte_pipe_error_context;
        };
        let d = unsafe { deadline.read() };
        if d.version != 1 || d.size as usize != size_of::<c::WaitSetDeadline>() || d.reserved0 != 0
        {
            return c::byte_pipe_error_invalid;
        }
        let b = Budget {
            kind: d.kind,
            absolute_ns: d.absolute_ns,
            last_ns: d.last_ns,
            key: ClockKey {
                source: d.clock_source,
                generation: d.clock_generation,
                event_backend: d.event_backend,
                tick_hz: d.tick_hz,
            },
        };
        if let Err(e) = b.validate() {
            return clock_error(e);
        }
        let epoch = unsafe {
            let _guard = runtime::RuntimeGuard::acquire();
            let r = match find(end, who) {
                Ok(x) => x,
                Err(e) => return e,
            };
            if !(*r).active {
                return c::byte_pipe_error_stale;
            }
            (*r).epoch
        };
        if writing {
            unsafe {
                ptr::copy_nonoverlapping(data, staging, (length).min(STEP));
            }
        }
        (b, epoch)
    }; // No unwind guard survives a later park.
    loop {
        let Some(operation) = notifications::Operation::enter() else {
            return c::byte_pipe_error_context;
        };
        let flags = runtime::save_runtime();
        let r = unsafe { find(end, who) };
        let mut result = c::BytePipeProgress::default();
        let r = match r {
            Ok(r) => r,
            Err(c::byte_pipe_error_stale) => {
                runtime::restore_runtime(flags);
                result.state = c::byte_pipe_closed;
                unsafe {
                    output.write(result);
                }
                return 0;
            }
            Err(e) => {
                runtime::restore_runtime(flags);
                return e;
            }
        };
        unsafe {
            let p = (*r).pipe;
            let remaining = match budget.remaining(sample()) {
                Ok(v) => v,
                Err(e) => {
                    runtime::restore_runtime(flags);
                    return clock_error(e);
                }
            };
            if !(*r).active {
                result.state = c::byte_pipe_closed;
            } else if (*r).epoch != epoch {
                result.state = c::byte_pipe_cancelled;
            } else if budget.kind == 1 && remaining == Some(0) {
                result.state = c::byte_pipe_timed_out;
            } else if writing && (*p).ring.readers == 0 {
                result.state = c::byte_pipe_broken_pipe;
            } else if !writing && (*p).ring.eof() {
                result.state = c::byte_pipe_eof;
            } else {
                let count = if writing {
                    (*p).ring.write_count((*p).capacity, length, STEP)
                } else {
                    (*p).ring.read_count(length, STEP)
                };
                if count != 0 {
                    let head = if writing {
                        ((*p).ring.head + (*p).ring.len) % (*p).capacity
                    } else {
                        (*p).ring.head
                    };
                    let first = count.min((*p).capacity - head);
                    let ring = p.add(1).cast::<u8>();
                    if writing {
                        ptr::copy_nonoverlapping(staging.cast_const(), ring.add(head), first);
                        ptr::copy_nonoverlapping(
                            staging.cast_const().add(first),
                            ring,
                            count - first,
                        );
                        (*p).ring.written((*p).capacity, count);
                        wake(p, true, false);
                    } else {
                        ptr::copy_nonoverlapping(ring.add(head), staging, first);
                        ptr::copy_nonoverlapping(ring, staging.add(first), count - first);
                        (*p).ring.read((*p).capacity, count);
                        wake(p, false, true);
                    }
                    result.bytes = count as u64;
                    result.state = c::byte_pipe_ready;
                } else if budget.kind == 0 {
                    result.state = c::byte_pipe_would_block;
                }
            }
            if result.state != 0 {
                runtime::restore_runtime(flags);
                if !writing && result.bytes != 0 {
                    ptr::copy_nonoverlapping(staging.cast_const(), data, result.bytes as usize);
                }
                output.write(result); // short unwind guard spans actual copy return
                return 0;
            }
            let ticks = match remaining {
                None => u64::MAX,
                Some(ns) => match deadline_policy::ticks(ns, budget.key.tick_hz) {
                    Ok(v) => v,
                    Err(e) => {
                        runtime::restore_runtime(flags);
                        return clock_error(e);
                    }
                },
            };
            let queue = if writing {
                ptr::addr_of_mut!((*p).writers)
            } else {
                ptr::addr_of_mut!((*p).readers)
            };
            if (*p).readers.core.count + (*p).writers.core.count >= MAX_WAITERS {
                runtime::restore_runtime(flags);
                return c::byte_pipe_error_capacity;
            }
            let request = WaitRequest {
                timeout: ticks,
                reason: Text::literal(b"ipc-byte-pipe"),
                predicate: None,
                context: ptr::null_mut(),
                release: None,
                release_context: ptr::null_mut(),
                flags,
                consume_runtime: true,
            };
            // Predicate check and queue enrollment share the same Runtime.
            // Drop only the short copy guard before consuming it into park.
            drop(operation);
            let outcome = sync_wait::r4os_rust_sync_wait(queue, &request);
            // Never dereference p/r/queue after this call: Close may free them.
            if matches!(outcome, 0 | 5) {
                return c::byte_pipe_error_context;
            }
        }
    }
}
pub(super) unsafe extern "C" fn read(
    end: c::BytePipeEnd,
    data: *mut u8,
    length: u64,
    deadline: *const c::WaitSetDeadline,
    output: *mut c::BytePipeProgress,
) -> i32 {
    unsafe { io(end, data, length, deadline, output, false) }
}
pub(super) unsafe extern "C" fn write(
    end: c::BytePipeEnd,
    data: *const u8,
    length: u64,
    deadline: *const c::WaitSetDeadline,
    output: *mut c::BytePipeProgress,
) -> i32 {
    unsafe { io(end, data.cast_mut(), length, deadline, output, true) }
}
/// Exact process threads have physically retired. Other processes' references
/// survive; no creator heap or pointer owns the resident shared backing.
pub(super) unsafe fn cleanup(who: c::ProgramProcessHandle) -> bool {
    if !PRESENT.load(Ordering::Acquire) {
        return true;
    }
    let Some(_operation) = notifications::Operation::enter() else {
        return false;
    };
    loop {
        let end = unsafe {
            let _guard = runtime::RuntimeGuard::acquire();
            let mut p = (*STATE.0.get()).first;
            let mut found = None;
            while !p.is_null() && found.is_none() {
                for i in 0..ENDS {
                    let r = ptr::addr_of_mut!((*p).ends[i]);
                    if (*r).handle != 0 && same((*r).owner, who) {
                        found = Some(identity(r));
                        break;
                    }
                }
                p = (*p).next;
            }
            found
        };
        let Some(end) = end else {
            return true;
        };
        if unsafe { remove(end, who) } != 0 {
            return false;
        }
    }
}

const _: c::R4SysFns::byte_pipe_create = create;
const _: c::R4SysFns::byte_pipe_dup = duplicate;
const _: c::R4SysFns::byte_pipe_read = read;
const _: c::R4SysFns::byte_pipe_write = write;
const _: c::R4SysFns::byte_pipe_cancel = cancel;
const _: c::R4SysFns::byte_pipe_close = close;
const _: c::R4SysFns::byte_pipe_info = info;
