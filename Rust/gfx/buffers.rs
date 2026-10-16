//! Original resident backing, dynamic metadata and exact release runtime.
//! BO references are confined to no-sleep metadata calls. Heap, VM/TLB and
//! external callbacks never span a metadata borrow or the BO lock.
use super::{
    buffer_owner as model,
    layout::{Binding, Descriptor},
    types::{self, Error, Handle, Owner},
};
use crate::{
    driver::irq_router,
    memory::{heap, phys, vm_query, vm_ranges, vm_transactions},
    sched::{
        sync_locks,
        sync_types::{Mutex, Text},
        task_context::{self, Unwind},
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{MaybeUninit, align_of, size_of},
    ptr,
};
pub const TOTAL_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
pub const PRODUCER_BUDGET: u64 = 2 * 1024 * 1024 * 1024;
pub const SYSTEM_RESERVE: u64 = 16 * 1024 * 1024;
pub const RASTER_OWNER: Owner = Owner {
    kind: 0,
    id: 1,
    generation: 1,
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Need {
    pub objects: usize,
    pub references: usize,
    pub leases: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Created {
    pub buffer: Handle,
    pub reference: Handle,
    pub address: u64,
    pub bytes: u64,
}
#[derive(Clone, Copy)]
struct Allocation {
    pointer: *mut u8,
    len: usize,
}
#[derive(Clone, Copy)]
struct Pool {
    pointer: *mut c_void,
    len: usize,
}
impl Pool {
    const EMPTY: Self = Self {
        pointer: ptr::null_mut(),
        len: 0,
    };
}
#[derive(Clone, Copy)]
struct Growth {
    busy: bool,
    owned: Option<Allocation>,
    retired: Option<Allocation>,
}
impl Growth {
    const EMPTY: Self = Self {
        busy: false,
        owned: None,
        retired: None,
    };
}
const EMPTY_BUDGET: model::DeviceBudget = model::DeviceBudget {
    present: false,
    driver: Owner::EMPTY,
    binding: Binding {
        adapter: 0,
        driver_owner: 0,
        device_generation: 0,
    },
    bytes: 0,
    closing: false,
};
struct State {
    pools: [Pool; 3],
    serial: u64,
    committed_bytes: u64,
    device_budgets: [model::DeviceBudget; 16],
    growth: [Growth; 3],
}
struct Cell<T>(UnsafeCell<T>);
// SAFETY: the original shared BO mutex admits every metadata access. Growth
// controls pin exact allocations across unlocked heap work; no Rust reference
// to State/pools survives a wait, allocator or VM operation.
unsafe impl<T> Sync for Cell<T> {}
static STATE: Cell<State> = Cell(UnsafeCell::new(State {
    pools: [Pool::EMPTY; 3],
    serial: 0,
    committed_bytes: 0,
    device_budgets: [EMPTY_BUDGET; 16],
    growth: [Growth::EMPTY; 3],
}));
static MUTEX: Cell<Mutex> = Cell(UnsafeCell::new(Mutex {
    rank: 100,
    mode: 1,
    name: Text::literal(b"gfx-buffer-owner"),
    ..Mutex::EMPTY
}));
pub fn mutex() -> *mut Mutex {
    MUTEX.0.get()
}
pub fn lock() {
    while !unsafe { sync_locks::r4os_rust_sync_mutex(mutex(), 0, 0) } {
        core::hint::spin_loop();
    }
}
pub fn unlock() {
    unsafe {
        sync_locks::r4os_rust_sync_mutex(mutex(), 2, 0);
    }
}
pub struct Call(Unwind);
impl Call {
    pub fn acquire() -> Result<Self, Error> {
        let t = unsafe { task_context::enter() };
        if t.admitted() {
            Ok(Self(t))
        } else {
            Err(Error::Busy)
        }
    }
}
impl Drop for Call {
    fn drop(&mut self) {
        unsafe {
            task_context::leave(self.0);
        }
    }
}
/// # Safety
/// Caller owns the exact shared BO metadata lock until the returned metadata
/// view is dropped. No callback, allocation, VM operation or wait spans it.
pub(crate) unsafe fn view() -> model::RawView {
    unsafe {
        let s = STATE.0.get();
        model::RawView {
            objects: (*s).pools[0].pointer.cast(),
            objects_len: (*s).pools[0].len,
            references: (*s).pools[1].pointer.cast(),
            references_len: (*s).pools[1].len,
            leases: (*s).pools[2].pointer.cast(),
            leases_len: (*s).pools[2].len,
            serial: ptr::addr_of_mut!((*s).serial),
            committed_bytes: ptr::addr_of_mut!((*s).committed_bytes),
            budget_bytes: TOTAL_BUDGET,
            producer_budget_bytes: PRODUCER_BUDGET,
            device_budgets: ptr::addr_of_mut!((*s).device_budgets).cast(),
            device_budgets_len: 16,
            dynamic_mask: 7,
        }
    }
}
/// # Safety
/// Same exact no-sleep owner/lifetime contract as view().
pub(crate) unsafe fn table<'a>() -> model::Table<'a> {
    unsafe { view().borrow() }
}
/// # Safety
/// Same exact no-sleep owner/lifetime contract as view(); read-only metadata.
pub(crate) unsafe fn read<'a>() -> model::ReadView<'a> {
    unsafe { view().borrow_read() }
}
fn needs(n: Need) -> [usize; 3] {
    [n.objects, n.references, n.leases]
}
fn pool_layout(kind: usize) -> (usize, usize) {
    match kind {
        0 => (size_of::<model::Object>(), align_of::<model::Object>()),
        1 => (
            size_of::<model::Reference>(),
            align_of::<model::Reference>(),
        ),
        2 => (size_of::<model::Lease>(), align_of::<model::Lease>()),
        _ => unreachable!(),
    }
}
fn free(a: Allocation) -> bool {
    unsafe { heap::r4os_rust_heap_free(a.pointer, a.len) == 0 }
}
struct FinishGrowth(usize);
impl Drop for FinishGrowth {
    fn drop(&mut self) {
        lock();
        unsafe {
            (*STATE.0.get()).growth[self.0].busy = false;
        }
        unlock();
    }
}
fn prepare(n: Need) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Busy);
    }
    let _call = Call::acquire()?;
    for (kind, needed) in needs(n).into_iter().enumerate() {
        if needed != 0 {
            prepare_pool(kind, needed)?;
        }
    }
    Ok(())
}
/// Success returns with the original metadata lock held. Allocation remains
/// outside it; retry only the original four times, never report contention OOM.
pub fn lock_prepared(n: Need) -> Result<(), Error> {
    if irq_router::in_dispatch() {
        return Err(Error::Busy);
    }
    for _ in 0..4 {
        lock();
        let ready = needs(n).into_iter().enumerate().all(|(kind, needed)| {
            needed == 0 || unsafe { read().free_slots(kind as u32, needed) >= needed }
        });
        if ready {
            return Ok(());
        }
        unlock();
        prepare(n)?;
    }
    Err(Error::Busy)
}
fn prepare_pool(kind: usize, needed: usize) -> Result<(), Error> {
    unsafe {
        let s = STATE.0.get();
        lock();
        if (*s).growth[kind].busy {
            unlock();
            return Err(Error::Busy);
        }
        if read().free_slots(kind as u32, needed) >= needed {
            unlock();
            return Ok(());
        }
        let old_count = (*s).pools[kind].len;
        let Some(added) = old_count.checked_add(needed) else {
            unlock();
            return Err(Error::Exhausted);
        };
        let count = 64.max(added.max(old_count.saturating_mul(2)));
        if count > u32::MAX as usize {
            unlock();
            return Err(Error::Exhausted);
        }
        let (element, alignment) = pool_layout(kind);
        let Some(bytes) = count.checked_mul(element) else {
            unlock();
            return Err(Error::Exhausted);
        };
        (*s).growth[kind].busy = true;
        let pending = (*s).growth[kind].retired;
        unlock();
        let _finish = FinishGrowth(kind);
        if let Some(old) = pending {
            if !free(old) {
                return Err(Error::Busy);
            }
            lock();
            (*s).growth[kind].retired = None;
            unlock();
        }
        let pointer = heap::r4os_rust_heap_alloc(bytes, alignment);
        if pointer.is_null() {
            return Err(Error::OutOfMemory);
        }
        let allocation = Allocation {
            pointer,
            len: bytes,
        };
        // The prefix is uninitialized: do not form a slice or read it. Only write
        // the independent new tail before copying the latest locked old prefix.
        for i in old_count..count {
            match kind {
                0 => pointer
                    .cast::<model::Object>()
                    .add(i)
                    .write(model::Object::default()),
                1 => pointer
                    .cast::<model::Reference>()
                    .add(i)
                    .write(model::Reference::default()),
                2 => pointer
                    .cast::<model::Lease>()
                    .add(i)
                    .write(model::Lease::default()),
                _ => unreachable!(),
            }
        }
        lock();
        let replaced = read().replace_storage(kind as u32, pointer.cast(), count);
        if let Err(e) = replaced {
            (*s).growth[kind].retired = Some(allocation);
            unlock();
            return Err(e);
        }
        (*s).pools[kind] = Pool {
            pointer: pointer.cast(),
            len: count,
        };
        let detached = (*s).growth[kind].owned;
        (*s).growth[kind].owned = Some(allocation);
        (*s).growth[kind].retired = detached;
        unlock();
        if let Some(old) = detached {
            if !free(old) {
                return Err(Error::Busy);
            }
            lock();
            (*s).growth[kind].retired = None;
            unlock();
        }
        Ok(())
    }
}
fn abort(ticket: model::Create) {
    lock();
    unsafe {
        table().abort(ticket).expect("Original BO abort ticket");
    }
    unlock();
}
pub fn create(owner: Owner, descriptor: Descriptor) -> Result<Created, Error> {
    if descriptor.location != 0 || !descriptor.binding.portable() {
        return Err(Error::Unsupported);
    }
    let _call = Call::acquire()?;
    collect();
    lock_prepared(Need {
        objects: 1,
        references: 1,
        leases: 0,
    })?;
    let ticket = unsafe { table().begin(owner, descriptor) };
    unlock();
    let ticket = ticket?;
    let mut stats = phys::Stats::default();
    unsafe {
        phys::r4os_rust_phys_stats(&mut stats);
    }
    let free_bytes = stats.free_frames * 4096;
    if free_bytes <= SYSTEM_RESERVE || ticket.bytes > free_bytes - SYSTEM_RESERVE {
        abort(ticket);
        return Err(Error::OutOfMemory);
    }
    let request = vm_ranges::Reserve {
        eviction_allowed: true,
        window: 8,
        kind: 8,
        owner: 5,
        owner_id: ticket.buffer.generation,
        name_ptr: b"graphics-buffer".as_ptr(),
        name_len: b"graphics-buffer".len(),
        flags: 2 | (1 << 63),
        base: 0,
        len: ticket.bytes,
    };
    let mut range = 0;
    if unsafe {
        vm_ranges::r4os_rust_vm_reserve(&request, 4096.max(descriptor.alignment), &mut range)
    } != 0
    {
        abort(ticket);
        return Err(Error::OutOfMemory);
    }
    let mut info = MaybeUninit::<vm_ranges::Range>::uninit();
    assert!(unsafe { vm_query::r4os_rust_vm_range_info(range, info.as_mut_ptr()) });
    let address = unsafe { info.assume_init() }.base;
    let mut committed = 0;
    while committed < ticket.bytes {
        let count = (ticket.bytes - committed).min(64 * 4096);
        if vm_transactions::r4os_rust_vm_commit(range, committed, count) != 0 {
            lock();
            unsafe {
                table()
                    .publish(
                        ticket,
                        model::Backing {
                            cookie: u64::from(range),
                            bytes: ticket.bytes,
                            ..model::Backing::default()
                        },
                    )
                    .expect("Original partial BO backing");
                let _ = table().drop_reference(ticket.reference, owner);
            }
            unlock();
            collect();
            return Err(Error::OutOfMemory);
        }
        committed += count;
    }
    lock();
    unsafe {
        table()
            .publish(
                ticket,
                model::Backing {
                    cookie: u64::from(range),
                    cpu_address: address,
                    bytes: ticket.bytes,
                    cache: 1,
                    ..model::Backing::default()
                },
            )
            .expect("Original BO backing");
    }
    let described = unsafe { read().describe(ticket.reference, owner) };
    unlock();
    described?;
    Ok(Created {
        buffer: ticket.buffer,
        reference: ticket.reference,
        address,
        bytes: descriptor.bytes,
    })
}
pub fn visibility() {
    unsafe {
        core::arch::asm!("mfence", options(nostack, preserves_flags));
    }
}
/// # Safety
/// Exact BO metadata owner is held; no external operation spans the view.
pub(crate) unsafe fn map_locked(
    reference: Handle,
    owner: Owner,
    access: u32,
    offset: u64,
    bytes: u64,
) -> Result<model::Use, Error> {
    let result = unsafe { table().use_resource(reference, owner, access, offset, bytes) }?;
    visibility();
    Ok(result)
}
/// # Safety
/// Exact BO metadata owner is held and the caller retains its CPU lease.
pub(crate) unsafe fn unmap_cpu_locked(lease: Handle, owner: Owner) -> Result<(), Error> {
    let value = unsafe { read().use_info(lease, owner) }?;
    if !types::is_cpu(value.access) {
        return Err(Error::Invalid);
    }
    visibility();
    unsafe { table().end_use(lease, owner, false) }
}
pub fn drop_reference(reference: Handle, owner: Owner) -> Result<(), Error> {
    let _call = Call::acquire()?;
    lock();
    let result = unsafe { table().drop_reference(reference, owner) };
    unlock();
    result?;
    collect();
    Ok(())
}
pub fn stopped(owner: Owner) {
    lock();
    visibility();
    unsafe {
        table().stopped_owner(owner);
    }
    unlock();
    collect();
}
pub fn collect() {
    let Ok(_call) = Call::acquire() else {
        return;
    };
    struct CollectMetadata;
    impl Drop for CollectMetadata {
        fn drop(&mut self) {
            collect_metadata();
        }
    }
    let _metadata = CollectMetadata;
    lock();
    let budget = unsafe { (*STATE.0.get()).pools[0].len };
    unlock();
    for _ in 0..budget {
        lock();
        let ticket = unsafe { table().pending_release(true) };
        unlock();
        let Some(ticket) = ticket else {
            return;
        };
        let released = vm_transactions::r4os_rust_vm_release(
            u32::try_from(ticket.backing.cookie).expect("Original VM range ID"),
        ) == 0;
        lock();
        unsafe {
            let _ = table().finish_release(ticket, released);
        }
        unlock();
        if !released {
            return;
        }
    }
}
unsafe fn empty_locked() -> bool {
    unsafe {
        if (*STATE.0.get()).committed_bytes != 0 {
            return false;
        }
        for kind in 0..3 {
            let len = (*STATE.0.get()).pools[kind].len;
            if read().free_slots(kind as u32, len) != len {
                return false;
            }
        }
        true
    }
}
fn collect_metadata() {
    unsafe {
        let s = STATE.0.get();
        for kind in 0..3 {
            lock();
            if (*s).growth[kind].busy {
                unlock();
                continue;
            }
            if (*s).growth[kind].retired.is_none() && empty_locked() {
                (*s).growth[kind].retired = (*s).growth[kind].owned;
                (*s).growth[kind].owned = None;
                (*s).pools[kind] = Pool::EMPTY;
            }
            if let Some(allocation) = (*s).growth[kind].retired {
                (*s).growth[kind].busy = true;
                unlock();
                let released = free(allocation);
                lock();
                if released {
                    (*s).growth[kind].retired = None;
                }
                (*s).growth[kind].busy = false;
                unlock();
            } else {
                unlock();
            }
        }
    }
}
pub fn retains_driver(id: u32) -> bool {
    lock();
    let retained = (|| {
        let r = unsafe { read() };
        for o in r.objects {
            if o.phase != model::EMPTY
                && o.owned_cookie != 0
                && o.producer.kind == 2
                && o.producer.id == u64::from(id)
            {
                return true;
            }
            if matches!(o.phase, model::RELEASING | model::DESTROYING)
                && o.producer.kind == 2
                && o.producer.id == u64::from(id)
            {
                return true;
            }
            if o.has_backing && o.backing.has_driver && o.backing.driver.id == u64::from(id) {
                return true;
            }
        }
        for l in r.leases {
            if l.handle.id != 0 && l.owner.kind == 2 && l.owner.id == u64::from(id) {
                return true;
            }
        }
        for reference in r.references {
            if reference.handle.id != 0
                && reference.owner.kind == 2
                && reference.owner.id == u64::from(id)
            {
                return true;
            }
        }
        false
    })();
    unlock();
    retained
}
pub fn pending_release_for_owner(owner: Owner) -> bool {
    lock();
    let pending = unsafe { read().pending_release_for_owner(owner) };
    unlock();
    pending
}
/// # Safety
/// Caller holds the original common metadata owner; Args and the operation's
/// output records are exact, resident and disjoint from all mutable metadata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_global_bo_run(
    op: u32,
    args: *const model::Args,
    out: *mut c_void,
) -> i32 {
    match unsafe { model::dispatch(view(), op, args.read(), out) } {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_buffers_mutex() -> *mut Mutex {
    mutex()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_buffers_lock(acquire: bool) {
    if acquire {
        lock();
    } else {
        unlock();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_gfx_buffers_visibility() {
    visibility();
}
/// # Safety
/// Args and each operation's exact input/output are initialized resident C
/// records. Callers of map/unmap already hold the shared BO metadata owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_gfx_buffers_run(
    op: u32,
    args: *const model::Args,
    out: *mut c_void,
) -> i32 {
    let args = unsafe { args.read() };
    let result = (|| -> Result<(), Error> {
        unsafe {
            match op {
                0 => out
                    .cast::<Created>()
                    .write(create(args.owner, args.input.cast::<Descriptor>().read())?),
                1 => drop_reference(args.handle, args.owner)?,
                2 => stopped(args.owner),
                3 => collect(),
                4 => out.cast::<bool>().write(retains_driver(
                    u32::try_from(args.value).expect("Original driver ID"),
                )),
                5 => out
                    .cast::<bool>()
                    .write(pending_release_for_owner(args.owner)),
                6 => lock_prepared(args.input.cast::<Need>().read())?,
                7 => out.cast::<model::Use>().write(map_locked(
                    args.handle,
                    args.owner,
                    args.access,
                    args.offset,
                    args.bytes,
                )?),
                8 => unmap_cpu_locked(args.handle, args.owner)?,
                _ => panic!("Invalid buffer runtime operation"),
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => 0,
        Err(e) => e as i32,
    }
}
const _: () = {
    assert!(size_of::<Need>() == 24);
    assert!(size_of::<Created>() == 48);
};
