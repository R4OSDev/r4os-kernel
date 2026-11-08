//! Process-owned resident wait-any sets. Source publication, subscription,
//! predicate observation and the original Task queue enrollment share Runtime.
//! No foreign pointer, callback or unwind pin survives a scheduler park.
#[path = "waitsets/policy.rs"]
mod policy;
use super::{context, notifications, payload_types};
use crate::{
    arch::runtime,
    contract as c,
    driver::treap::{Node, Tree},
    memory::heap,
    platform::monotonic,
    sched::{
        scheduler,
        sync_types::{Text, WaitQueue, WaitRequest},
        sync_wait,
        task::task_registry,
    },
};
use core::{
    cell::UnsafeCell,
    mem::{align_of, size_of},
    ptr,
};
use policy::{Budget, ClockKey, Sample};
const MAX: usize = 32;
#[repr(C)]
pub struct Owner {
    tree: Tree,
    closing: bool,
}
struct Serial(UnsafeCell<u64>);
// All reservations and namespace/source mutations retain the existing Runtime.
unsafe impl Sync for Serial {}
static SERIAL: Serial = Serial(UnsafeCell::new(0));

/// Intrusive kernel-owned link, never a caller's descriptor or task wait node.
/// Only Notifications mutates the source links, under the same Runtime owner.
#[repr(C)]
pub(super) struct Subscription {
    pub(super) previous: *mut Subscription,
    pub(super) next: *mut Subscription,
    pub(super) source_handle: u64,
    pub(super) linked: bool,
    parent: *mut Record,
    sequence: u64,
    closed: bool,
}
#[repr(C)]
struct Registration {
    link: Subscription,
    id: u64,
    spec: c::WaitSource,
    last_ns: u64,
    clock_invalid: bool,
}
#[repr(C)]
struct Record {
    node: Node,
    handle: u64,
    creator_id: u32,
    creator_generation: u64,
    notifications: *mut notifications::Owner,
    capacity: usize,
    count: usize,
    cursor: usize,
    next_registration: u64,
    revision: u64,
    closed: bool,
    waiting: bool,
    queue: WaitQueue,
    registrations: [Registration; MAX],
}
fn valid<T>(pointer: *const T, count: usize) -> bool {
    let address = pointer.addr();
    let Some(bytes) = size_of::<T>().checked_mul(count) else {
        return false;
    };
    address != 0
        && bytes != 0
        && address.is_multiple_of(align_of::<T>())
        && address.checked_add(bytes).is_some()
}
fn overlaps(a: usize, n: usize, b: usize, m: usize) -> bool {
    a < b + m && b < a + n
}
fn identity() -> Option<(u32, u64)> {
    let task = scheduler::current();
    if task.is_null() {
        return None;
    }
    let core = unsafe { task_registry::core(task.cast()) };
    Some(unsafe { ((*core).id, (*core).generation) })
}
fn owners() -> Option<(*mut Owner, *mut notifications::Owner)> {
    unsafe {
        let instance = context::current();
        if instance.is_null() {
            return None;
        }
        let payload = (*instance).runtime_payload.cast::<payload_types::Runtime>();
        if payload.is_null() {
            return None;
        }
        Some((
            ptr::addr_of_mut!((*payload).wait_sets),
            ptr::addr_of_mut!((*instance).notifications),
        ))
    }
}
unsafe fn find(owner: *mut Owner, handle: u64) -> *mut Record {
    unsafe { (*owner).tree.find(handle).cast() }
}
fn key(source: &c::WaitSource) -> ClockKey {
    ClockKey {
        source: source.clock_source,
        generation: source.clock_generation,
        event_backend: source.event_backend,
        tick_hz: source.tick_hz,
    }
}
fn clock() -> Option<Sample> {
    let value = monotonic::snapshot();
    let key = ClockKey {
        source: value.source,
        generation: value.generation,
        event_backend: value.event_backend,
        tick_hz: value.event.requested_hz,
    };
    (value.flags & c::monotonic_clock_flag_valid != 0 && key.valid()).then_some(Sample {
        key,
        instant_ns: value.instant_ns,
    })
}
fn policy_error(error: policy::Error) -> i32 {
    match error {
        policy::Error::Invalid => c::waitset_error_invalid,
        policy::Error::ClockUnavailable => c::waitset_error_clock_unavailable,
        policy::Error::ClockChanged | policy::Error::ClockRegressed => {
            c::waitset_error_clock_changed
        }
        policy::Error::Overflow => c::waitset_error_overflow,
    }
}
fn syntax(source: &c::WaitSource) -> Result<(), i32> {
    if source.version != 1
        || source.size as usize != size_of::<c::WaitSource>()
        || source.reserved0 != 0
        || source.generation != 0
    {
        return Err(c::waitset_error_invalid);
    }
    match source.kind {
        c::waitset_source_notification => {
            if source.object == 0
                || source.flags & !c::waitset_source_flag_stop != 0
                || source.deadline_ns != 0
                || key(source) != ClockKey::default()
            {
                return Err(c::waitset_error_invalid);
            }
        }
        c::waitset_source_timer => {
            if source.object != 0 || source.flags != 0 || source.observed_revision != 0 {
                return Err(c::waitset_error_invalid);
            }
            if !key(source).valid() {
                return Err(c::waitset_error_clock_unavailable);
            }
        }
        _ => return Err(c::waitset_error_unsupported),
    }
    Ok(())
}
unsafe fn creator(record: *mut Record) -> bool {
    identity().is_some_and(|(id, generation)| unsafe {
        id == (*record).creator_id && generation == (*record).creator_generation
    })
}
unsafe fn registration(record: *mut Record, id: u64) -> *mut Registration {
    unsafe {
        if id != 0 {
            for index in 0..(*record).capacity {
                let slot = ptr::addr_of_mut!((*record).registrations[index]);
                if (*slot).id == id {
                    return slot;
                }
            }
        }
        ptr::null_mut()
    }
}
/// Called only by the original notification owner while it holds Runtime.
/// Sequence publication precedes this bounded wake; no external callback runs.
pub(super) unsafe fn source_changed(link: *mut Subscription, sequence: u64, closed: bool) {
    unsafe {
        (*link).sequence = sequence;
        (*link).closed = closed;
        let record = (*link).parent;
        if !(*record).closed {
            sync_wait::r4os_rust_sync_queue(
                ptr::addr_of_mut!((*record).queue),
                1,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
            );
        }
    }
}
pub(super) unsafe fn create(capacity: u32, output: *mut u64) -> i32 {
    if capacity == 0 || capacity > MAX as u32 || !valid(output.cast_const(), 1) {
        return c::waitset_error_invalid;
    }
    let Some(_operation) = notifications::Operation::enter() else {
        return c::waitset_error_context;
    };
    let Some((owner, notification_owner)) = owners() else {
        return c::waitset_error_context;
    };
    let Some((creator_id, creator_generation)) = identity() else {
        return c::waitset_error_context;
    };
    unsafe {
        let handle = {
            let _guard = runtime::RuntimeGuard::acquire();
            if (*owner).closing {
                return c::waitset_error_closed;
            }
            if *SERIAL.0.get() == u64::MAX {
                return c::waitset_error_exhausted;
            }
            *SERIAL.0.get() += 1;
            *SERIAL.0.get()
        };
        let memory = heap::r4os_rust_heap_alloc(size_of::<Record>(), align_of::<Record>());
        if memory.is_null() {
            return c::waitset_error_memory;
        }
        // Initialize in place, no whole resident record temporary on a stack.
        memory.write_bytes(0, size_of::<Record>());
        let record = memory.cast::<Record>();
        (*record).handle = handle;
        (*record).creator_id = creator_id;
        (*record).creator_generation = creator_generation;
        (*record).notifications = notification_owner;
        (*record).capacity = capacity as usize;
        (*record).revision = 1;
        ptr::addr_of_mut!((*record).queue).write(WaitQueue::EMPTY);
        for index in 0..MAX {
            (*record).registrations[index].link.parent = record;
        }
        {
            let _guard = runtime::RuntimeGuard::acquire();
            assert!(!(*owner).closing && find(owner, handle).is_null());
            (*owner)
                .tree
                .insert(handle, ptr::addr_of_mut!((*record).node));
        }
        output.write(handle);
        c::waitset_ok
    }
}
pub(super) unsafe fn enroll(handle: u64, source: *const c::WaitSource, output: *mut u64) -> i32 {
    if !valid(source, 1)
        || !valid(output.cast_const(), 1)
        || overlaps(
            source.addr(),
            size_of::<c::WaitSource>(),
            output.addr(),
            size_of::<u64>(),
        )
    {
        return c::waitset_error_invalid;
    }
    let source = unsafe { source.read() };
    if let Err(error) = syntax(&source) {
        return error;
    }
    let Some(_operation) = notifications::Operation::enter() else {
        return c::waitset_error_context;
    };
    let Some((owner, _)) = owners() else {
        return c::waitset_error_context;
    };
    let id = unsafe {
        let _guard = runtime::RuntimeGuard::acquire();
        let record = find(owner, handle);
        if record.is_null() {
            return c::waitset_error_stale;
        }
        if (*owner).closing || (*record).closed {
            return c::waitset_error_closed;
        }
        if !creator(record) {
            return c::waitset_error_owner;
        }
        if (*record).waiting {
            return c::waitset_error_busy;
        }
        if (*record).count == (*record).capacity {
            return c::waitset_error_capacity;
        }
        if (*record).next_registration == u64::MAX || (*record).revision == u64::MAX {
            return c::waitset_error_exhausted;
        }
        let mut selected = ptr::null_mut();
        for index in 0..(*record).capacity {
            let slot = ptr::addr_of_mut!((*record).registrations[index]);
            if (*slot).id == 0 {
                selected = slot;
                break;
            }
        }
        assert!(!selected.is_null());
        let now = if source.kind == c::waitset_source_timer {
            let Some(sample) = clock() else {
                return c::waitset_error_clock_unavailable;
            };
            if sample.key != key(&source) {
                return c::waitset_error_clock_changed;
            }
            sample.instant_ns
        } else {
            match notifications::subscribe(
                (*record).notifications,
                source.object,
                ptr::addr_of_mut!((*selected).link),
                source.observed_revision,
            ) {
                Ok(()) => 0,
                Err(error) => return error,
            }
        };
        (*record).next_registration += 1;
        (*record).revision += 1;
        (*record).count += 1;
        (*selected).id = (*record).next_registration;
        (*selected).spec = source;
        (*selected).last_ns = now;
        (*selected).clock_invalid = false;
        (*selected).id
    };
    unsafe { output.write(id) };
    c::waitset_ok
}
pub(super) unsafe fn rearm(handle: u64, id: u64, source: *const c::WaitSource) -> i32 {
    if !valid(source, 1) {
        return c::waitset_error_invalid;
    }
    let source = unsafe { source.read() };
    if let Err(error) = syntax(&source) {
        return error;
    }
    let Some(_operation) = notifications::Operation::enter() else {
        return c::waitset_error_context;
    };
    let Some((owner, _)) = owners() else {
        return c::waitset_error_context;
    };
    unsafe {
        let _guard = runtime::RuntimeGuard::acquire();
        let record = find(owner, handle);
        if record.is_null() {
            return c::waitset_error_stale;
        }
        if (*owner).closing || (*record).closed {
            return c::waitset_error_closed;
        }
        if !creator(record) {
            return c::waitset_error_owner;
        }
        if (*record).waiting {
            return c::waitset_error_busy;
        }
        if (*record).revision == u64::MAX {
            return c::waitset_error_exhausted;
        }
        let slot = registration(record, id);
        if slot.is_null() {
            return c::waitset_error_stale;
        }
        let old = &(*slot).spec;
        if source.kind != old.kind
            || source.object != old.object
            || source.generation != old.generation
            || source.user_tag != old.user_tag
            || source.flags != old.flags
        {
            return c::waitset_error_invalid;
        }
        let now = if source.kind == c::waitset_source_timer {
            let Some(sample) = clock() else {
                return c::waitset_error_clock_unavailable;
            };
            if sample.key != key(&source) {
                return c::waitset_error_clock_changed;
            }
            sample.instant_ns
        } else {
            if (*slot).link.closed {
                return c::waitset_error_closed;
            }
            if source.observed_revision > (*slot).link.sequence {
                return c::waitset_error_invalid;
            }
            0
        };
        (*slot).spec = source;
        (*slot).last_ns = now;
        (*slot).clock_invalid = false;
        (*record).revision += 1;
        c::waitset_ok
    }
}
pub(super) unsafe fn remove(handle: u64, id: u64) -> i32 {
    let Some(_operation) = notifications::Operation::enter() else {
        return c::waitset_error_context;
    };
    let Some((owner, _)) = owners() else {
        return c::waitset_error_context;
    };
    unsafe {
        let _guard = runtime::RuntimeGuard::acquire();
        let record = find(owner, handle);
        if record.is_null() {
            return c::waitset_error_stale;
        }
        if (*record).closed {
            return c::waitset_error_closed;
        }
        if !creator(record) {
            return c::waitset_error_owner;
        }
        if (*record).waiting {
            return c::waitset_error_busy;
        }
        let slot = registration(record, id);
        if slot.is_null() {
            return c::waitset_error_stale;
        }
        notifications::unsubscribe((*record).notifications, ptr::addr_of_mut!((*slot).link));
        (*slot).id = 0;
        (*record).count -= 1;
        // Removal and Close remain possible at exhaustion; never wrap identity.
        (*record).revision = (*record).revision.saturating_add(1);
        c::waitset_ok
    }
}

