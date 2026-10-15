//! Original controller-local storage workers. Task identities are immutable
//! generation-bound snapshots; no storage guard crosses creation/wait/wake.
use crate::{
    memory::owner::Guard,
    sched::{scheduler, sync_objects, sync_types::Event, task},
    storage::{block_accounting as accounting, block_registry},
};
use core::{cell::UnsafeCell, ffi::c_void, ptr};
const NO_LANE: u8 = 255;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Identity {
    pub started: bool,
    pub id: u32,
    pub generation: u64,
}
struct Controller {
    identity: Identity,
    event: Event,
}
impl Controller {
    const EMPTY: Self = Self {
        identity: Identity {
            started: false,
            id: 0,
            generation: 0,
        },
        event: Event::AUTO,
    };
}
struct State {
    primary: Identity,
    primary_lane: u8,
    controllers: [Controller; 8],
}
struct Workers(UnsafeCell<State>);
// Short storage admission protects metadata; event enrollment/signaling has
// the existing native synchronization owner and stable resident addresses.
unsafe impl Sync for Workers {}
static WORKERS: Workers = Workers(UnsafeCell::new(State {
    primary: Identity {
        started: false,
        id: 0,
        generation: 0,
    },
    primary_lane: NO_LANE,
    controllers: [const { Controller::EMPTY }; 8],
}));
#[repr(C)]
struct Create {
    name: *const u8,
    name_len: usize,
    state: u8,
    role: u8,
    needs_fpu: bool,
    smp_eligible: bool,
    smp_r4x: bool,
    owner_kind: u8,
    owner_context: *mut c_void,
    entry: Option<unsafe extern "C" fn()>,
}
unsafe extern "C" {
    // Same native owner interfaces used by the existing Zig task facade.
    fn r4os_rust_task_create(input: *const Create, critical: bool, failure: *mut u8)
    -> *mut c_void;
    fn r4os_rust_task_control(operation: u8, a: u64, b: u64) -> u64;
    // Temporary call into the not-yet-migrated block execution leaf; this
    // does not own worker identities, events, accounting or lane selection.

}
fn primary() -> Identity {
    let _owner = Guard::acquire(1);
    unsafe { (*WORKERS.0.get()).primary }
}
fn controller(lane: u8) -> Identity {
    if lane >= 8 {
        return Identity::default();
    }
    let _owner = Guard::acquire(1);
    unsafe { (*WORKERS.0.get()).controllers[usize::from(lane)].identity }
}
fn primary_lane() -> u8 {
    let _owner = Guard::acquire(1);
    unsafe { (*WORKERS.0.get()).primary_lane }
}
fn identity_alive(value: Identity) -> bool {
    value.started
        && value.id != 0
        && value.generation != 0
        && unsafe { r4os_rust_task_control(19, u64::from(value.id), value.generation) } != 0
}
fn event(lane: u8) -> *mut Event {
    assert!(lane < 8);
    unsafe { ptr::addr_of_mut!((*WORKERS.0.get()).controllers[usize::from(lane)].event) }
}
/// # Safety
/// Exclusive BSP boot reset before any live worker or event waiter exists.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_workers_init() {
    let state = WORKERS.0.get();
    unsafe {
        ptr::addr_of_mut!((*state).primary).write(Identity::default());
        (*state).primary_lane = NO_LANE;
        for index in 0..8 {
            ptr::addr_of_mut!((*state).controllers[index]).write(Controller::EMPTY);
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_workers_alive() -> bool {
    identity_alive(primary())
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_workers_ready() -> bool {
    !scheduler::current().is_null() && r4os_rust_block_workers_alive()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_worker_alive(lane: u8) -> bool {
    identity_alive(controller(lane))
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_worker_start(lane: u8, critical: bool) -> bool {
    if lane >= 8 {
        return false;
    }
    if r4os_rust_block_worker_alive(lane) {
        return true;
    }
    const NAMES: [&[u8]; 8] = [
        b"block-work",
        b"block-work1",
        b"block-work2",
        b"block-work3",
        b"block-work4",
        b"block-work5",
        b"block-work6",
        b"block-work7",
    ];
    let name = NAMES[usize::from(lane)];
    let input = Create {
        name: name.as_ptr(),
        name_len: name.len(),
        state: 1,
        role: 3,
        needs_fpu: true,
        smp_eligible: false,
        smp_r4x: false,
        owner_kind: 0,
        owner_context: ptr::null_mut(),
        entry: Some(worker_main),
    };
    let mut failure = 0;
    let created = unsafe { r4os_rust_task_create(&input, critical, &mut failure) };
    if created.is_null() {
        return false;
    }
    // New ready task cannot end until the shared entry discovers its exact
    // published lane. Read only immutable task identity, then publish it.
    let (id, generation) = unsafe { task::immutable_identity(created) };
    let _owner = Guard::acquire(1);
    unsafe {
        (*WORKERS.0.get()).controllers[usize::from(lane)].identity = Identity {
            started: true,
            id,
            generation,
        };
    }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_workers_start() -> bool {
    if r4os_rust_block_workers_alive() {
        return true;
    }
    // Retains the original boot initialization boundary: this call follows
    // boot device registration and precedes productive filesystem requests.
    unsafe {
        r4os_rust_block_workers_init();
    }
    for lane in 0..8u8 {
        if block_registry::r4os_rust_block_registry_query(4, lane) == 0 {
            continue;
        }
        if primary_lane() == NO_LANE {
            if !r4os_rust_block_worker_start(lane, true) {
                accounting::r4os_rust_block_runtime_change(0, 0, 0);
                accounting::r4os_rust_block_runtime_change(0, 1, 0);
                return false;
            }
            let _owner = Guard::acquire(1);
            unsafe {
                (*WORKERS.0.get()).primary_lane = lane;
            }
        } else if !r4os_rust_block_worker_start(lane, false) {
            accounting::r4os_rust_block_runtime_change(1, 13, 1);
        }
    }
    let lane = primary_lane();
    if lane == NO_LANE {
        accounting::r4os_rust_block_runtime_change(0, 0, 0);
        accounting::r4os_rust_block_runtime_change(0, 1, 0);
        return false;
    }
    let identity = controller(lane);
    {
        let _owner = Guard::acquire(1);
        unsafe {
            (*WORKERS.0.get()).primary = identity;
        }
    }
    accounting::r4os_rust_block_runtime_change(0, 0, 1);
    accounting::r4os_rust_block_runtime_change(0, 1, u64::from(identity.id));
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_worker_lane(lane: u8) -> u8 {
    if r4os_rust_block_worker_alive(lane) {
        return lane;
    }
    let primary = primary_lane();
    if r4os_rust_block_worker_alive(primary) {
        return primary;
    }
    NO_LANE
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_worker_signal(lane: u8) {
    // Caller already chose the exact effective lane before releasing its
    // admission owner. Do not reinterpret it and switch the execution owner.
    if lane < 8 {
        unsafe {
            sync_objects::r4os_rust_sync_event(event(lane), 0, 0);
        }
    }
}
/// # Safety
/// Output is one exclusive aligned C RuntimeSummary; only copied identities
/// remain through task-registry liveness checks; no worker/registry loan escapes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_workers_summary(out: *mut accounting::RuntimeSummary) {
    unsafe {
        accounting::r4os_rust_block_runtime_snapshot(out);
    }
    let primary = primary();
    let alive = identity_alive(primary);
    unsafe {
        (*out).worker_started = u32::from(alive);
        (*out).worker_task_id = if alive { primary.id } else { 0 };
        (*out).controller_count = block_registry::r4os_rust_block_registry_query(3, 0) as u32;
        (*out).worker_count = 0;
    }
    for lane in 0..8u8 {
        if block_registry::r4os_rust_block_registry_query(4, lane) != 0
            && r4os_rust_block_worker_alive(lane)
        {
            unsafe {
                (*out).worker_count += 1;
            }
        }
    }
}
fn current_lane() -> u8 {
    let current = scheduler::current();
    if current.is_null() {
        return NO_LANE;
    }
    let (id, generation) = unsafe { task::immutable_identity(current) };
    for lane in 0..8u8 {
        let identity = controller(lane);
        if identity.started && identity.id == id && identity.generation == generation {
            return lane;
        }
    }
    NO_LANE
}
unsafe extern "C" fn worker_main() {
    let mut lane = current_lane();
    while lane == NO_LANE {
        scheduler::yield_task();
        lane = current_lane();
    }
    loop {
        if unsafe { crate::storage::block_execution::pump_controller(lane) } {
            continue;
        }
        accounting::r4os_rust_block_runtime_change(1, 4, 1);
        unsafe {
            sync_objects::r4os_rust_sync_event(event(lane), 2, u64::MAX);
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<Create>() == 40);
    assert!(core::mem::size_of::<Identity>() == 16);
};
