//! Original native allocation broker runtime. Placement/RM work remains in
//! R4D; the sole graphics worker publishes notifications outside the BO owner.
use super::{
    allocation_state as model, buffer_api as api, buffers, driver_memory,
    types::{Error, Handle, Owner},
};
use crate::{
    contract as a,
    driver::{api_admission, irq_router, work},
    platform::{monotonic, timer},
    sched::{
        scheduler, sync_objects,
        sync_types::{Event, WaitQueue},
        task,
    },
};
use core::{cell::UnsafeCell, ffi::c_void, mem::size_of, ptr};
const CAPACITY: usize = 128;
pub const RESULT_OWNER: Owner = Owner {
    kind: 0,
    id: 4,
    generation: 1,
};
const EMPTY_ENTRY: model::Entry = model::Entry {
    handle: Handle::EMPTY,
    owner: Owner {
        kind: 1,
        id: 0,
        generation: 0,
    },
    provider: Handle::EMPTY,
    allocation: a::GfxNativeAllocation {
        version: 1,
        size: 64,
        adapter_id: 0,
        kind: 0,
        memory_generation: 0,
        deadline_ns: 0,
        byte_length: 0,
        width: 0,
        height: 0,
        format: 0,
        usage: 0,
        layout: 0,
        reserved0: 0,
    },
    phase: 0,
    result: 0,
    completed_ns: 0,
    open: false,
    claimed: false,
    reference: Handle::EMPTY,
    waiters: 0,
    notification_pending: false,
    publishing: false,
};
#[derive(Clone, Copy, Default)]
struct Waiter {
    task_id: u32,
    generation: u64,
    request: Handle,
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
    entries: [model::Entry; CAPACITY],
    serial: u64,
    waiters: [Waiter; 256],
    providers: [Provider; 16],
    provider_serial: u64,
    service_needed: bool,
}
struct Cell<T>(UnsafeCell<T>);
// SAFETY: the original BO owner serializes all state. Events are stable until
// original waiter/publishing barriers permit reuse; no borrow crosses a wait.
unsafe impl<T> Sync for Cell<T> {}
static STATE: Cell<State> = Cell(UnsafeCell::new(State {
    entries: [EMPTY_ENTRY; CAPACITY],
    serial: 0,
    waiters: [Waiter {
        task_id: 0,
        generation: 0,
        request: Handle::EMPTY,
    }; 256],
    providers: [Provider::EMPTY; 16],
    provider_serial: 0,
    service_needed: false,
}));
static EVENTS: Cell<[Event; CAPACITY]> = Cell(UnsafeCell::new(
    [const {
        Event {
            signaled: false,
            mode: 0,
            queue: WaitQueue::EMPTY,
        }
    }; CAPACITY],
));
unsafe fn state() -> model::View {
    unsafe {
        let s = STATE.0.get();
        model::View {
            entries: ptr::addr_of_mut!((*s).entries).cast(),
            capacity: CAPACITY,
            serial: ptr::addr_of_mut!((*s).serial),
        }
    }
}
unsafe fn event(i: usize) -> *mut Event {
    assert!(i < CAPACITY);
    unsafe { EVENTS.0.get().cast::<Event>().add(i) }
}
// Queue availability/wakeup stays with the existing display queue (.83.23).

