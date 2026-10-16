//! Task resource construction and exact failed-release anchors. Not integrated
//! yet. Only scalar VM calls cross the temporary owner boundary. Runtime flags
//! are explicit around metadata changes, never held over heap/VM operations.
use super::{
    task_registry as registry,
    task_types::{Bytes, CreateFailure, Entry, Role, State, Task, TaskCore},
};
use crate::{
    arch::{fpu, percpu, runtime, smp_policy},
    build_bridge::config,
    memory::heap,
    platform::timer,
    sched::task_context,
};
use core::{
    cell::UnsafeCell,
    mem::{align_of, size_of},
    ptr::{self, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};
use registry::{CriticalBundle, GuardedStack, NO_RESERVE};
pub(crate) const STACK_SIZE: u64 = 64 * 1024;
pub(crate) const GUARD_SIZE: u64 = 16 * 1024;
const PATTERN: u8 = 0xa5;
unsafe extern "C" {
    // Each VM operation is independently admitted by the existing memory
    // owner; no registry state or native task layout crosses this boundary.
    fn r4os_zig_task_vm(operation: u8, id: u32, output: *mut u64) -> u32;
    fn r4os_zig_kernel_write(text: *const u8, len: usize);
    fn r4os_zig_kernel_dec(value: u64);
    fn taskEntryTrampoline() -> !;
}
fn puts(text: &[u8]) {
    unsafe { r4os_zig_kernel_write(text.as_ptr(), text.len()) }
}
fn dec(value: u64) {
    unsafe { r4os_zig_kernel_dec(value) }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct FailureStats {
    pub task_metadata: u64,
    pub stack: u64,
    pub fpu: u64,
    pub waiter: u64,
    pub memory: u64,
    pub last: u8,
}
struct Failures(UnsafeCell<(FailureStats, u8)>);
// SAFETY: runtime admission covers failure accounting and one-shot injection.
unsafe impl Sync for Failures {}
static FAILURES: Failures = Failures(UnsafeCell::new((
    FailureStats {
        task_metadata: 0,
        stack: 0,
        fpu: 0,
        waiter: 0,
        memory: 0,
        last: 0,
    },
    0,
)));
pub(super) unsafe fn note_failure(failure: CreateFailure) {
    if failure == CreateFailure::None {
        return;
    }
    let flags = runtime::save_runtime();
    unsafe {
        let stats = ptr::addr_of_mut!((*FAILURES.0.get()).0);
        (*stats).last = failure as u8;
        let value = match failure {
            CreateFailure::TaskMetadata => ptr::addr_of_mut!((*stats).task_metadata),
            CreateFailure::Stack => ptr::addr_of_mut!((*stats).stack),
            CreateFailure::Fpu => ptr::addr_of_mut!((*stats).fpu),
            CreateFailure::Waiter => ptr::addr_of_mut!((*stats).waiter),
            CreateFailure::Memory => ptr::addr_of_mut!((*stats).memory),
            CreateFailure::None => unreachable!(),
        };
        value.write(value.read().wrapping_add(1));
    }
    runtime::restore_runtime(flags);
}
unsafe fn failed(output: *mut CreateFailure, failure: CreateFailure) {
    unsafe {
        output.write(failure);
        note_failure(failure)
    }
}
pub(super) unsafe fn failure_stats() -> FailureStats {
    let flags = runtime::save_runtime();
    let value = unsafe { (*FAILURES.0.get()).0 };
    runtime::restore_runtime(flags);
    value
}
pub(super) unsafe fn arm_failure(failure: CreateFailure) -> bool {
    if failure == CreateFailure::None {
        return false;
    }
    let flags = runtime::save_runtime();
    let admitted = unsafe {
        if (*FAILURES.0.get()).1 == 0 {
            (*FAILURES.0.get()).1 = failure as u8;
            true
        } else {
            false
        }
    };
    runtime::restore_runtime(flags);
    admitted
}
unsafe fn take_failure() -> CreateFailure {
    let flags = runtime::save_runtime();
    let value = unsafe { (*FAILURES.0.get()).1 };
    unsafe { (*FAILURES.0.get()).1 = 0 };
    runtime::restore_runtime(flags);
    match value {
        0 => CreateFailure::None,
        1 => CreateFailure::TaskMetadata,
        2 => CreateFailure::Stack,
        3 => CreateFailure::Fpu,
        4 => CreateFailure::Waiter,
        5 => CreateFailure::Memory,
        _ => unreachable!(),
    }
}
struct Telemetry {
    fields: [AtomicU64; 8],
}
static TELEMETRY: [Telemetry; 4] = [const {
    Telemetry {
        fields: [const { AtomicU64::new(0) }; 8],
    }
}; 4];
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StackTelemetry {
    pub created: u64,
    pub released: u64,
    pub high_water_total_bytes: u64,
    pub high_water_max_bytes: u64,
    pub create_cycles_total: u64,
    pub create_cycles_max: u64,
    pub release_cycles_total: u64,
    pub release_cycles_max: u64,
}
pub fn telemetry_for(role: Role) -> StackTelemetry {
    let f = &TELEMETRY[role as usize].fields;
    StackTelemetry {
        created: f[0].load(Ordering::Relaxed),
        released: f[1].load(Ordering::Relaxed),
        high_water_total_bytes: f[2].load(Ordering::Relaxed),
        high_water_max_bytes: f[3].load(Ordering::Relaxed),
        create_cycles_total: f[4].load(Ordering::Relaxed),
        create_cycles_max: f[5].load(Ordering::Relaxed),
        release_cycles_total: f[6].load(Ordering::Relaxed),
        release_cycles_max: f[7].load(Ordering::Relaxed),
    }
}
pub fn record_created(role: Role, cycles: u64) {
    let f = &TELEMETRY[role as usize].fields;
    f[0].fetch_add(1, Ordering::Relaxed);
    if config::ENABLE_METRICS {
        f[4].fetch_add(cycles, Ordering::Relaxed);
        f[5].fetch_max(cycles, Ordering::Relaxed);
    }
}
pub fn record_released(role: Role, high_water: u64, cycles: u64) {
    let f = &TELEMETRY[role as usize].fields;
    f[1].fetch_add(1, Ordering::Relaxed);
    if config::ENABLE_METRICS {
        f[2].fetch_add(high_water, Ordering::Relaxed);
        f[3].fetch_max(high_water, Ordering::Relaxed);
        f[6].fetch_add(cycles, Ordering::Relaxed);
        f[7].fetch_max(cycles, Ordering::Relaxed);
    }
}
pub fn timestamp() -> u64 {
    if !config::ENABLE_METRICS {
        return 0;
    }
    let low: u32;
    let high: u32;
    unsafe { core::arch::asm!("rdtsc",out("eax") low,out("edx") high,options(nomem,nostack)) };
    u64::from(low) | (u64::from(high) << 32)
}
pub(super) unsafe fn poison(stack: GuardedStack) {
    if config::ENABLE_METRICS && stack.base != 0 && stack.top > stack.base {
        unsafe {
            ptr::write_bytes(
                ptr::with_exposed_provenance_mut::<u8>(stack.base as usize),
                PATTERN,
                (stack.top - stack.base) as usize,
            )
        };
    }
}
pub(super) unsafe fn measure(task: *mut Task) {
    if !config::ENABLE_METRICS {
        return;
    }
    unsafe {
        let c = registry::core(task);
        if (*c).stack_high_water_measured {
            return;
        };
        (*c).stack_high_water_measured = true;
        if (*c).stack_base == 0 || (*c).stack_top <= (*c).stack_base {
            return;
        }
        let len = ((*c).stack_top - (*c).stack_base) as usize;
        let bytes = ptr::with_exposed_provenance::<u8>((*c).stack_base as usize);
        let mut untouched = 0;
        while untouched < len && bytes.add(untouched).read() == PATTERN {
            untouched += 1;
        }
        (*c).stack_high_water_bytes = (len - untouched) as u64;
    }
}
pub fn release_stack_direct(stack: GuardedStack) -> bool {
    stack.range_id == 0 || unsafe { r4os_zig_task_vm(4, stack.range_id, null_mut()) == 0 }
}
unsafe fn anchor_fresh(owner: *mut Task, stack: GuardedStack) {
    if release_stack_direct(stack) {
        return;
    }
    unsafe {
        let c = registry::core(owner);
        (*c).stack_range_id = stack.range_id;
        (*c).stack_base = stack.base;
        (*c).stack_top = stack.top;
    }
}
pub(super) unsafe fn alloc_stack(owner: *mut Task) -> Option<GuardedStack> {
    unsafe {
        let flags = runtime::save_runtime();
        let r = registry::raw();
        let mut selected = None;
        for index in (0..(*r).stack_cache_count).rev() {
            if !(*r).stack_cache[index].release_pending
                && !(*r).stack_cache[index].release_in_progress
            {
                selected = Some(index);
                break;
            }
        }
        if let Some(index) = selected {
            let last = (*r).stack_cache_count - 1;
            let stack = (*r).stack_cache[index];
            if index != last {
                (*r).stack_cache[index] = (*r).stack_cache[last];
            }
            (*r).stack_cache_count = last;
            (*r).stack_cache_hits = (*r).stack_cache_hits.wrapping_add(1);
            runtime::restore_runtime(flags);
            poison(stack);
            return Some(stack);
        }
        (*r).stack_cache_misses = (*r).stack_cache_misses.wrapping_add(1);
        runtime::restore_runtime(flags);
        let mut id = 0u64;
        if r4os_zig_task_vm(0, 0, &mut id) != 0 {
            return None;
        }
        let mut base = 0u64;
        if r4os_zig_task_vm(1, id as u32, &mut base) != 0 {
            anchor_fresh(
                owner,
                GuardedStack {
                    range_id: id as u32,
                    ..GuardedStack::EMPTY
                },
            );
            return None;
        }
        let stack = GuardedStack {
            range_id: id as u32,
            base: base + GUARD_SIZE,
            top: base + GUARD_SIZE + STACK_SIZE,
            ..GuardedStack::EMPTY
        };
        if r4os_zig_task_vm(2, id as u32, null_mut()) != 0
            || r4os_zig_task_vm(3, id as u32, null_mut()) != 0
        {
            anchor_fresh(owner, stack);
            return None;
        }
        poison(stack);
        Some(stack)
    }
}
pub(super) unsafe fn allocate_fpu(task: *mut Task) -> bool {
    unsafe {
        let bytes = fpu::r4os_rust_fpu_control(3) as usize;
        if bytes == 0 {
            return false;
        }
        let pointer = heap::r4os_rust_heap_alloc(bytes, fpu::STORAGE_ALIGN);
        if pointer.is_null() {
            return false;
        }
        let c = registry::core(task);
        (*c).fpu_state = Some(Bytes {
            pointer,
            len: bytes,
        });
        (*c).fpu_state_bytes = bytes as u32;
        (*c).fpu_state_valid = fpu::r4os_rust_fpu_buffer(0, pointer, bytes);
        (*c).fpu_state_valid
    }
}
pub(super) unsafe fn free_metadata(task: *mut Task) -> bool {
    if unsafe { heap::r4os_rust_heap_free(task.cast(), size_of::<Task>()) } != 0 {
        puts(b"Task object release failed\r\n");
        false
    } else {
        true
    }
}
pub(super) unsafe fn release_unpublished(task: *mut Task) -> bool {
    unsafe {
        let c = registry::core(task);
        if let Some(bytes) = (*c).fpu_state {
            if heap::r4os_rust_heap_free(bytes.pointer.cast_mut(), bytes.len) != 0 {
                puts(b"Unpublished Task FPU release pending retry\r\n");
                return false;
            }
            (*c).fpu_state = None;
            (*c).fpu_state_bytes = 0;
            (*c).fpu_state_valid = false;
        }
        if (*c).stack_range_id != 0 {
            let stack = GuardedStack {
                range_id: (*c).stack_range_id,
                base: (*c).stack_base,
                top: (*c).stack_top,
                ..GuardedStack::EMPTY
            };
            if !release_stack_direct(stack) {
                return false;
            }
            (*c).stack_range_id = 0;
            (*c).stack_base = 0;
            (*c).stack_top = 0;
        }
        true
    }
}
pub(super) unsafe fn rollback(task: *mut Task) {
    unsafe {
        if release_unpublished(task) && free_metadata(task) {
            return;
        }
        let flags = runtime::save_runtime();
        registry::enqueue_rollback(task);
        runtime::restore_runtime(flags);
        puts(b"Unpublished Task rollback pending retry\r\n");
    }
}
pub(super) unsafe fn retry_rollback() -> bool {
    unsafe {
        let unwind = task_context::enter();
        if !unwind.admitted() {
            return false;
        }
        let flags = runtime::save_runtime();
        let mut candidate = (*registry::raw()).rollback_head;
        while !candidate.is_null() && (*registry::core(candidate)).release_in_progress {
            candidate = (*registry::core(candidate)).rollback_next;
        }
        if !candidate.is_null() {
            (*registry::core(candidate)).release_in_progress = true;
        }
        runtime::restore_runtime(flags);
        let success = if candidate.is_null() {
            false
        } else if !release_unpublished(candidate) {
            let f = runtime::save_runtime();
            (*registry::core(candidate)).release_in_progress = false;
            runtime::restore_runtime(f);
            false
        } else {
            let f = runtime::save_runtime();
            let unlinked = registry::unlink_rollback(candidate);
            if !unlinked {
                (*registry::core(candidate)).release_in_progress = false;
            }
            runtime::restore_runtime(f);
            if !unlinked {
                puts(b"Unpublished Task rollback anchor missing\r\n");
                false
            } else if free_metadata(candidate) {
                true
            } else {
                let f = runtime::save_runtime();
                registry::enqueue_rollback(candidate);
                runtime::restore_runtime(f);
                puts(b"Unpublished Task rollback pending retry\r\n");
                false
            }
        };
        task_context::leave(unwind);
        success
    }
}
pub(super) unsafe fn reclaim_cache(requested: u32) -> u32 {
    unsafe {
        let unwind = task_context::enter();
        if !unwind.admitted() {
            return 0;
        }
        let limit = if requested == 0 {
            registry::STACK_CACHE_LIMIT
        } else {
            requested as usize
        };
        let mut reclaimed = 0u32;
        while (reclaimed as usize) < limit {
            let flags = runtime::save_runtime();
            let r = registry::raw();
            let mut selected = None;
            for index in 0..(*r).stack_cache_count {
                if !(*r).stack_cache[index].release_in_progress {
                    selected = Some(index);
                    break;
                }
            }
            let Some(index) = selected else {
                runtime::restore_runtime(flags);
                break;
            };
            (*r).stack_cache[index].release_pending = true;
            (*r).stack_cache[index].release_in_progress = true;
            let stack = (*r).stack_cache[index];
            runtime::restore_runtime(flags);
            let released = release_stack_direct(stack);
            let f = runtime::save_runtime();
            let found = (0..(*r).stack_cache_count)
                .find(|&i| (*r).stack_cache[i].range_id == stack.range_id);
            if !released {
                if let Some(i) = found {
                    (*r).stack_cache[i].release_pending = true;
                    (*r).stack_cache[i].release_in_progress = false;
                }
                (*r).stack_cache_release_failures =
                    (*r).stack_cache_release_failures.wrapping_add(1);
                runtime::restore_runtime(f);
                puts(b"Task stack cache release pending retry\r\n");
                break;
            }
            let Some(i) = found else {
                runtime::restore_runtime(f);
                puts(b"Task stack cache release anchor missing\r\n");
                break;
            };
            let last = (*r).stack_cache_count - 1;
            if i != last {
                (*r).stack_cache[i] = (*r).stack_cache[last];
            }
            (*r).stack_cache_count = last;
            reclaimed += 1;
            (*r).stack_cache_reclaims = (*r).stack_cache_reclaims.wrapping_add(1);
            runtime::restore_runtime(f);
        }
        task_context::leave(unwind);
        reclaimed
    }
}
pub(super) unsafe fn release_task_stack(task: *mut Task) -> bool {
    unsafe {
        let c = registry::core(task);
        if (*c).stack_range_id == 0 {
            (*c).stack_base = 0;
            (*c).stack_top = 0;
            return true;
        }
        measure(task);
        let stack = GuardedStack {
            range_id: (*c).stack_range_id,
            base: (*c).stack_base,
            top: (*c).stack_top,
            ..GuardedStack::EMPTY
        };
        let f = runtime::save_runtime();
        let r = registry::raw();
        let direct = (*r).stack_cache_count == registry::STACK_CACHE_LIMIT;
        if !direct {
            (*r).stack_cache[(*r).stack_cache_count] = stack;
            (*r).stack_cache_count += 1;
        }
        runtime::restore_runtime(f);
        if direct && !release_stack_direct(stack) {
            return false;
        }
        (*c).stack_range_id = 0;
        (*c).stack_base = 0;
        (*c).stack_top = 0;
        true
    }
}
pub(super) unsafe fn release_resources(task: *mut Task) -> bool {
    unsafe {
        let c = registry::core(task);
        if (*c).held_lock_count != 0 || (*c).unwind_guard_count != 0 {
            puts(b"TASK RELEASE INVARIANT: owned synchronization remains id=");
            dec(u64::from((*c).id));
            puts(b"\r\n");
            runtime::halt_forever();
        }
        if !crate::sched::wait_node::clear_owner(ptr::addr_of_mut!((*c).wait_node)) {
            return false;
        }
        if (*c).critical_reserve_slot != NO_RESERVE {
            return true;
        }
        if let Some(bytes) = (*c).fpu_state {
            if heap::r4os_rust_heap_free(bytes.pointer.cast_mut(), bytes.len) != 0 {
                puts(b"Task FPU state release pending retry\r\n");
                return false;
            }
            (*c).fpu_state = None;
            (*c).fpu_state_bytes = 0;
            (*c).fpu_state_valid = false;
        }
        release_task_stack(task)
    }
}
pub(super) unsafe fn initial_rsp(top: u64) -> u64 {
    unsafe {
        let mut layout = core::mem::MaybeUninit::uninit();
        smp_policy::r4os_rust_initial_stack(top, layout.as_mut_ptr());
        let layout = layout.assume_init();
        let mut sp = layout.entry_rsp - 8;
        ptr::with_exposed_provenance_mut::<u64>(sp as usize)
            .write(taskEntryTrampoline as *const () as u64);
        for _ in 0..6 {
            sp -= 8;
            ptr::with_exposed_provenance_mut::<u64>(sp as usize).write(0);
        }
        sp
    }
}
#[derive(Clone, Copy)]
pub struct CreateArgs {
    pub name: Bytes,
    pub state: State,
    pub needs_fpu: bool,
    pub entry: Option<Entry>,
    pub role: Role,
    pub smp_eligible: bool,
    pub smp_r4x: bool,
    pub owner_kind: super::task_types::ExecutionOwnerKind,
    pub owner_context: *mut core::ffi::c_void,
}
pub(crate) unsafe fn create(args: CreateArgs, failure: *mut CreateFailure) -> *mut Task {
    unsafe {
        failure.write(CreateFailure::None);
        if !(*registry::raw()).initialized {
            failed(failure, CreateFailure::Memory);
            return null_mut();
        }
        let started = timestamp();
        let unwind = task_context::enter();
        if !unwind.admitted() {
            failed(failure, CreateFailure::Memory);
            return null_mut();
        }
        let result = create_admitted(args, failure, started);
        task_context::leave(unwind);
        result
    }
}
unsafe fn create_admitted(
    args: CreateArgs,
    failure: *mut CreateFailure,
    started: u64,
) -> *mut Task {
    unsafe {
        let forced = take_failure();
        if forced != CreateFailure::None {
            failed(failure, forced);
            return null_mut();
        }
        let task = heap::r4os_rust_heap_alloc(size_of::<Task>(), align_of::<Task>()).cast::<Task>();
        if task.is_null() {
            failed(failure, CreateFailure::TaskMetadata);
            return null_mut();
        }
        task.write(Task {
            core: UnsafeCell::new(TaskCore::default()),
        });
        let c = registry::core(task);
        if !(*c).wait_node.queue.is_null() {
            failed(failure, CreateFailure::Waiter);
            rollback(task);
            return null_mut();
        }
        let Some(stack) = alloc_stack(task) else {
            failed(failure, CreateFailure::Stack);
            rollback(task);
            return null_mut();
        };
        (*c).stack_base = stack.base;
        (*c).stack_top = stack.top;
        (*c).stack_range_id = stack.range_id;
        if args.needs_fpu && !allocate_fpu(task) {
            failed(failure, CreateFailure::Fpu);
            rollback(task);
            return null_mut();
        }
        let fpu_memory = (*c).fpu_state;
        let now = timer::tick_count();
        let running = args.state == State::Running;
        let cpu = percpu::current_index() as u8;
        c.write(TaskCore {
            name: args.name,
            state: args.state,
            home_cpu: if running { cpu } else { 0 },
            home_cpu_bound: running,
            running_cpu: if running { cpu } else { 0xff },
            smp_eligible: args.smp_eligible,
            smp_r4x_work: args.smp_r4x,
            rsp: if args.entry.is_some() {
                initial_rsp(stack.top)
            } else {
                0
            },
            stack_base: stack.base,
            stack_top: stack.top,
            stack_range_id: stack.range_id,
            created_tick: now,
            ready_since_tick: if args.state == State::Ready { now } else { 0 },
            last_scheduled_tick: if running { now } else { 0 },
            last_accounted_tick: if running { now } else { 0 },
            switches_in: u64::from(running),
            entry: args.entry,
            fpu_state: fpu_memory,
            fpu_state_bytes: fpu_memory.map_or(0, |b| b.len as u32),
            fpu_state_valid: fpu_memory.is_some(),
            uses_fpu: args.needs_fpu,
            ..TaskCore::default()
        });
        registry::configure_role(task, args.role);
        let flags = runtime::save_runtime();
        let Some(id) = registry::allocate_id() else {
            failed(failure, CreateFailure::Memory);
            runtime::restore_runtime(flags);
            rollback(task);
            return null_mut();
        };
        let Some(generation) = registry::allocate_generation() else {
            failed(failure, CreateFailure::Memory);
            runtime::restore_runtime(flags);
            rollback(task);
            return null_mut();
        };
        (*c).id = id;
        (*c).generation = generation;
        (*c).execution_owner_kind = args.owner_kind;
        (*c).execution_owner_context = args.owner_context;
        if args.owner_kind == super::task_types::ExecutionOwnerKind::DriverThread {
            (*c).unwind_guard_count = 1;
        }
        registry::link_registry(task);
        if matches!(args.state, State::Ready | State::Running) {
            registry::reset_activation(task);
        }
        runtime::restore_runtime(flags);
        record_created(args.role, timestamp().wrapping_sub(started));
        task
    }
}

pub(super) unsafe fn reset_critical(index: usize) {
    unsafe {
        let bundle = (*registry::raw()).critical[index];
        if bundle.task.is_null() {
            return;
        }
        registry::core(bundle.task).write(TaskCore {
            critical_reserve_slot: index as u8,
            stack_base: bundle.stack.base,
            stack_top: bundle.stack.top,
            stack_range_id: bundle.stack.range_id,
            fpu_state: bundle.fpu_state,
            fpu_state_bytes: bundle.fpu_state.map_or(0, |b| b.len as u32),
            fpu_state_valid: bundle.fpu_state.is_some(),
            ..TaskCore::default()
        });
    }
}
unsafe fn cleanup_critical() {
    unsafe {
        for index in 0..registry::CRITICAL_RESERVE_COUNT {
            let task = (*registry::raw()).critical[index].task;
            if task.is_null() {
                continue;
            }
            rollback(task);
            (*registry::raw()).critical[index] = CriticalBundle::EMPTY;
        }
        (*registry::raw()).critical_in_use = 0;
    }
}
unsafe fn prepare_critical() -> bool {
    unsafe {
        (*registry::raw()).critical = [CriticalBundle::EMPTY; registry::CRITICAL_RESERVE_COUNT];
        (*registry::raw()).critical_in_use = 0;
        for index in 0..registry::CRITICAL_RESERVE_COUNT {
            let task =
                heap::r4os_rust_heap_alloc(size_of::<Task>(), align_of::<Task>()).cast::<Task>();
            if task.is_null() {
                note_failure(CreateFailure::TaskMetadata);
                cleanup_critical();
                return false;
            }
            task.write(Task {
                core: UnsafeCell::new(TaskCore {
                    critical_reserve_slot: index as u8,
                    ..TaskCore::default()
                }),
            });
            let Some(stack) = alloc_stack(task) else {
                note_failure(CreateFailure::Stack);
                rollback(task);
                cleanup_critical();
                return false;
            };
            let c = registry::core(task);
            (*c).stack_base = stack.base;
            (*c).stack_top = stack.top;
            (*c).stack_range_id = stack.range_id;
            if !allocate_fpu(task) {
                note_failure(CreateFailure::Fpu);
                rollback(task);
                cleanup_critical();
                return false;
            }
            (*registry::raw()).critical[index] = CriticalBundle {
                task,
                stack,
                fpu_state: (*c).fpu_state,
                in_use: false,
            };
        }
        true
    }
}
pub(super) unsafe fn init() -> bool {
    unsafe {
        registry::reset();
        (*FAILURES.0.get()).0 = FailureStats::default();
        (*FAILURES.0.get()).1 = 0;
        for role in &TELEMETRY {
            for field in &role.fields {
                field.store(0, Ordering::Relaxed);
            }
        }
        if !prepare_critical() {
            return false;
        }
        (*registry::raw()).initialized = true;
        let mut failure = CreateFailure::None;
        let main = create(
            CreateArgs {
                name: Bytes {
                    pointer: b"kernel-main".as_ptr(),
                    len: 11,
                },
                state: State::Running,
                needs_fpu: true,
                entry: None,
                role: Role::Interactive,
                smp_eligible: false,
                smp_r4x: false,
                owner_kind: super::task_types::ExecutionOwnerKind::None,
                owner_context: null_mut(),
            },
            &mut failure,
        );
        if main.is_null() {
            (*registry::raw()).initialized = false;
            cleanup_critical();
            return false;
        }
        percpu::r4os_rust_percpu_control(11, 0, 1, 0);
        puts(b"[TASKSTACK] guard-pages active: guard=");
        dec(GUARD_SIZE);
        puts(b" stack=");
        dec(STACK_SIZE);
        puts(b" registry=dynamic stack_cache=");
        dec(registry::STACK_CACHE_LIMIT as u64);
        puts(b" critical_reserve=");
        dec(registry::CRITICAL_RESERVE_COUNT as u64);
        puts(b"\r\n");
        true
    }
}
pub(crate) unsafe fn create_critical(
    name: Bytes,
    state: State,
    entry: Entry,
    role: Role,
    failure: *mut CreateFailure,
) -> *mut Task {
    unsafe {
        failure.write(CreateFailure::None);
        if !(*registry::raw()).initialized {
            failed(failure, CreateFailure::Memory);
            return null_mut();
        }
        let started = timestamp();
        let flags = runtime::save_runtime();
        let r = registry::raw();
        let selected = (0..registry::CRITICAL_RESERVE_COUNT).find(|&i| !(*r).critical[i].in_use);
        let Some(index) = selected else {
            failed(failure, CreateFailure::TaskMetadata);
            runtime::restore_runtime(flags);
            return null_mut();
        };
        let bundle = (*r).critical[index];
        let task = bundle.task;
        if task.is_null() {
            failed(failure, CreateFailure::TaskMetadata);
            runtime::restore_runtime(flags);
            return null_mut();
        }
        let c = registry::core(task);
        if !(*c).wait_node.queue.is_null() {
            failed(failure, CreateFailure::Waiter);
            runtime::restore_runtime(flags);
            return null_mut();
        }
        let Some(bytes) = bundle.fpu_state else {
            failed(failure, CreateFailure::Fpu);
            runtime::restore_runtime(flags);
            return null_mut();
        };
        if !fpu::r4os_rust_fpu_buffer(0, bytes.pointer.cast_mut(), bytes.len) {
            failed(failure, CreateFailure::Fpu);
            runtime::restore_runtime(flags);
            return null_mut();
        }
        let now = timer::tick_count();
        let running = state == State::Running;
        c.write(TaskCore {
            critical_reserve_slot: index as u8,
            name,
            state,
            rsp: initial_rsp(bundle.stack.top),
            stack_base: bundle.stack.base,
            stack_top: bundle.stack.top,
            stack_range_id: bundle.stack.range_id,
            created_tick: now,
            ready_since_tick: if state == State::Ready { now } else { 0 },
            last_scheduled_tick: if running { now } else { 0 },
            switches_in: u64::from(running),
            entry: Some(entry),
            fpu_state: Some(bytes),
            fpu_state_bytes: bytes.len as u32,
            fpu_state_valid: true,
            uses_fpu: true,
            ..TaskCore::default()
        });
        registry::configure_role(task, role);
        let Some(id) = registry::allocate_id() else {
            failed(failure, CreateFailure::Memory);
            reset_critical(index);
            runtime::restore_runtime(flags);
            return null_mut();
        };
        let Some(generation) = registry::allocate_generation() else {
            failed(failure, CreateFailure::Memory);
            reset_critical(index);
            runtime::restore_runtime(flags);
            return null_mut();
        };
        (*c).id = id;
        (*c).generation = generation;
        (*r).critical[index].in_use = true;
        (*r).critical_in_use += 1;
        registry::link_registry(task);
        if matches!(state, State::Ready | State::Running) {
            registry::reset_activation(task);
        }
        runtime::restore_runtime(flags);
        record_created(role, timestamp().wrapping_sub(started));
        task
    }
}