unsafe fn collect(
    record: *mut Record,
    sample: Option<Sample>,
    capacity: usize,
    events: &mut [c::WaitSetEvent; MAX],
) -> c::WaitSetBatch {
    unsafe {
        let mut mask = 0u32;
        let mut stop_mask = 0u32;
        let mut flags = [0u32; MAX];
        let mut status = [c::waitset_event_ready as i32; MAX];
        for index in 0..(*record).capacity {
            let slot = &mut (*record).registrations[index];
            if slot.id == 0 {
                continue;
            }
            let ready = if slot.spec.kind == c::waitset_source_notification {
                if slot.link.closed {
                    status[index] = c::waitset_event_source_closed as i32;
                    flags[index] |= c::waitset_flag_rescan;
                }
                if slot
                    .link
                    .sequence
                    .saturating_sub(slot.spec.observed_revision)
                    > 1
                {
                    flags[index] |= c::waitset_flag_rescan | c::waitset_flag_coalesced;
                }
                slot.link.closed || slot.link.sequence != slot.spec.observed_revision
            } else {
                if let Some(sample) = sample {
                    if sample.key != key(&slot.spec) || sample.instant_ns < slot.last_ns {
                        slot.clock_invalid = true;
                    } else {
                        slot.last_ns = sample.instant_ns;
                    }
                } else {
                    slot.clock_invalid = true;
                }
                if slot.clock_invalid {
                    status[index] = c::waitset_event_clock_changed as i32;
                    flags[index] |= c::waitset_flag_rescan;
                }
                slot.clock_invalid || sample.is_some_and(|s| s.instant_ns >= slot.spec.deadline_ns)
            };
            if ready {
                mask |= 1 << index;
                if slot.spec.flags & c::waitset_source_flag_stop != 0 {
                    stop_mask |= 1 << index;
                }
            }
        }
        let selected = policy::select_ready(
            mask,
            stop_mask,
            (*record).capacity,
            capacity,
            (*record).cursor,
        );
        let mut batch = c::WaitSetBatch {
            outcome: c::waitset_outcome_ready,
            count: selected.count as u32,
            configuration_revision: (*record).revision,
            flags: if selected.more {
                c::waitset_flag_more | c::waitset_flag_rescan
            } else {
                0
            },
            ..c::WaitSetBatch::default()
        };
        for (out, index) in events
            .iter_mut()
            .zip(selected.indices[..selected.count].iter().copied())
        {
            let slot = &(*record).registrations[index as usize];
            *out = c::WaitSetEvent {
                kind: slot.spec.kind,
                flags: flags[index as usize],
                registration: slot.id,
                user_tag: slot.spec.user_tag,
                revision: if slot.spec.kind == c::waitset_source_notification {
                    slot.link.sequence
                } else {
                    0
                },
                status: status[index as usize],
                ..c::WaitSetEvent::default()
            };
            batch.flags |= flags[index as usize];
            if slot.spec.flags & c::waitset_source_flag_stop != 0 {
                batch.outcome = c::waitset_outcome_cancelled;
            }
        }
        (*record).cursor = selected.next;
        for step in 0..(*record).capacity {
            let index = (selected.next + step) % (*record).capacity;
            if (*record).registrations[index].id != 0 {
                batch.next_registration = (*record).registrations[index].id;
                break;
            }
        }
        batch
    }
}
unsafe fn finish_wait(owner: *mut Owner, handle: u64) {
    unsafe {
        let _guard = runtime::RuntimeGuard::acquire();
        let record = find(owner, handle);
        if !record.is_null() {
            (*record).waiting = false;
        }
    }
}
pub(super) unsafe fn wait(
    handle: u64,
    deadline: *const c::WaitSetDeadline,
    output: *mut c::WaitSetEvent,
    capacity: u32,
    batch_output: *mut c::WaitSetBatch,
) -> i32 {
    if capacity == 0
        || capacity > MAX as u32
        || !valid(deadline, 1)
        || !valid(output.cast_const(), capacity as usize)
        || !valid(batch_output.cast_const(), 1)
    {
        return c::waitset_error_invalid;
    }
    let bytes = capacity as usize * size_of::<c::WaitSetEvent>();
    if overlaps(
        output.addr(),
        bytes,
        deadline.addr(),
        size_of::<c::WaitSetDeadline>(),
    ) || overlaps(
        batch_output.addr(),
        size_of::<c::WaitSetBatch>(),
        deadline.addr(),
        size_of::<c::WaitSetDeadline>(),
    ) || overlaps(
        output.addr(),
        bytes,
        batch_output.addr(),
        size_of::<c::WaitSetBatch>(),
    ) {
        return c::waitset_error_invalid;
    }
    let deadline = unsafe { deadline.read() };
    if deadline.version != 1
        || deadline.size as usize != size_of::<c::WaitSetDeadline>()
        || deadline.reserved0 != 0
    {
        return c::waitset_error_invalid;
    }
    let mut budget = Budget {
        kind: deadline.kind,
        absolute_ns: deadline.absolute_ns,
        last_ns: deadline.last_ns,
        key: ClockKey {
            source: deadline.clock_source,
            generation: deadline.clock_generation,
            event_backend: deadline.event_backend,
            tick_hz: deadline.tick_hz,
        },
    };
    if let Err(error) = budget.validate() {
        return policy_error(error);
    }
    if !notifications::can_sleep() {
        return c::waitset_error_context;
    }
    let Some((owner, _)) = owners() else {
        return c::waitset_error_context;
    };
    let mut entered = false;
    loop {
        let mut rows = [c::WaitSetEvent::default(); MAX];
        let flags = runtime::save_runtime();
        let record = unsafe { find(owner, handle) };
        if record.is_null() {
            runtime::restore_runtime(flags);
            if !entered {
                return c::waitset_error_stale;
            }
            unsafe {
                batch_output.write(c::WaitSetBatch {
                    outcome: c::waitset_outcome_closed,
                    ..c::WaitSetBatch::default()
                });
            }
            return c::waitset_ok;
        }
        unsafe {
            if !creator(record) {
                runtime::restore_runtime(flags);
                return c::waitset_error_owner;
            }
            if (*record).closed {
                runtime::restore_runtime(flags);
                batch_output.write(c::WaitSetBatch {
                    outcome: c::waitset_outcome_closed,
                    ..c::WaitSetBatch::default()
                });
                return c::waitset_ok;
            }
            if !entered && (*record).waiting {
                runtime::restore_runtime(flags);
                return c::waitset_error_busy;
            }
            entered = true;
            (*record).waiting = true;
            let sample = clock();
            let remaining = match budget.remaining(sample) {
                Ok(value) => value,
                Err(error) => {
                    (*record).waiting = false;
                    runtime::restore_runtime(flags);
                    return policy_error(error);
                }
            };
            let mut batch = collect(record, sample, capacity as usize, &mut rows);
            if batch.count != 0 || remaining == Some(0) {
                if batch.count == 0 {
                    batch.outcome = c::waitset_outcome_timeout;
                }
                (*record).waiting = false;
                runtime::restore_runtime(flags);
                ptr::copy_nonoverlapping(rows.as_ptr(), output, batch.count as usize);
                batch_output.write(batch);
                return c::waitset_ok;
            }
            let mut next_ns = remaining;
            for index in 0..(*record).capacity {
                let slot = &(*record).registrations[index];
                if slot.id != 0 && slot.spec.kind == c::waitset_source_timer {
                    let duration = slot
                        .spec
                        .deadline_ns
                        .saturating_sub(sample.expect("validated timer sample").instant_ns);
                    next_ns = Some(next_ns.map_or(duration, |value| value.min(duration)));
                }
            }
            let ticks = match next_ns {
                None => u64::MAX,
                Some(value) => {
                    match policy::ticks(value, sample.expect("finite checked sample").key.tick_hz) {
                        Ok(value) => value,
                        Err(error) => {
                            (*record).waiting = false;
                            runtime::restore_runtime(flags);
                            return policy_error(error);
                        }
                    }
                }
            };
            let request = WaitRequest {
                timeout: ticks,
                reason: Text::literal(b"program-waitset"),
                predicate: None,
                context: ptr::null_mut(),
                release: None,
                release_context: ptr::null_mut(),
                flags,
                consume_runtime: true,
            };
            // This consumes the exact owner into original queue admission and
            // saved dispatch. No Record/queue/source pointer is read afterwards.
            let result =
                sync_wait::r4os_rust_sync_wait(ptr::addr_of_mut!((*record).queue), &request);
            match result {
                1 | 2 => {}
                3 | 4 => {
                    finish_wait(owner, handle);
                    batch_output.write(c::WaitSetBatch {
                        outcome: c::waitset_outcome_closed,
                        ..c::WaitSetBatch::default()
                    });
                    return c::waitset_ok;
                }
                0 | 5 => {
                    finish_wait(owner, handle);
                    return c::waitset_error_context;
                }
                _ => panic!("Invalid waitset wait result"),
            }
        }
    }
}

