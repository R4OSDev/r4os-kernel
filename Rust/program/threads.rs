//! Stable thread records, exact execution/input leases and native thread registry.
//! Original program-state owner admits mutation; no Rust record reference crosses
//! a wait, peer cleanup or task switch. Foreign readers borrow the same C records.
use super::{policy, registry, registry_state, registry_types::*, stack};
use crate::{
    contract::{
        ProgramJoinHandle as JoinHandle, ProgramProcessHandle as Handle, ProgramThreadInfo as Info,
    },
    memory::{heap, owner::Guard},
    platform::timer,
    sched::{
        scheduler, sync_objects,
        sync_types::WaitQueue,
        sync_wait,
        task::{task_api, task_registry, task_types::ExecutionOwnerKind},
        task_context,
    },
};
use core::{
    cell::UnsafeCell,
    mem::{self, MaybeUninit},
    ptr,
};
#[repr(C)]
pub struct OptionalLease {
    pub value: MaybeUninit<ProgramInstanceLease>,
    pub present: bool,
}
impl Default for OptionalLease {
    fn default() -> Self {
        Self {
            value: MaybeUninit::uninit(),
            present: false,
        }
    }
}
#[repr(C)]
pub struct Thread {
    pub console_input_lease: OptionalLease,
    pub registry_next: *mut Thread,
    pub join_queue: WaitQueue,
    pub created_tick: u64,
    pub arg: usize,
    pub join_owner_task_generation: u64,
    pub instance_generation: u64,
    pub entry: usize,
    pub stack: stack::Stack,
    pub task_generation: u64,
    pub generation: u64,
    pub owner_instance: *mut ProgramInstance,
    pub registry_prev: *mut Thread,
    pub finished_tick: u64,
    pub join_owner_task_id: u32,
    pub spawn_transaction_depth: u32,
    pub exit_code: i32,
    pub instance_id: u32,
    pub task_id: u32,
    pub id: u32,
    pub state: u32,
    pub flags: u32,
    pub join_count: u32,
    pub join_waiter_refs: u32,
    pub pin_count: u32,
    pub stack_released: bool,
    pub exit_deferred: bool,
    pub execution_pinned: bool,
    pub task_detached: bool,
    pub retire_for_instance: bool,
    pub retire_in_progress: bool,
    pub join_lease_active: bool,
    pub retire_pending: bool,
    pub used: bool,
    pub(super) _padding0: [u8; 3],
    // Only this exact pinned execution writes its resident kernel return
    // anchor. The fault path consumes it without touching the module stack.
    pub(super) fault_frame: *mut crate::arch::callback_abort::Frame,
    pub(super) stack_exhausted: bool,
}
impl Default for Thread {
    fn default() -> Self {
        Self {
            console_input_lease: OptionalLease::default(),
            registry_next: ptr::null_mut(),
            join_queue: WaitQueue::EMPTY,
            created_tick: 0,
            arg: 0,
            join_owner_task_generation: 0,
            instance_generation: 0,
            entry: 0,
            stack: stack::Stack::default(),
            task_generation: 0,
            generation: 0,
            owner_instance: ptr::null_mut(),
            registry_prev: ptr::null_mut(),
            finished_tick: 0,
            join_owner_task_id: 0,
            spawn_transaction_depth: 0,
            exit_code: 0,
            instance_id: 0,
            task_id: 0,
            id: 0,
            state: 0,
            flags: 0,
            join_count: 0,
            join_waiter_refs: 0,
            pin_count: 0,
            stack_released: false,
            exit_deferred: false,
            execution_pinned: false,
            task_detached: false,
            retire_for_instance: false,
            retire_in_progress: false,
            join_lease_active: false,
            retire_pending: false,
            used: false,
            _padding0: [0; 3],
            fault_frame: ptr::null_mut(),
            stack_exhausted: false,
        }
    }
}
const _: () = {
    assert!(mem::size_of::<Thread>() == 336);
    assert!(mem::align_of::<Thread>() == 8);
    assert!(mem::size_of::<OptionalLease>() == 40);
    assert!(mem::offset_of!(OptionalLease, present) == 32);
    assert!(mem::offset_of!(Thread, registry_prev) == 248);
    assert!(mem::offset_of!(Thread, registry_next) == 40);
    assert!(mem::offset_of!(Thread, used) == 316);
    assert!(mem::offset_of!(Thread, pin_count) == 304);
    assert!(mem::offset_of!(Thread, retire_pending) == 315);
    assert!(mem::offset_of!(Thread, retire_in_progress) == 313);
    assert!(mem::offset_of!(Thread, retire_for_instance) == 312);
    assert!(mem::offset_of!(Thread, task_detached) == 311);
    assert!(mem::offset_of!(Thread, stack_released) == 308);
    assert!(mem::offset_of!(Thread, id) == 284);
    assert!(mem::offset_of!(Thread, generation) == 232);
    assert!(mem::offset_of!(Thread, instance_id) == 276);
    assert!(mem::offset_of!(Thread, instance_generation) == 112);
    assert!(mem::offset_of!(Thread, execution_pinned) == 310);
    assert!(mem::offset_of!(Thread, fault_frame) == 320);
    assert!(mem::offset_of!(Thread, stack_exhausted) == 328);
    assert!(mem::offset_of!(Thread, console_input_lease) == 0);
    assert!(mem::offset_of!(Thread, spawn_transaction_depth) == 268);
    assert!(mem::offset_of!(Thread, exit_deferred) == 309);
    assert!(mem::offset_of!(Thread, owner_instance) == 240);
    assert!(mem::offset_of!(Thread, task_id) == 280);
    assert!(mem::offset_of!(Thread, task_generation) == 224);
    assert!(mem::offset_of!(Thread, state) == 288);
    assert!(mem::offset_of!(Thread, flags) == 292);
    assert!(mem::offset_of!(Thread, entry) == 120);
    assert!(mem::offset_of!(Thread, arg) == 96);
    assert!(mem::offset_of!(Thread, stack) == 128);
    assert!(mem::offset_of!(Thread, exit_code) == 272);
    assert!(mem::offset_of!(Thread, created_tick) == 88);
    assert!(mem::offset_of!(Thread, finished_tick) == 256);
    assert!(mem::offset_of!(Thread, join_count) == 296);
    assert!(mem::offset_of!(Thread, join_owner_task_id) == 264);
    assert!(mem::offset_of!(Thread, join_owner_task_generation) == 104);
    assert!(mem::offset_of!(Thread, join_lease_active) == 314);
    assert!(mem::offset_of!(Thread, join_waiter_refs) == 300);
    assert!(mem::offset_of!(Thread, join_queue) == 48);
};

