//! Original resident cross-process VA broker. One BO owner admits metadata;
//! the existing sole graphics worker publishes events/notifies the exact R4D.
use super::{
    allocations, buffer_api as api, buffers, driver_memory,
    layout::Binding,
    types::{Error, Handle, Owner},
    virtual_state as model,
};
use crate::{
    contract as a,
    driver::{api_admission, irq_router, treap::Tree, work},
    memory::heap,
    platform::monotonic,
    sched::{
        scheduler, sync_objects,
        sync_types::{Event, WaitQueue},
        task,
    },
};
use core::{
    cell::UnsafeCell,
    mem::{align_of, size_of},
    ptr,
};
#[repr(C)]
struct Record {
    value: model::Entry,
    ready: Event,
    retired: Event,
    publishing: bool,
    waiters: usize,
    reference: a::GfxBufferReference,
    driver: Owner,
}
impl Default for Record {
    fn default() -> Self {
        Self {
            value: model::Entry::default(),
            ready: event_new(),
            retired: event_new(),
            publishing: false,
            waiters: 0,
            reference: a::GfxBufferReference::default(),
            driver: Owner {
                kind: 2,
                id: 0,
                generation: 0,
            },
        }
    }
}
struct Waiter {
    next: *mut Waiter,
    previous: *mut Waiter,
    record: *mut Record,
    task_id: u32,
    generation: u64,
}
#[derive(Clone, Copy)]
struct Provider {
    handle: Handle,
    owner: Owner,
    config: a::GfxNativeProvider,
    closing: bool,
    retiring: bool,
    notifying: bool,
    work_handle: u32,
}
impl Provider {
    const EMPTY: Self = Self {
        handle: Handle::EMPTY,
        owner: Owner {
            kind: 2,
            id: 0,
            generation: 0,
        },
        config: a::GfxNativeProvider {
            version: 1,
            size: 40,
            adapter_id: 0,
            reserved0: 0,
            memory_generation: 0,
            notify: 0,
            context: 0,
        },
        closing: false,
        retiring: false,
        notifying: false,
        work_handle: 0,
    };
}
struct State {
    store: model::Store,
    waiters: *mut Waiter,
    providers: [Provider; 16],
    provider_serial: u64,
    service_needed: bool,
}
struct Cell(UnsafeCell<State>);
// SAFETY: one original BO owner serializes metadata and exact node identities.
// Heap/event/driver calls retain only raw lifetime-pinned identities, never a
// mutable Rust reference to State or any linked node across a blocking call.
unsafe impl Sync for Cell {}
static STATE: Cell = Cell(UnsafeCell::new(State {
    store: model::Store {
        tree: Tree {
            root: ptr::null_mut(),
            xorshift: 0,
        },
        serial: 0,
    },
    waiters: ptr::null_mut(),
    providers: [Provider::EMPTY; 16],
    provider_serial: 0,
    service_needed: false,
}));
fn event_new() -> Event {
    Event {
        signaled: false,
        mode: 0,
        queue: WaitQueue::EMPTY,
    }
}
fn now() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
unsafe fn store() -> *mut model::Store {
    unsafe { ptr::addr_of_mut!((*STATE.0.get()).store) }
}
fn record(e: *mut model::Entry) -> *mut Record {
    e.cast()
}
fn allocate<T>() -> Result<*mut T, Error> {
    let p: *mut T = heap::r4os_rust_heap_alloc(size_of::<T>(), align_of::<T>()).cast();
    if p.is_null() {
        Err(Error::OutOfMemory)
    } else {
        Ok(p)
    }
}
fn release<T>(value: *mut T) {
    assert!(unsafe { heap::r4os_rust_heap_free(value.cast(), size_of::<T>()) } == 0);
}
struct Prepared<T> {
    value: *mut T,
    release: bool,
}
impl<T> Drop for Prepared<T> {
    fn drop(&mut self) {
        if self.release {
            release(self.value);
        }
    }
}
unsafe fn provider_locked(h: Handle, o: Owner) -> Result<*mut Provider, Error> {
    unsafe {
        if h.id == 0 || h.id > 16 {
            return Err(Error::Stale);
        }
        let p = ptr::addr_of_mut!((*STATE.0.get()).providers)
            .cast::<Provider>()
            .add(h.id as usize - 1);
        if (*p).handle != h {
            return Err(Error::Stale);
        }
        if (*p).owner != o {
            return Err(Error::WrongOwner);
        }
        Ok(p)
    }
}
unsafe fn held_locked(p: *const Provider) -> bool {
    unsafe {
        if (*p).notifying || (*p).work_handle != 0 {
            return true;
        }
        let mut node = (*store()).tree.min();
        while !node.is_null() {
            let e = node.cast::<model::Entry>();
            if (*e).provider == (*p).handle
                && (!(*e).retired || (*record(e)).reference.reference.id != 0)
            {
                return true;
            }
            node = model::next(node);
        }
        false
    }
}
pub(crate) fn register(owner: Owner, config: a::GfxNativeProvider) -> Result<Handle, Error> {
    if !allocations::available() || irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    if config.version != 1
        || config.size < size_of::<a::GfxNativeProvider>() as u32
        || config.reserved0 != 0
        || config.adapter_id == 0
        || config.memory_generation == 0
        || config.notify == 0
    {
        return Err(Error::Invalid);
    }
    let _lock = api::Locked::acquire();
    unsafe {
        driver_memory::admit_locked(owner)?;
        let s = STATE.0.get();
        for p in (*s).providers {
            if p.handle.id != 0 && p.config.adapter_id == config.adapter_id {
                return Err(Error::Busy);
            }
        }
        if (*s).provider_serial == u64::MAX {
            return Err(Error::Exhausted);
        }
        for i in 0..16 {
            if (*s).providers[i].handle.id == 0 {
                (*s).provider_serial += 1;
                let h = Handle {
                    id: i as u32 + 1,
                    generation: (*s).provider_serial,
                };
                (*s).providers[i] = Provider {
                    handle: h,
                    owner,
                    config,
                    ..Provider::EMPTY
                };
                return Ok(h);
            }
        }
    }
    Err(Error::Capacity)
}
pub(crate) fn unregister(owner: Owner, h: Handle) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let busy = {
        let _lock = api::Locked::acquire();
        unsafe {
            let p = provider_locked(h, owner)?;
            (*p).closing = true;
            (*store()).close_provider(h);
            (*STATE.0.get()).service_needed = true;
            if held_locked(p) {
                true
            } else {
                p.write(Provider::EMPTY);
                false
            }
        }
    };
    allocations::wake();
    if busy { Err(Error::Busy) } else { Ok(()) }
}
pub(crate) fn closing_driver(id: u32) {
    {
        let _lock = api::Locked::acquire();
        unsafe {
            let s = STATE.0.get();
            for i in 0..16 {
                let p = ptr::addr_of_mut!((*s).providers[i]);
                if id != 0 && (*p).owner.id == u64::from(id) {
                    (*p).closing = true;
                    (*p).retiring = true;
                    (*store()).close_provider((*p).handle);
                    (*s).service_needed = true;
                }
            }
        }
    }
    allocations::wake();
}
pub(crate) fn retains_driver(id: u32) -> bool {
    let _lock = api::Locked::acquire();
    unsafe {
        (*STATE.0.get())
            .providers
            .iter()
            .any(|p| id != 0 && p.owner.id == u64::from(id))
    }
}
/// # Safety
/// The exact process admission bit remains resident until this original BO
/// owner transition finishes, before process exit is published.
pub(crate) unsafe fn stop_admission(closed: *mut bool) {
    let _lock = api::Locked::acquire();
    unsafe {
        closed.write(true);
    }
}
pub(crate) unsafe fn start(
    owner: Owner,
    closed: *const bool,
    input: a::GfxVirtualRequest,
) -> Result<a::GfxVirtualStatus, Error> {
    unsafe {
        if !allocations::available() || irq_router::in_dispatch() {
            return Err(Error::Unavailable);
        }
        let instant = now();
        model::validate(input, instant)?;
        let value = allocate::<Record>()?;
        value.write(Record::default());
        let mut prepared = Prepared {
            value,
            release: true,
        };
        buffers::lock_prepared(buffers::Need {
            references: usize::from(input.kind == 2),
            ..buffers::Need::default()
        })?;
        let result = {
            let _lock = api::Locked;
            if closed.read() {
                return Err(Error::Closed);
            }
            let provider = (*STATE.0.get())
                .providers
                .iter()
                .find(|p| p.handle.id != 0 && p.config.adapter_id == input.adapter_id)
                .copied()
                .ok_or(Error::Unsupported)?;
            if provider.closing {
                return Err(Error::Closed);
            }
            if provider.config.memory_generation != input.memory_generation {
                return Err(Error::Stale);
            }
            driver_memory::admit_locked(provider.owner)?;
            (*store()).parent(owner, provider.handle, input)?;
            (*value).driver = provider.owner;
            if input.kind == 2 {
                let reference = api::handle(input.reference)?;
                let d = buffers::read().describe(reference, owner)?;
                if buffers::read().read_only(reference, owner)?
                    || buffers::read().mapping_only(reference, owner)?
                {
                    return Err(Error::Unsupported);
                }
                if input.byte_length > d.bytes || input.byte_offset > d.bytes - input.byte_length {
                    return Err(Error::Invalid);
                }
                if !d.binding.portable()
                    && (d.binding.adapter != input.adapter_id
                        || d.binding.device_generation != input.memory_generation
                        || u64::from(d.binding.driver_owner) != provider.owner.id)
                {
                    return Err(Error::Stale);
                }
                let borrowed = buffers::table().share(reference, provider.owner)?;
                (*value).reference = api::reference_locked(borrowed, provider.owner)
                    .expect("Original VA backing reference");
            }
            if let Err(e) = (*store()).start(
                ptr::addr_of_mut!((*value).value),
                owner,
                provider.handle,
                input,
                instant,
            ) {
                if (*value).reference.reference.id != 0 {
                    buffers::table()
                        .drop_reference(
                            api::handle((*value).reference.reference)
                                .expect("Original VA retained handle"),
                            provider.owner,
                        )
                        .expect("Original VA admission rollback");
                }
                return Err(e);
            }
            (*STATE.0.get()).service_needed = true;
            (*value).value.status()
        };
        prepared.release = false;
        allocations::wake();
        Ok(result)
    }
}
pub(crate) fn query(owner: Owner, h: Handle) -> Result<a::GfxVirtualStatus, Error> {
    let _lock = api::Locked::acquire();
    unsafe { Ok((*(*store()).owned(h, owner)?).status()) }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Execution {
    pub entry: *mut model::Entry,
    pub driver: Owner,
    pub reference: Handle,
    pub binding: a::GfxNativeBinding,
}
/// # Safety
/// Shared BO/queue owner is held. The full imported reference belongs to this
/// resident VA record. No heap, callbacks or driver work may span this call.
pub(crate) unsafe fn retain_execution_locked(
    owner: Owner,
    input: a::GfxNativeResource,
    target: Binding,
) -> Result<Execution, Error> {
    unsafe {
        if input.version != 1
            || input.size != size_of::<a::GfxNativeResource>() as u32
            || input.access > 1
            || input.reserved0 != 0
        {
            return Err(Error::Invalid);
        }
        let e = (*store()).owned(api::handle(input.binding)?, owner)?;
        let value = record(e);
        let p = provider_locked((*e).provider, (*value).driver)?;
        if (*p).closing {
            return Err(Error::Closed);
        }
        if (*p).owner.id != u64::from(target.driver_owner)
            || (*p).config.adapter_id != target.adapter
            || (*p).config.memory_generation != target.device_generation
            || (*e).request.adapter_id != target.adapter
            || (*e).request.memory_generation != target.device_generation
            || (*value).reference.flags != 0
        {
            return Err(Error::Stale);
        }
        driver_memory::admit_locked((*p).owner)?;
        let reference = api::handle((*value).reference.reference)?;
        (&mut *e).retain_execution()?;
        Ok(Execution {
            entry: e,
            driver: (*value).driver,
            reference,
            binding: a::GfxNativeBinding {
                binding: api::public_handle((*e).handle),
                token: (*e).token,
                address: (*e).address,
                byte_length: (*e).request.byte_length,
                access: input.access,
                ..a::GfxNativeBinding::default()
            },
        })
    }
}
/// # Safety
/// Exact retained execution remains pinned under the shared BO/queue owner.
pub(crate) unsafe fn release_execution_locked(value: Execution) {
    unsafe {
        model::release_execution(value.entry);
        (*STATE.0.get()).service_needed = true;
    }
}
pub(crate) fn close(owner: Owner, h: Handle, mode: u32) -> Result<(), Error> {
    if mode > 1 {
        return Err(Error::Invalid);
    }
    {
        let _lock = api::Locked::acquire();
        unsafe {
            let e = (*store()).owned(h, owner)?;
            model::close(e, mode == 1, a::gfx_queue_error_wait_cancelled);
            (*STATE.0.get()).service_needed = true;
        }
    }
    allocations::wake();
    Ok(())
}
pub(crate) fn stopped(owner: Owner) {
    {
        let _lock = api::Locked::acquire();
        unsafe {
            (*store()).stopped(owner);
            (*STATE.0.get()).service_needed = true;
        }
    }
    allocations::wake();
}
pub(crate) fn take(owner: Owner, h: Handle) -> Result<a::GfxVirtualJob, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let instant = now();
    let _lock = api::Locked::acquire();
    unsafe {
        let p = provider_locked(h, owner)?;
        if !(*p).closing {
            driver_memory::admit_locked(owner)?;
        }
        let e = (*store()).take(h, instant);
        if e.is_null() {
            return Err(Error::Busy);
        }
        (*STATE.0.get()).service_needed = true;
        Ok(a::GfxVirtualJob {
            resource: api::public_handle((*e).handle),
            operation: (*e).claim,
            request: (*e).request,
            parent_token: if (*e).parent.is_null() {
                a::GfxVirtualToken::default()
            } else {
                (*(*e).parent).token
            },
            token: (*e).token,
            reference: (*record(e)).reference,
            ..a::GfxVirtualJob::default()
        })
    }
}
pub(crate) fn complete(
    owner: Owner,
    h: Handle,
    completion: a::GfxVirtualCompletion,
) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let instant = now();
    {
        let _lock = api::Locked::acquire();
        unsafe {
            provider_locked(h, owner)?;
            (*store()).expire(instant);
            (*store()).complete(h, completion)?;
            (*STATE.0.get()).service_needed = true;
        }
    }
    allocations::wake();
    Ok(())
}
unsafe fn unlink_waiter_locked(w: *mut Waiter) {
    unsafe {
        if (*w).previous.is_null() {
            (*STATE.0.get()).waiters = (*w).next;
        } else {
            (*(*w).previous).next = (*w).next;
        }
        if !(*w).next.is_null() {
            (*(*w).next).previous = (*w).previous;
        }
        assert!((*(*w).record).waiters != 0);
        (*(*w).record).waiters -= 1;
        (*STATE.0.get()).service_needed = true;
    }
}
pub(crate) fn retired_task(id: u32, generation: u64) {
    let mut detached: *mut Waiter = ptr::null_mut();
    {
        let _lock = api::Locked::acquire();
        unsafe {
            let mut next = (*STATE.0.get()).waiters;
            while !next.is_null() {
                let w = next;
                next = (*w).next;
                if (*w).task_id != id || (*w).generation != generation {
                    continue;
                }
                unlink_waiter_locked(w);
                (*w).next = detached;
                detached = w;
            }
        }
    }
    while !detached.is_null() {
        unsafe {
            let w = detached;
            detached = (*w).next;
            release(w);
        }
    }
    allocations::wake();
}
pub(crate) fn wait(
    owner: Owner,
    h: Handle,
    until: u32,
    timeout: u64,
) -> Result<a::GfxVirtualStatus, Error> {
    if until > 1 {
        return Err(Error::Invalid);
    }
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let current = scheduler::current();
    if current.is_null() {
        return Err(Error::Unavailable);
    }
    let waiter = {
        let Ok(_call) = buffers::Call::acquire() else {
            return Err(Error::WaitCancelled);
        };
        let value = allocate::<Waiter>()?;
        let mut prepared = Prepared {
            value,
            release: true,
        };
        {
            let _lock = api::Locked::acquire();
            unsafe {
                let e = (*store()).owned(h, owner)?;
                if (*e).settled(until) {
                    return Ok((*e).status());
                }
                let target = record(e);
                if (*target).waiters == usize::MAX {
                    return Err(Error::Exhausted);
                }
                let (id, generation) = task::immutable_identity(current);
                value.write(Waiter {
                    next: (*STATE.0.get()).waiters,
                    previous: ptr::null_mut(),
                    record: target,
                    task_id: id,
                    generation,
                });
                if !(*STATE.0.get()).waiters.is_null() {
                    (*(*STATE.0.get()).waiters).previous = value;
                }
                (*STATE.0.get()).waiters = value;
                (*target).waiters += 1;
            }
        }
        prepared.release = false;
        value
    };
    // Registration pins both events through kill and concurrent publication.
    let event = unsafe {
        if until == 0 {
            ptr::addr_of_mut!((*(*waiter).record).ready)
        } else {
            ptr::addr_of_mut!((*(*waiter).record).retired)
        }
    };
    let waited = unsafe { sync_objects::r4os_rust_sync_event(event, 2, timeout) };
    let Ok(_call) = buffers::Call::acquire() else {
        return Err(Error::WaitCancelled);
    }; // exact retirement owns the waiter
    let (result, done) = {
        let _lock = api::Locked::acquire();
        unsafe {
            let result = (*(*waiter).record).value.status();
            let done = (*(*waiter).record).value.settled(until);
            unlink_waiter_locked(waiter);
            (result, done)
        }
    };
    release(waiter);
    allocations::wake();
    if done {
        Ok(result)
    } else if waited == 2 {
        Err(Error::WaitTimeout)
    } else {
        Err(Error::WaitCancelled)
    }
}
unsafe extern "C" fn notify_driver(index: usize) -> i32 {
    if index >= 16 {
        return -1;
    }
    let id = work::r4os_rust_driver_work_current();
    // The exact pending VA request survives a refused attempt and service
    // retries it; the shared Work lane must remain available meanwhile.
    if id == 0 || !api_admission::r4os_rust_driver_api_enter(id, 0) {
        return -1;
    }
    struct Leave;
    impl Drop for Leave {
        fn drop(&mut self) {
            api_admission::r4os_rust_driver_api_leave();
        }
    }
    let _owner = Leave;
    let selected = {
        let _lock = api::Locked::acquire();
        unsafe { (*STATE.0.get()).providers[index] }
    };
    if selected.owner.id != u64::from(id) || selected.handle.id == 0 {
        return 0;
    }
    let callback: work::Handler = unsafe { core::mem::transmute(selected.config.notify as usize) };
    unsafe { callback(selected.config.context as usize) }
}
pub(crate) fn service() -> bool {
    let needed = {
        let _lock = api::Locked::acquire();
        unsafe { (*STATE.0.get()).service_needed }
    };
    if !needed {
        return false;
    }
    let instant = now();
    let mut next = {
        let _lock = api::Locked::acquire();
        unsafe {
            (*store()).expire(instant);
            (*store()).tree.min()
        }
    };
    let mut dropped = false;
    // Only this worker unlinks nodes. Concurrent admissions append serials.
    while !next.is_null() {
        unsafe {
            let e = next.cast::<model::Entry>();
            let value = record(e);
            let publish = {
                let _lock = api::Locked::acquire();
                next = model::next(next);
                let p = (*e).notifications;
                (*e).notifications = 0;
                (*value).publishing = p != 0;
                p
            };
            if publish & 1 != 0 {
                sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*value).ready), 0, 0);
            }
            if publish & 2 != 0 {
                sync_objects::r4os_rust_sync_event(ptr::addr_of_mut!((*value).retired), 0, 0);
            }
            let remove = {
                let _lock = api::Locked::acquire();
                (*value).publishing = false;
                if (*e).retired && (*value).reference.reference.id != 0 {
                    buffers::table()
                        .drop_reference(
                            api::handle((*value).reference.reference)
                                .expect("Original retired VA reference"),
                            (*value).driver,
                        )
                        .expect("Original VA reference release");
                    (*value).reference = a::GfxBufferReference::default();
                    dropped = true;
                }
                let remove =
                    model::Store::removable(&*e) && (*value).waiters == 0 && !(*value).publishing;
                if remove {
                    (*store()).remove(e);
                }
                remove
            };
            if remove {
                release(value);
            }
        }
    }
    if dropped {
        buffers::collect();
    }
    for i in 0..16 {
        let snapshot = {
            let _lock = api::Locked::acquire();
            unsafe { (*STATE.0.get()).providers[i] }
        };
        if snapshot.handle.id == 0 {
            continue;
        }
        if snapshot.work_handle != 0 {
            let mut status = work::CompletionStatus::default();
            let rc =
                unsafe { work::r4os_rust_driver_work_status(snapshot.work_handle, &mut status) };
            if (rc != 0 || matches!(status.state, 3 | 4))
                && (rc != 0 || work::r4os_rust_driver_work_release(snapshot.work_handle) == 0)
            {
                let _lock = api::Locked::acquire();
                unsafe {
                    let p = ptr::addr_of_mut!((*STATE.0.get()).providers[i]);
                    if (*p).handle == snapshot.handle && (*p).work_handle == snapshot.work_handle {
                        (*p).work_handle = 0;
                    }
                }
            }
            continue;
        }
        let ready = {
            let _lock = api::Locked::acquire();
            unsafe {
                let p = ptr::addr_of_mut!((*STATE.0.get()).providers[i]);
                let mut ready = false;
                if !(*p).notifying && (*p).handle == snapshot.handle {
                    let mut node = (*store()).tree.min();
                    while !node.is_null() {
                        let e = node.cast::<model::Entry>();
                        if (*e).provider == (*p).handle && (*e).pending().is_some() {
                            ready = true;
                            break;
                        }
                        node = model::next(node);
                    }
                }
                if ready {
                    (*p).notifying = true;
                }
                ready
            }
        };
        if !ready {
            continue;
        }
        let mut handle = 0;
        let rc = unsafe {
            work::r4os_rust_driver_work_submit(
                u32::try_from(snapshot.owner.id).expect("Original driver ID"),
                notify_driver,
                i,
                0,
                false,
                &mut handle,
            )
        };
        let _lock = api::Locked::acquire();
        unsafe {
            let p = ptr::addr_of_mut!((*STATE.0.get()).providers[i]);
            assert!((*p).handle == snapshot.handle);
            (*p).notifying = false;
            (*p).work_handle = if rc == 0 { handle } else { 0 };
        }
    }
    let _lock = api::Locked::acquire();
    unsafe {
        let s = STATE.0.get();
        let mut pending = false;
        for i in 0..16 {
            let p = ptr::addr_of_mut!((*s).providers[i]);
            if (*p).handle.id != 0 {
                if (*p).retiring && !held_locked(p) {
                    p.write(Provider::EMPTY);
                } else if (*p).work_handle != 0 || (*p).notifying {
                    pending = true;
                }
            }
        }
        let mut node = (*store()).tree.min();
        while !node.is_null() {
            let e = node.cast::<model::Entry>();
            if (*e).result == 0
                || (*e).has_claim
                || ((*e).closing && !(*e).retired)
                || (*e).notifications != 0
                || (model::Store::removable(&*e) && (*record(e)).waiters == 0)
            {
                pending = true;
            }
            node = model::next(node);
        }
        (*s).service_needed = pending;
        pending
    }
}
const _: () = {
    assert!(size_of::<Execution>() == 120);
    assert!(core::mem::offset_of!(Execution, entry) == 0);
    assert!(core::mem::offset_of!(Execution, driver) == 8);
    assert!(core::mem::offset_of!(Execution, reference) == 32);
    assert!(core::mem::offset_of!(Execution, binding) == 48);
};