unsafe fn close_record(owner: *mut Owner, handle: u64) -> i32 {
    unsafe {
        let record = {
            let _guard = runtime::RuntimeGuard::acquire();
            let record = find(owner, handle);
            if record.is_null() {
                return c::waitset_error_stale;
            }
            (*record).closed = true;
            (*record).waiting = false;
            for index in 0..(*record).capacity {
                let slot = ptr::addr_of_mut!((*record).registrations[index]);
                notifications::unsubscribe(
                    (*record).notifications,
                    ptr::addr_of_mut!((*slot).link),
                );
                (*slot).id = 0;
            }
            (*record).count = 0;
            sync_wait::r4os_rust_sync_queue(
                ptr::addr_of_mut!((*record).queue),
                4,
                ptr::null_mut(),
                3,
                ptr::null_mut(),
            );
            assert!((*record).queue.core.count == 0);
            (*owner).tree.remove(ptr::addr_of_mut!((*record).node));
            record
        };
        if heap::r4os_rust_heap_free(record.cast(), size_of::<Record>()) == 0 {
            return c::waitset_ok;
        }
        let _guard = runtime::RuntimeGuard::acquire();
        assert!(find(owner, handle).is_null());
        (*owner)
            .tree
            .insert(handle, ptr::addr_of_mut!((*record).node));
        c::waitset_error_release
    }
}
pub(super) unsafe fn close(handle: u64) -> i32 {
    let Some(_operation) = notifications::Operation::enter() else {
        return c::waitset_error_context;
    };
    let Some((owner, _)) = owners() else {
        return c::waitset_error_context;
    };
    unsafe { close_record(owner, handle) }
}
/// Exact retired creator or complete process cleanup. Each Runtime interval
/// visits one record and at most32links; backing free always runs unlocked.
/// Failed frees remain closed/indexed, while all other matching sets are still
/// detached in this pass. No Task pointer survives retirement or an unlock.
pub(super) unsafe fn cleanup(owner: *mut Owner, creator_identity: Option<(u32, u64)>) -> bool {
    unsafe {
        let upper = {
            let _guard = runtime::RuntimeGuard::acquire();
            if creator_identity.is_none() {
                (*owner).closing = true;
            }
            *SERIAL.0.get()
        };
        let mut after = 0;
        let mut success = true;
        loop {
            let handle = {
                let _guard = runtime::RuntimeGuard::acquire();
                let mut cursor = (*owner).tree.root;
                let mut selected = ptr::null_mut();
                while !cursor.is_null() {
                    if (*cursor).key > after {
                        selected = cursor;
                        cursor = (*cursor).children[0];
                    } else {
                        cursor = (*cursor).children[1];
                    }
                }
                if selected.is_null() || (*selected).key > upper {
                    break;
                }
                after = (*selected).key;
                let record = selected.cast::<Record>();
                if creator_identity.is_some_and(|(id, generation)| {
                    id != (*record).creator_id || generation != (*record).creator_generation
                }) {
                    continue;
                }
                after
            };
            let result = close_record(owner, handle);
            if result != c::waitset_ok && result != c::waitset_error_stale {
                success = false;
            }
        }
        success
    }
}
const _: () = {
    assert!(size_of::<Owner>() == 24);
    assert!(align_of::<Owner>() == 8);
    assert!(core::mem::offset_of!(Record, node) == 0);
    assert!(core::mem::offset_of!(Registration, link) == 0);
    assert!(size_of::<Record>() < 8192);
};