pub(super) const MAIN: u32 = 1;
pub(super) const JOINABLE: u32 = 2;
pub(super) const JOINED: u32 = 4;
#[repr(C)]
pub struct State {
    pub head: *mut Thread,
    pub tail: *mut Thread,
    pub count: usize,
    pub peak: usize,
    pub epoch: u64,
    pub create_failures: u64,
    pub next_generation: u64,
    pub next_id: u32,
}
struct Storage(UnsafeCell<State>);
// SAFETY: existing program-state owner serializes mutations. Fault/crash paths
// borrow stable intrusive nodes only under their unchanged exception lifetime.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    head: ptr::null_mut(),
    tail: ptr::null_mut(),
    count: 0,
    peak: 0,
    epoch: 1,
    create_failures: 0,
    next_generation: 1,
    next_id: 1,
}));
pub(super) fn state() -> *mut State {
    STATE.0.get()
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_threads_state() -> *mut State {
    state()
}
pub(super) fn yield_task() {
    scheduler::r4os_rust_scheduler_control(8, 0, 0);
}
pub(super) fn exit_task() -> ! {
    scheduler::r4os_rust_scheduler_control(13, 0, 0);
    unreachable!("Terminal task switch returned")
}
pub(super) struct Operation(pub task_context::Unwind);
impl Operation {
    pub(super) fn enter() -> Option<Self> {
        let token = unsafe { task_context::enter() };
        if token.admitted() {
            Some(Self(token))
        } else {
            None
        }
    }
}
impl Drop for Operation {
    fn drop(&mut self) {
        let _ = unsafe { task_context::leave(self.0) };
    }
}
pub(super) unsafe fn queue(thread: *mut Thread, operation: u8) -> u64 {
    unsafe {
        sync_wait::r4os_rust_sync_queue(
            ptr::addr_of_mut!((*thread).join_queue),
            operation,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
        )
    }
}
pub(super) unsafe fn bump() {
    unsafe {
        (*state()).epoch = (*state()).epoch.wrapping_add(1);
        if (*state()).epoch == 0 {
            (*state()).epoch = 1;
        }
    }
}
pub(super) unsafe fn by_id(id: u32) -> *mut Thread {
    if id == 0 {
        return ptr::null_mut();
    }
    unsafe {
        let mut cursor = (*state()).head;
        while !cursor.is_null() {
            if (*cursor).used && (*cursor).id == id {
                return cursor;
            }
            cursor = (*cursor).registry_next;
        }
    }
    ptr::null_mut()
}
pub(super) unsafe fn by_handle(handle: JoinHandle) -> *mut Thread {
    unsafe {
        let thread = by_id(handle.thread_id);
        if thread.is_null()
            || (*thread).instance_id != handle.instance_id
            || (*thread).instance_generation != handle.instance_generation
            || (*thread).generation != handle.thread_generation
        {
            ptr::null_mut()
        } else {
            thread
        }
    }
}
pub(super) unsafe fn contains(wanted: *const Thread) -> bool {
    unsafe {
        let mut cursor = (*state()).head;
        while !cursor.is_null() {
            if ptr::eq(cursor, wanted) {
                return true;
            }
            cursor = (*cursor).registry_next;
        }
    }
    false
}
pub(super) unsafe fn allocate_id() -> Option<u32> {
    unsafe {
        let mut candidate = if (*state()).next_id == 0 {
            1
        } else {
            (*state()).next_id
        };
        let start = candidate;
        loop {
            (*state()).next_id = candidate.wrapping_add(1);
            if (*state()).next_id == 0 {
                (*state()).next_id = 1;
            }
            if by_id(candidate).is_null() {
                return Some(candidate);
            }
            candidate = (*state()).next_id;
            if candidate == start {
                return None;
            }
        }
    }
}
pub(super) unsafe fn allocate_generation() -> Option<u64> {
    unsafe {
        let generation = (*state()).next_generation;
        if generation == 0 || generation == u64::MAX {
            return None;
        }
        (*state()).next_generation += 1;
        Some(generation)
    }
}
pub(super) unsafe fn link(thread: *mut Thread) {
    unsafe {
        (*thread).registry_prev = (*state()).tail;
        (*thread).registry_next = ptr::null_mut();
        if !(*state()).tail.is_null() {
            (*(*state()).tail).registry_next = thread;
        } else {
            (*state()).head = thread;
        }
        (*state()).tail = thread;
        (*state()).count += 1;
        if (*state()).count > (*state()).peak {
            (*state()).peak = (*state()).count;
        }
        bump();
    }
}
pub(super) unsafe fn unlink(thread: *mut Thread) {
    unsafe {
        if !(*thread).registry_prev.is_null() {
            (*(*thread).registry_prev).registry_next = (*thread).registry_next;
        } else {
            (*state()).head = (*thread).registry_next;
        }
        if !(*thread).registry_next.is_null() {
            (*(*thread).registry_next).registry_prev = (*thread).registry_prev;
        } else {
            (*state()).tail = (*thread).registry_prev;
        }
        (*thread).registry_prev = ptr::null_mut();
        (*thread).registry_next = ptr::null_mut();
        if (*state()).count != 0 {
            (*state()).count -= 1;
        }
        bump();
    }
}
pub(super) unsafe fn publish(thread: *mut Thread) -> bool {
    let _guard = Guard::acquire(0);
    unsafe {
        let Some(id) = allocate_id() else {
            return false;
        };
        let Some(generation) = allocate_generation() else {
            return false;
        };
        (*thread).id = id;
        (*thread).generation = generation;
        link(thread);
        true
    }
}
pub(super) fn allocate() -> *mut Thread {
    let memory = heap::r4os_rust_heap_alloc(mem::size_of::<Thread>(), mem::align_of::<Thread>());
    if memory.is_null() {
        return ptr::null_mut();
    }
    let thread = memory.cast::<Thread>();
    unsafe { thread.write(Thread::default()) };
    thread
}
pub(super) unsafe fn free(thread: *mut Thread) -> bool {
    if unsafe { heap::r4os_rust_heap_free(thread.cast(), mem::size_of::<Thread>()) } != 0 {
        crate::boot::foundation::puts(b"ProgramThread object release failed\r\n");
        return false;
    }
    true
}
pub(super) unsafe fn current() -> *mut Thread {
    unsafe {
        let task = scheduler::current();
        if task.is_null() {
            return ptr::null_mut();
        }
        let core = task_registry::core(task.cast());
        if (*core).execution_owner_kind != ExecutionOwnerKind::ProgramThread {
            return ptr::null_mut();
        }
        let thread = (*core).execution_owner_context.cast::<Thread>();
        if thread.is_null()
            || !(*thread).used
            || (*thread).task_id != (*core).id
            || (*thread).task_generation != (*core).generation
        {
            return ptr::null_mut();
        }
        thread
    }
}
pub(super) unsafe fn current_ready() -> *mut Thread {
    for _ in 0..64 {
        let thread = unsafe { current() };
        if !thread.is_null() {
            return thread;
        }
        yield_task();
    }
    ptr::null_mut()
}
pub(super) unsafe fn pin(id: u32, exact: Option<JoinHandle>, allow_retiring: bool) -> *mut Thread {
    let _guard = Guard::acquire(0);
    unsafe {
        let thread = if let Some(handle) = exact {
            by_handle(handle)
        } else {
            by_id(id)
        };
        if thread.is_null()
            || !(*thread).used
            || (!allow_retiring && ((*thread).retire_pending || (*thread).retire_in_progress))
            || (*thread).pin_count == u32::MAX
        {
            return ptr::null_mut();
        }
        (*thread).pin_count += 1;
        thread
    }
}
pub(super) unsafe fn unpin(thread: *mut Thread) -> bool {
    let _guard = Guard::acquire(0);
    unsafe {
        if !contains(thread) || (*thread).pin_count == 0 {
            return false;
        }
        (*thread).pin_count -= 1;
        true
    }
}
pub(super) unsafe fn mark(thread: *mut Thread, running: bool, code: i32) {
    let guard = Guard::acquire(0);
    unsafe {
        if running {
            if contains(thread) && (*thread).used && (*thread).state == 1 {
                (*thread).state = 2;
                bump();
            }
            return;
        }
        if !contains(thread) || !(*thread).used || matches!((*thread).state, 3 | 4) {
            return;
        }
        (*thread).exit_code = code;
        (*thread).state = 3;
        (*thread).finished_tick = timer::tick_count();
        bump();
        drop(guard);
        queue(thread, 1);
    }
}
pub(super) unsafe fn pin_execution(thread: *mut Thread) -> *mut ProgramInstance {
    unsafe {
        if (*thread).execution_pinned {
            return (*thread).owner_instance;
        }
        let h = Handle {
            instance_id: (*thread).instance_id,
            generation: (*thread).instance_generation,
            ..Handle::default()
        };
        let mut lease = MaybeUninit::<ProgramInstanceLease>::uninit();
        if !registry::r4os_rust_program_registry_pin(h, false, lease.as_mut_ptr()) {
            return ptr::null_mut();
        }
        let lease = lease.assume_init();
        (*thread).execution_pinned = true;
        lease.instance
    }
}
pub(super) unsafe fn unpin_execution(thread: *mut Thread) {
    unsafe {
        if !(*thread).execution_pinned || !registry::lock() {
            return;
        }
        let h = Handle {
            instance_id: (*thread).instance_id,
            generation: (*thread).instance_generation,
            ..Handle::default()
        };
        let slot = registry::lookup_handle(h, true);
        let mut wake = false;
        if !slot.is_null() && (*slot).pin_count != 0 {
            (*slot).pin_count -= 1;
            (*thread).execution_pinned = false;
            wake = (*slot).pin_count == 0 && (*slot).state == 6;
        } else {
            let stats = (*registry_state::state()).stats;
            (*stats).stale_lease_rejections = (*stats).stale_lease_rejections.wrapping_add(1);
        }
        registry::unlock();
        if wake {
            sync_objects::r4os_rust_sync_event((*registry_state::state()).event, 0, 0);
        }
    }
}
pub(super) unsafe fn pin_input(
    thread: *mut Thread,
    h: Handle,
    out: *mut ProgramInstanceLease,
) -> bool {
    let Some(_operation) = Operation::enter() else {
        return false;
    };
    unsafe {
        if (*thread).console_input_lease.present {
            return false;
        }
        let mut lease = MaybeUninit::<ProgramInstanceLease>::uninit();
        if !registry::r4os_rust_program_registry_pin(h, false, lease.as_mut_ptr()) {
            return false;
        }
        ptr::copy_nonoverlapping(
            lease.as_ptr(),
            (*thread).console_input_lease.value.as_mut_ptr(),
            1,
        );
        (*thread).console_input_lease.present = true;
        out.write(lease.assume_init());
        true
    }
}
pub(super) unsafe fn release_input(thread: *mut Thread) -> bool {
    let Some(_operation) = Operation::enter() else {
        return false;
    };
    if !registry::lock() {
        return false;
    }
    unsafe {
        if !(*thread).console_input_lease.present {
            registry::unlock();
            return true;
        }
        let lease = (*thread).console_input_lease.value.assume_init_read();
        let slot = registry::lookup_handle(
            Handle {
                instance_id: lease.id,
                generation: lease.generation,
                ..Handle::default()
            },
            true,
        );
        if slot.is_null()
            || slot != lease.slot
            || ptr::addr_of_mut!((*slot).instance) != lease.instance
            || (*slot).pin_count == 0
        {
            let stats = (*registry_state::state()).stats;
            (*stats).stale_lease_rejections = (*stats).stale_lease_rejections.wrapping_add(1);
            registry::unlock();
            return false;
        }
        (*slot).pin_count -= 1;
        (*thread).console_input_lease.present = false;
        let wake = (*slot).pin_count == 0 && (*slot).state == 6;
        registry::unlock();
        if wake {
            sync_objects::r4os_rust_sync_event((*registry_state::state()).event, 0, 0);
        }
        true
    }
}
pub(super) unsafe fn natural_epilogue(thread: *const Thread) -> bool {
    unsafe {
        let present = (*thread).task_id != 0
            && (*thread).task_generation != 0
            && task_api::r4os_rust_task_control(
                20,
                u64::from((*thread).task_id),
                (*thread).task_generation,
            ) != 0;
        policy::r4os_rust_program_natural_exit(
            (*thread).state == 3,
            (*thread).execution_pinned,
            present,
        )
    }
}
pub(super) unsafe fn info(thread: *const Thread) -> Info {
    unsafe {
        let mut stack = ptr::addr_of!((*thread).stack).read();
        if (*thread).flags & MAIN != 0 && !(*thread).owner_instance.is_null() {
            let instance = (*thread).owner_instance;
            if (*instance).id == (*thread).instance_id {
                stack = stack::from_instance(instance).unwrap_or_default();
            }
        }
        Info {
            version: 1,
            size: mem::size_of::<Info>() as u32,
            thread_id: (*thread).id,
            instance_id: (*thread).instance_id,
            task_id: (*thread).task_id,
            state: (*thread).state,
            flags: (*thread).flags,
            exit_code: (*thread).exit_code,
            stack_base: stack.base,
            stack_reserved_bytes: stack.reserve_size,
            stack_committed_bytes: stack.committed_size,
            stack_guard_base: stack.guard_base,
            created_tick: (*thread).created_tick,
            finished_tick: (*thread).finished_tick,
            join_count: (*thread).join_count,
            reserved0: 0,
        }
    }
}
pub(super) fn record_failure() {
    let _guard = Guard::acquire(0);
    unsafe {
        (*state()).create_failures = (*state()).create_failures.wrapping_add(1);
    }
}
/// # Safety
/// 0..=3 retain caller's program-state admission; lookup results keep original
/// stable-thread lifetime. 4=current and5=current-ready acquire no registry pin.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_lookup(
    op: u8,
    id: u32,
    handle: JoinHandle,
    wanted: *const Thread,
) -> *mut Thread {
    unsafe {
        match op {
            0 => by_id(id),
            1 => by_handle(handle),
            2 => {
                if contains(wanted) {
                    wanted.cast_mut()
                } else {
                    ptr::null_mut()
                }
            }
            4 => current(),
            5 => current_ready(),
            _ => unreachable!(),
        }
    }
}
/// # Safety
/// Output is exclusive C scalar; caller retains original program-state owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_identity(
    generation: bool,
    out: *mut u64,
) -> bool {
    let value = unsafe {
        if generation {
            allocate_generation()
        } else {
            allocate_id().map(u64::from)
        }
    };
    if let Some(value) = value {
        unsafe { out.write(value) };
        true
    } else {
        false
    }
}
/// # Safety
/// Original thread lifetime. Ops0/1 require program-state owner (link/unlink);
/// 2=publish,3=running,4=done,5=unpin,6=free,7=natural epilogue,8=release input.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_control(
    op: u8,
    thread: *mut Thread,
    code: i32,
) -> bool {
    unsafe {
        match op {
            0 => {
                link(thread);
                true
            }
            1 => {
                unlink(thread);
                true
            }
            2 => publish(thread),
            3 => {
                mark(thread, true, 0);
                true
            }
            4 => {
                mark(thread, false, code);
                true
            }
            5 => unpin(thread),
            6 => free(thread),
            7 => natural_epilogue(thread),
            8 => release_input(thread),
            _ => unreachable!(),
        }
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_program_thread_allocate() -> *mut Thread {
    allocate()
}
/// # Safety
/// If locked, caller already retains original program-state owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_epoch(locked: bool) {
    if locked {
        unsafe { bump() };
    } else {
        let _guard = Guard::acquire(0);
        unsafe { bump() };
    }
}
/// # Safety
/// Original owner lifetime; exact=false selects id, exact=true the value handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_pin(
    id: u32,
    handle: JoinHandle,
    exact: bool,
    retiring: bool,
) -> *mut Thread {
    unsafe { pin(id, exact.then_some(handle), retiring) }
}
/// # Safety
/// Exact executing/retiring thread, with its unchanged caller lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_execution(
    thread: *mut Thread,
    release: bool,
) -> *mut ProgramInstance {
    unsafe {
        if release {
            unpin_execution(thread);
            ptr::null_mut()
        } else {
            pin_execution(thread)
        }
    }
}
/// # Safety
/// Exact current thread and exclusive lease output; pin is retained by thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_input(
    thread: *mut Thread,
    handle: Handle,
    out: *mut ProgramInstanceLease,
) -> bool {
    unsafe { pin_input(thread, handle, out) }
}
/// # Safety
/// Resident pinned/exact thread and exclusive canonical output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_info(thread: *const Thread, out: *mut Info) {
    unsafe { out.write(info(thread)) };
}
const _: () = {
    assert!(mem::size_of::<State>() == 64);
    assert!(mem::align_of::<State>() == 8);
    assert!(mem::offset_of!(State, head) == 0);
    assert!(mem::offset_of!(State, tail) == 8);
    assert!(mem::offset_of!(State, count) == 16);
    assert!(mem::offset_of!(State, peak) == 24);
    assert!(mem::offset_of!(State, epoch) == 32);
    assert!(mem::offset_of!(State, create_failures) == 40);
    assert!(mem::offset_of!(State, next_generation) == 48);
    assert!(mem::offset_of!(State, next_id) == 56);
};
/// # Safety
/// Original current-execution lifetime; 0=current public thread ID,1=spawn cancel.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_program_thread_scalar(operation: u8) -> u64 {
    let thread = unsafe { current() };
    if thread.is_null() {
        return 0;
    }
    unsafe {
        match operation {
            0 => u64::from((*thread).id),
            1 => u64::from((*thread).exit_deferred),
            _ => unreachable!(),
        }
    }
}