/// # Safety
/// Operation-specific exact resident records and original BO/queue ownership
/// remain live. The execution pointer pins this exact entry until release.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_virtual_run(
    op: u32,
    args: *const super::buffer_owner::Args,
    out: *mut core::ffi::c_void,
) -> i32 {
    let args = unsafe { args.read() };
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                0 => out.cast::<Handle>().write(register(
                    args.owner,
                    args.input.cast::<a::GfxNativeProvider>().read(),
                )?),
                1 => unregister(args.owner, args.handle)?,
                2 => closing_driver(args.value as u32),
                3 => out.cast::<bool>().write(retains_driver(args.value as u32)),
                4 => stop_admission(args.buffer.cast()),
                5 => out.cast::<a::GfxVirtualStatus>().write(start(
                    args.owner,
                    args.other_input.cast(),
                    args.input.cast::<a::GfxVirtualRequest>().read(),
                )?),
                6 => out
                    .cast::<a::GfxVirtualStatus>()
                    .write(query(args.owner, args.handle)?),
                7 => out.cast::<Execution>().write(retain_execution_locked(
                    args.owner,
                    args.input.cast::<a::GfxNativeResource>().read(),
                    args.binding,
                )?),
                8 => release_execution_locked(args.input.cast::<Execution>().read()),
                9 => close(args.owner, args.handle, args.access)?,
                10 => stopped(args.owner),
                11 => out
                    .cast::<a::GfxVirtualJob>()
                    .write(take(args.owner, args.handle)?),
                12 => complete(
                    args.owner,
                    args.handle,
                    args.input.cast::<a::GfxVirtualCompletion>().read(),
                )?,
                13 => retired_task(args.value as u32, args.offset),
                14 => out.cast::<a::GfxVirtualStatus>().write(wait(
                    args.owner,
                    args.handle,
                    args.access,
                    args.value,
                )?),
                15 => out.cast::<bool>().write(service()),
                _ => panic!("Invalid virtual runtime operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