pub(crate) fn available() -> bool {
    unsafe { gfx_queue_peer(0) }
}
pub(crate) fn wake() {
    unsafe {
        gfx_queue_peer(1);
    }
}
fn now() -> u64 {
    monotonic::now_nanoseconds().unwrap_or(0)
}
unsafe fn provider_locked(h: Handle, o: Owner) -> Result<*mut Provider, Error> {
    unsafe {
        if h.id == 0 || h.id > 16 {
            return Err(Error::Stale);
        }
        let p = (*STATE.0.get())
            .providers
            .as_mut_ptr()
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
    unsafe { (*p).notifying || (*p).work_handle != 0 || state().retains_provider((*p).handle) }
}
pub(crate) fn register(owner: Owner, config: a::GfxNativeProvider) -> Result<Handle, Error> {
    if !available() || irq_router::in_dispatch() {
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
    let instant = now();
    let busy = {
        let _lock = api::Locked::acquire();
        unsafe {
            let p = provider_locked(h, owner)?;
            (*p).closing = true;
            (*STATE.0.get()).service_needed = true;
            state().close_provider(h, instant);
            if held_locked(p) {
                true
            } else {
                p.write(Provider::EMPTY);
                false
            }
        }
    };
    wake();
    if busy { Err(Error::Busy) } else { Ok(()) }
}
pub(crate) fn closing_driver(id: u32) {
    let instant = now();
    {
        let _lock = api::Locked::acquire();
        unsafe {
            let s = STATE.0.get();
            (*s).service_needed = true;
            for i in 0..16 {
                let p = ptr::addr_of_mut!((*s).providers[i]);
                if id != 0 && (*p).owner.id == u64::from(id) {
                    (*p).closing = true;
                    (*p).retiring = true;
                    state().close_provider((*p).handle, instant);
                }
            }
        }
    }
    wake();
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
pub(crate) fn start(
    owner: Owner,
    input: a::GfxNativeAllocation,
) -> Result<a::GfxNativeStatus, Error> {
    if !available() || irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let instant = now();
    model::validate(input, instant)?;
    let result = {
        let _lock = api::Locked::acquire();
        unsafe {
            let selected = (*STATE.0.get())
                .providers
                .iter()
                .find(|p| p.handle.id != 0 && p.config.adapter_id == input.adapter_id)
                .copied()
                .ok_or(Error::Unsupported)?;
            if selected.closing {
                return Err(Error::Closed);
            }
            if selected.config.memory_generation != input.memory_generation {
                return Err(Error::Stale);
            }
            driver_memory::admit_locked(selected.owner)?;
            let e = state().start(owner, selected.handle, input, instant)?;
            (*STATE.0.get()).service_needed = true;
            event((*e).handle.id as usize - 1).write(Event {
                signaled: false,
                mode: 0,
                queue: WaitQueue::EMPTY,
            });
            (*e).status()
        }
    };
    wake();
    Ok(result)
}
pub(crate) fn query(owner: Owner, h: Handle) -> Result<a::GfxNativeStatus, Error> {
    let _lock = api::Locked::acquire();
    unsafe { Ok((*state().owned(h, owner)?).status()) }
}
pub(crate) unsafe fn receive(owner: Owner, h: Handle, output: *mut a::GfxBufferReference) -> i32 {
    unsafe {
        enum Received {
            Terminal(i32),
            Reference(a::GfxBufferReference),
        }
        let instant = now();
        let result = (|| -> Result<Received, Error> {
            buffers::lock_prepared(buffers::Need {
                references: 1,
                ..buffers::Need::default()
            })?;
            let _lock = api::Locked;
            let e = state().owned(h, owner)?;
            if (*e).phase != 2 {
                return Err(Error::Busy);
            }
            if (*e).result != 1 {
                return Ok(Received::Terminal((*e).result));
            }
            let reference = buffers::table().share((*e).reference, owner)?;
            let v = api::reference_locked(reference, owner)
                .expect("Original allocation result reference");
            let retained = state()
                .close(h, owner, instant)
                .expect("Original allocation close");
            (*STATE.0.get()).service_needed = true;
            buffers::table()
                .drop_reference(retained, RESULT_OWNER)
                .expect("Original allocation retained reference");
            Ok(Received::Reference(v))
        })();
        // Preserve the stored terminal failure verbatim, without writing output or
        // waking the queue. It is not a new memory-owner error conversion.
        match result {
            Err(e) => api::status(e),
            Ok(Received::Terminal(result)) => result,
            Ok(Received::Reference(v)) => {
                output.write(v);
                wake();
                1
            }
        }
    }
}
unsafe fn close_locked(owner: Owner, h: Handle, instant: u64) -> Result<(), Error> {
    unsafe {
        let reference = state().close(h, owner, instant)?;
        (*STATE.0.get()).service_needed = true;
        if reference.id != 0 {
            buffers::table()
                .drop_reference(reference, RESULT_OWNER)
                .expect("Original closed allocation reference");
        }
        Ok(())
    }
}
pub(crate) fn close(owner: Owner, h: Handle) -> Result<(), Error> {
    let instant = now();
    {
        let _lock = api::Locked::acquire();
        unsafe { close_locked(owner, h, instant) }?;
    }
    wake();
    Ok(())
}
pub(crate) fn stopped(owner: Owner) {
    let instant = now();
    {
        let _lock = api::Locked::acquire();
        unsafe {
            for i in 0..CAPACITY {
                let e = ptr::addr_of!((*STATE.0.get()).entries[i]);
                if (*e).handle.id != 0 && (*e).open && (*e).owner == owner {
                    close_locked(owner, (*e).handle, instant).expect("Original stopped allocation");
                }
            }
        }
    }
    wake();
}
pub(crate) fn take(owner: Owner, p: Handle) -> Result<a::GfxNativeJob, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let instant = now();
    let _lock = api::Locked::acquire();
    unsafe {
        driver_memory::admit_locked(owner)?;
        let provider = provider_locked(p, owner)?;
        if (*provider).closing {
            return Err(Error::Closed);
        }
        let e = state().take(p, instant);
        if e.is_null() {
            return Err(Error::Busy);
        }
        Ok(a::GfxNativeJob {
            request: api::public_handle((*e).handle),
            allocation: (*e).allocation,
            ..a::GfxNativeJob::default()
        })
    }
}
pub(crate) fn complete(
    owner: Owner,
    p: Handle,
    h: Handle,
    result: i32,
    reference: a::GfxBufferHandle,
) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    if result == 0
        || !(a::gfx_queue_error_device_lost..=1).contains(&result)
        || reference.reserved0 != 0
        || (result != 1 && (reference.id != 0 || reference.generation != 0))
    {
        return Err(Error::Invalid);
    }
    let instant = now();
    buffers::lock_prepared(buffers::Need {
        references: usize::from(result == 1),
        ..buffers::Need::default()
    })?;
    {
        let _lock = api::Locked;
        unsafe {
            provider_locked(p, owner)?;
            let e = state().claim(h, p)?;
            state().expire(instant);
            let mut outcome = result;
            if result == 1 {
                let source = api::handle(reference)?;
                let d = buffers::read().describe(source, owner)?;
                if !model::matches((*e).allocation, d, owner)
                    || buffers::read().read_only(source, owner)?
                    || buffers::read().mapping_only(source, owner)?
                {
                    return Err(Error::Invalid);
                }
                if (*e).phase != 2 {
                    (*e).reference = match buffers::table().share(source, RESULT_OWNER) {
                        Ok(h) => h,
                        Err(err) => {
                            outcome = api::status(err);
                            Handle::EMPTY
                        }
                    };
                }
            }
            state().finish(h, p, outcome, instant)?;
            (*STATE.0.get()).service_needed = true;
        }
    }
    wake();
    Ok(())
}
pub(crate) fn retired_task(id: u32, generation: u64) {
    {
        let _lock = api::Locked::acquire();
        unsafe {
            let s = STATE.0.get();
            (*s).service_needed = true;
            for i in 0..256 {
                let w = ptr::addr_of_mut!((*s).waiters[i]);
                if (*w).task_id == id && (*w).generation == generation {
                    let e = state()
                        .find((*w).request)
                        .expect("Original allocation waiter");
                    assert!((*e).waiters != 0);
                    (*e).waiters -= 1;
                    w.write(Waiter::default());
                }
            }
        }
    }
    wake();
}
pub(crate) fn wait(owner: Owner, h: Handle, timeout: u64) -> Result<a::GfxNativeStatus, Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Unavailable);
    }
    let current = scheduler::current();
    if current.is_null() {
        return Err(Error::Unavailable);
    }
    let slot = {
        let _lock = api::Locked::acquire();
        unsafe {
            let e = state().owned(h, owner)?;
            if (*e).phase == 2 {
                return Ok((*e).status());
            }
            let s = STATE.0.get();
            let slot = (*s)
                .waiters
                .iter()
                .position(|w| w.task_id == 0)
                .ok_or(Error::Capacity)?;
            (*e).waiters += 1;
            let (id, generation) = task::immutable_identity(current);
            (*s).waiters[slot] = Waiter {
                task_id: id,
                generation,
                request: h,
            };
            slot
        }
    };
    let waited =
        unsafe { sync_objects::r4os_rust_sync_event(event(h.id as usize - 1), 2, timeout) };
    let snapshot = {
        let _lock = api::Locked::acquire();
        unsafe {
            let e = state().find(h).expect("Original waited allocation");
            (*e).waiters -= 1;
            (*STATE.0.get()).waiters[slot] = Waiter::default();
            (*STATE.0.get()).service_needed = true;
            (*e).status()
        }
    };
    wake();
    if snapshot.phase == 2 {
        Ok(snapshot)
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
    if id == 0 {
        return -1;
    }
    if !api_admission::r4os_rust_driver_api_enter(id, u64::from(timer::frequency().max(1)) * 3) {
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
    if selected.owner.id != u64::from(id) || selected.closing {
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
    {
        let _lock = api::Locked::acquire();
        unsafe {
            state().expire(instant);
        }
    }
    for i in 0..CAPACITY {
        let publish = {
            let _lock = api::Locked::acquire();
            unsafe {
                let e = ptr::addr_of_mut!((*STATE.0.get()).entries[i]);
                let publish = (*e).handle.id != 0 && (*e).notification_pending && !(*e).publishing;
                if publish {
                    (*e).notification_pending = false;
                    (*e).publishing = true;
                }
                publish
            }
        };
        if publish {
            unsafe {
                sync_objects::r4os_rust_sync_event(event(i), 0, 0);
            }
            let _lock = api::Locked::acquire();
            unsafe {
                (*STATE.0.get()).entries[i].publishing = false;
            }
        }
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
                let s = STATE.0.get();
                let p = ptr::addr_of_mut!((*s).providers[i]);
                let ready = !(*p).closing
                    && !(*p).notifying
                    && (*p).handle == snapshot.handle
                    && (*s)
                        .entries
                        .iter()
                        .any(|e| e.handle.id != 0 && e.phase == 0 && e.provider == (*p).handle);
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
        state().reap();
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
        for e in &(*s).entries {
            if e.handle.id != 0 && (e.phase != 2 || e.notification_pending) {
                pending = true;
            }
        }
        (*s).service_needed = pending;
        pending
    }
}

/// # Safety
/// Each operation receives its exact initialized resident private/public C
/// record. Only copied values survive waits; node/owner lifetime is unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_allocations_run(
    op: u32,
    args: *const super::buffer_owner::Args,
    out: *mut c_void,
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
                4 => out.cast::<a::GfxNativeStatus>().write(start(
                    args.owner,
                    args.input.cast::<a::GfxNativeAllocation>().read(),
                )?),
                5 => out
                    .cast::<a::GfxNativeStatus>()
                    .write(query(args.owner, args.handle)?),
                6 => close(args.owner, args.handle)?,
                7 => stopped(args.owner),
                8 => out
                    .cast::<a::GfxNativeJob>()
                    .write(take(args.owner, args.handle)?),
                9 => complete(
                    args.owner,
                    args.handle,
                    args.input.cast::<Handle>().read(),
                    args.access as i32,
                    args.other_input.cast::<a::GfxBufferHandle>().read(),
                )?,
                10 => retired_task(args.value as u32, args.offset),
                11 => out.cast::<a::GfxNativeStatus>().write(wait(
                    args.owner,
                    args.handle,
                    args.value,
                )?),
                12 => out.cast::<bool>().write(service()),
                _ => panic!("Invalid allocation runtime operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
/// # Safety
/// Exact initialized Args plus writable resident output, with caller admission
/// retained until the copied terminal status or imported reference is returned.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_allocation_receive(
    args: *const super::buffer_owner::Args,
    out: *mut a::GfxBufferReference,
) -> i32 {
    let args = unsafe { args.read() };
    unsafe { receive(args.owner, args.handle, out) }
}

unsafe fn gfx_queue_peer(operation: u32) -> bool {
    match operation {
        0 => crate::display::queue::available(),
        1 => {
            crate::display::queue::wake();
            true
        }
        _ => panic!("Invalid graphics queue link"),
    }
}
