//! Native R4D execution admission. The wait-spanning lifecycle guard is
//! separate from scheduler runtime metadata; exact storage task owners never
//! borrow the interrupted task's dedicated-thread admission.
use crate::{
    arch::runtime::RuntimeGuard,
    contract as a,
    driver::{heap, irq_router, resource_state, resources, semaphores, threads, work},
    sched::{
        scheduler, sync_locks,
        sync_types::{Text, UnwindGuard, WaitQueue},
        task::task_registry as registry,
    },
};
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Callback {
    used: bool,
    task_id: u32,
    owner: u32,
    depth: u32,
}
#[repr(C)]
pub struct CallbackToken {
    pub slot: usize,
    pub task_id: u32,
}
struct Storage {
    owner: AtomicU32,
    guard: UnsafeCell<UnwindGuard>,
    callbacks: UnsafeCell<[Callback; 8]>,
}
// SAFETY: the exact lifecycle guard serializes owner writes; its atomic scalar
// also permits AP admission reads. Callback slots use short runtime admission;
// neither metadata boundary aliases I/O.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage {
    owner: AtomicU32::new(0),
    guard: UnsafeCell::new(UnwindGuard {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: Text::literal(b"r4d-owner"),
        queue: WaitQueue::EMPTY,
    }),
    callbacks: UnsafeCell::new(
        [Callback {
            used: false,
            task_id: 0,
            owner: 0,
            depth: 0,
        }; 8],
    ),
};
fn owned() -> bool {
    unsafe { sync_locks::r4os_rust_sync_guard(STATE.guard.get(), 0, 0) }
}
fn owner() -> u32 {
    STATE.owner.load(Ordering::Relaxed)
}
fn current_task_id() -> u32 {
    let task = scheduler::current();
    if task.is_null() {
        0
    } else {
        unsafe { (*registry::core(task.cast())).id }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_api_enter(id: u32, ticks: u64) -> bool {
    if id == 0 {
        return false;
    }
    if !unsafe { sync_locks::r4os_rust_sync_guard(STATE.guard.get(), 2, ticks) } {
        return false;
    }
    if owner() != 0 && owner() != id {
        unsafe { sync_locks::r4os_rust_sync_guard(STATE.guard.get(), 3, 0) };
        return false;
    }
    STATE.owner.store(id, Ordering::Relaxed);
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_api_leave() -> bool {
    if !owned() {
        return false;
    }
    if unsafe { (*STATE.guard.get()).depth == 1 } {
        STATE.owner.store(0, Ordering::Relaxed);
    }
    unsafe { sync_locks::r4os_rust_sync_guard(STATE.guard.get(), 3, 0) }
}
// Scalar temporary facade reads; no mutable guard or canonical owner escapes.
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_api_owner_scalar(op: u8) -> u32 {
    match op {
        0 => owner(),
        1 => u32::from(owned()),
        2 => {
            if owned() {
                owner()
            } else {
                0
            }
        }
        _ => unreachable!(),
    }
}
/// # Safety
/// Resident C records and callbacks keep their exact driver/task lifetime under
/// the original lifecycle or storage callback admission until the call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_api_storage_enter(
    id: u32,
    out: *mut CallbackToken,
) -> bool {
    if id == 0 {
        return false;
    }
    let _g = RuntimeGuard::acquire();
    let task_id = current_task_id();
    let callbacks = unsafe { &mut *STATE.callbacks.get() };
    let mut free = None;
    for (i, e) in callbacks.iter_mut().enumerate() {
        if !e.used {
            free = free.or(Some(i));
            continue;
        }
        if e.task_id != task_id {
            continue;
        }
        if e.owner != id || e.depth == u32::MAX {
            return false;
        }
        e.depth += 1;
        unsafe { ptr::write(out, CallbackToken { slot: i, task_id }) };
        return true;
    }
    let Some(slot) = free else { return false };
    callbacks[slot] = Callback {
        used: true,
        task_id,
        owner: id,
        depth: 1,
    };
    unsafe { ptr::write(out, CallbackToken { slot, task_id }) };
    true
}
/// # Safety
/// Resident C records and callbacks keep their exact driver/task lifetime under
/// the original lifecycle or storage callback admission until the call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_api_storage_leave(token: *const CallbackToken) {
    let _g = RuntimeGuard::acquire();
    let t = unsafe { ptr::read(token) };
    if t.slot >= 8 {
        return;
    }
    let e = unsafe { &mut (*STATE.callbacks.get())[t.slot] };
    if !e.used || e.task_id != t.task_id || e.depth == 0 {
        return;
    }
    e.depth -= 1;
    if e.depth == 0 {
        *e = Callback::default()
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_api_storage_owner() -> u32 {
    let _g = RuntimeGuard::acquire();
    let task_id = current_task_id();
    for e in unsafe { &*STATE.callbacks.get() } {
        if e.used && e.task_id == task_id && e.depth != 0 {
            return e.owner;
        }
    }
    0
}
fn active() -> u32 {
    let irq = irq_router::r4os_rust_irq_router_local(true);
    if irq != 0 {
        return irq;
    }
    let storage = r4os_rust_driver_api_storage_owner();
    if storage != 0 {
        return storage;
    }
    let work = work::r4os_rust_driver_work_current();
    if work != 0 {
        return work;
    }
    if owner() != 0 && owned() { owner() } else { 0 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_api_runtime_owner(runtime: bool) -> u32 {
    if !runtime {
        return active();
    }
    if irq_router::r4os_rust_irq_router_local(false) != 0 {
        return irq_router::r4os_rust_irq_router_local(true);
    }
    let storage = r4os_rust_driver_api_storage_owner();
    if storage != 0 {
        return storage;
    }
    let thread = threads::r4os_rust_driver_thread_current(0, 0) as u32;
    if thread != 0 { thread } else { active() }
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_api_bind(id: u32, slot: usize) -> bool {
    if owner() != id || !owned() {
        return false;
    }
    let mut generation = 0;
    if !unsafe { driver_module_identity(slot, &mut generation) } {
        return false;
    }
    let state = unsafe { resources::r4os_rust_driver_resource_state() };
    if unsafe { resource_state::r4os_rust_resource_state_mutate(state, 0, id, slot, generation) }
        != 0
    {
        return false;
    }
    let mut binding = ptr::null();
    assert_eq!(
        unsafe { resource_state::r4os_rust_resource_current(state, id, &mut binding) },
        0
    );
    let epoch = unsafe { (*binding).epoch };
    let finish = || {
        unsafe { resource_state::r4os_rust_resource_state_mutate(state, 2, id, 0, 0) };
    };
    if !threads::r4os_rust_driver_thread_bind(id, epoch) {
        finish();
        return false;
    }
    if !semaphores::r4os_rust_driver_semaphore_bind(id, epoch) {
        assert!(threads::r4os_rust_driver_thread_discard(id, epoch));
        finish();
        return false;
    }
    if !{ crate::gfx::driver_memory::bind(id, epoch) } {
        assert!(semaphores::r4os_rust_driver_semaphore_discard(id, epoch));
        assert!(threads::r4os_rust_driver_thread_discard(id, epoch));
        finish();
        return false;
    }
    if !heap::r4os_rust_driver_heap_bind(id, epoch) {
        assert!({ crate::gfx::driver_memory::discard_empty_binding(id, epoch) });
        assert!(semaphores::r4os_rust_driver_semaphore_discard(id, epoch));
        assert!(threads::r4os_rust_driver_thread_discard(id, epoch));
        finish();
        return false;
    }
    true
}
/// # Safety
/// Resident C records and callbacks keep their exact driver/task lifetime under
/// the original lifecycle or storage callback admission until the call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_api_work_submit(
    handler: work::Handler,
    context: usize,
    flags: u32,
    out: *mut u32,
) -> i32 {
    if irq_router::r4os_rust_irq_router_local(false) == 0
        && r4os_rust_driver_api_storage_owner() == 0
        && threads::r4os_rust_driver_thread_current(0, 0) != 0
    {
        unsafe { *out = 0 };
        if flags != 0 {
            return -3;
        }
        let id = threads::r4os_rust_driver_thread_current(1, 0) as u32;
        if id == 0 {
            return a::driver_thread_error_closed;
        }
        return unsafe {
            work::r4os_rust_driver_work_submit(id, handler, context, flags, false, out)
        };
    }
    unsafe { work::r4os_rust_driver_work_submit(active(), handler, context, flags, false, out) }
}
/// # Safety
/// Resident C records and callbacks keep their exact driver/task lifetime under
/// the original lifecycle or storage callback admission until the call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_api_work_owned(
    handler: work::Handler,
    context: usize,
    out: *mut u32,
) -> i32 {
    if out.is_null() {
        return a::driver_thread_error_invalid;
    }
    unsafe { *out = 0 };
    if irq_router::r4os_rust_irq_router_local(false) != 0
        || r4os_rust_driver_api_storage_owner() != 0
    {
        return a::driver_thread_error_owner;
    }
    let thread = threads::r4os_rust_driver_thread_current(0, 0) as u32;
    let id = if thread != 0 {
        threads::r4os_rust_driver_thread_current(1, 0) as u32
    } else {
        active()
    };
    if id == 0 {
        return a::driver_thread_error_closed;
    }
    unsafe { work::r4os_rust_driver_work_submit(id, handler, context, 0, true, out) }
}
/// # Safety
/// Resident C records and callbacks keep their exact driver/task lifetime under
/// the original lifecycle or storage callback admission until the call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_api_invoke_owned(
    id: u32,
    handler: work::Handler,
    context: usize,
) -> i32 {
    if id == 0
        || work::r4os_rust_driver_work_current() != id
        || threads::r4os_rust_driver_thread_current(0, 0) != 0
        || irq_router::r4os_rust_irq_router_local(false) != 0
        || r4os_rust_driver_api_storage_owner() != 0
    {
        return a::driver_thread_error_owner;
    }
    if !r4os_rust_driver_api_enter(id, 0) {
        return a::driver_work_owner_busy;
    }
    let admission = unsafe { driver_owned_buffer_admit() };
    if admission != 0 {
        r4os_rust_driver_api_leave();
        return admission;
    }
    let result = unsafe { handler(context) };
    r4os_rust_driver_api_leave();
    result
}

struct TableStorage {
    value: UnsafeCell<MaybeUninit<a::DriverApi>>,
    initialized: UnsafeCell<bool>,
}
// SAFETY: bootstrap/lifecycle owner initializes once from the fixed stateless
// C callback template; its canonical table stays resident for every R4D call.
unsafe impl Sync for TableStorage {}
static TABLE: TableStorage = TableStorage {
    value: UnsafeCell::new(MaybeUninit::uninit()),
    initialized: UnsafeCell::new(false),
};
/// # Safety
/// Fixed C callback template is valid under bootstrap/lifecycle admission.
pub(crate) fn table() -> *mut a::DriverApi {
    let value = TABLE.value.get().cast::<a::DriverApi>();
    if !unsafe { *TABLE.initialized.get() } {
        let input = super::api_callbacks::template();
        assert!(
            input.magic == 0x31495044
                && input.version == a::driver_api_version
                && input.size == 656
        );
        unsafe {
            ptr::write(value, input);
            *TABLE.initialized.get() = true;
        }
    }
    value
}

unsafe fn driver_module_identity(slot: usize, generation: *mut u32) -> bool {
    let entry = crate::loader::registry_control::r4os_rust_mod_entry(slot);
    if entry.is_null() {
        return false;
    }
    unsafe {
        if (*entry).kind != 3 {
            return false;
        }
        generation.write((*entry).generation);
    }
    true
}

unsafe fn driver_owned_buffer_admit() -> i32 {
    match crate::driver::api_gfx::identity(0, true) {
        Ok(_) => 0,
        Err(code) => code,
    }
}
