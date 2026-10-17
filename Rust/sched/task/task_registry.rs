//! Native registry/queue/role owner. All storage access is raw and runtime-
//! serialized; no state reference survives a callback, allocation, wait or
//! physical context switch.
use super::task_types::{Role, State, Task, TaskCore};
use crate::{
    arch::{percpu, smp_policy},
    sched::task_index::{self, Index},
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::offset_of,
    ptr::{self, null_mut},
};

pub const ROLE_COUNT: usize = 4;
pub const STACK_CACHE_LIMIT: usize = 8;
pub const CRITICAL_RESERVE_COUNT: usize = 4;
pub const NO_RESERVE: u8 = 0xff;
pub const MAX_CPUS: usize = 32;
pub const NO_RANK: u8 = 0xff;

#[derive(Clone, Copy)]
pub struct GuardedStack {
    pub range_id: u32,
    pub base: u64,
    pub top: u64,
    pub release_pending: bool,
    pub release_in_progress: bool,
}
impl GuardedStack {
    pub const EMPTY: Self = Self {
        range_id: 0,
        base: 0,
        top: 0,
        release_pending: false,
        release_in_progress: false,
    };
}
#[derive(Clone, Copy)]
pub struct CriticalBundle {
    pub task: *mut Task,
    pub stack: GuardedStack,
    pub fpu_state: Option<super::task_types::Bytes>,
    pub in_use: bool,
}
impl CriticalBundle {
    pub const EMPTY: Self = Self {
        task: null_mut(),
        stack: GuardedStack::EMPTY,
        fpu_state: None,
        in_use: false,
    };
}
pub struct ReadyQueue {
    pub head: *mut Task,
    pub tail: *mut Task,
    pub count: usize,
    pub dispatch_counts: [usize; ROLE_COUNT],
    pub ranks: [Index; ROLE_COUNT],
    pub warning_cursor: *mut Task,
    pub balance_cursor: *mut Task,
}
impl ReadyQueue {
    const EMPTY: Self = Self {
        head: null_mut(),
        tail: null_mut(),
        count: 0,
        dispatch_counts: [0; ROLE_COUNT],
        ranks: [const {
            Index {
                root: null_mut(),
                first: null_mut(),
                count: 0,
                visits: 0,
            }
        }; ROLE_COUNT],
        warning_cursor: null_mut(),
        balance_cursor: null_mut(),
    };
}
pub struct Registry {
    pub head: *mut Task,
    pub tail: *mut Task,
    pub ready: [ReadyQueue; MAX_CPUS],
    pub role_activations: [u64; ROLE_COUNT],
    pub role_exhaustions: [u64; ROLE_COUNT],
    pub donation_requests: u64,
    pub donation_applied: u64,
    pub donation_released: u64,
    pub donation_exhaustions: u64,
    pub timeout_head: *mut Task,
    pub timeout_tail: *mut Task,
    pub timeout_count: usize,
    pub next_timeout_sequence: u128,
    pub next_ready_order: u128,
    pub timeout_index: Index,
    pub reap_head: *mut Task,
    pub reap_tail: *mut Task,
    pub reap_count: usize,
    pub reap_passes: u64,
    pub reap_visits: u64,
    pub rollback_head: *mut Task,
    pub rollback_count: usize,
    pub count: usize,
    pub peak: usize,
    pub epoch: u64,
    pub next_id: u32,
    pub next_generation: u64,
    pub generation_exhausted: bool,
    pub initialized: bool,
    pub kill_held_lock_deferrals: u64,
    pub recycle_guard_hits: u64,
    pub stack_cache: [GuardedStack; STACK_CACHE_LIMIT],
    pub stack_cache_count: usize,
    pub stack_cache_hits: u64,
    pub stack_cache_misses: u64,
    pub stack_cache_reclaims: u64,
    pub stack_cache_release_failures: u64,
    pub critical: [CriticalBundle; CRITICAL_RESERVE_COUNT],
    pub critical_in_use: usize,
    pub fpu_lazy_saves: u64,
    pub fpu_lazy_skips: u64,
}
impl Registry {
    const EMPTY: Self = Self {
        head: null_mut(),
        tail: null_mut(),
        ready: [const { ReadyQueue::EMPTY }; MAX_CPUS],
        role_activations: [0; ROLE_COUNT],
        role_exhaustions: [0; ROLE_COUNT],
        donation_requests: 0,
        donation_applied: 0,
        donation_released: 0,
        donation_exhaustions: 0,
        timeout_head: null_mut(),
        timeout_tail: null_mut(),
        timeout_count: 0,
        next_timeout_sequence: 1,
        next_ready_order: 1,
        timeout_index: Index {
            root: null_mut(),
            first: null_mut(),
            count: 0,
            visits: 0,
        },
        reap_head: null_mut(),
        reap_tail: null_mut(),
        reap_count: 0,
        reap_passes: 0,
        reap_visits: 0,
        rollback_head: null_mut(),
        rollback_count: 0,
        count: 0,
        peak: 0,
        epoch: 1,
        next_id: 1,
        next_generation: 1,
        generation_exhausted: false,
        initialized: false,
        kill_held_lock_deferrals: 0,
        recycle_guard_hits: 0,
        stack_cache: [GuardedStack::EMPTY; STACK_CACHE_LIMIT],
        stack_cache_count: 0,
        stack_cache_hits: 0,
        stack_cache_misses: 0,
        stack_cache_reclaims: 0,
        stack_cache_release_failures: 0,
        critical: [CriticalBundle::EMPTY; CRITICAL_RESERVE_COUNT],
        critical_in_use: 0,
        fpu_lazy_saves: 0,
        fpu_lazy_skips: 0,
    };
}
struct Storage(UnsafeCell<Registry>);
// SAFETY: BSP initialization before AP admission. Later users retain runtime
// admission; every function finishes before calling any external owner.
unsafe impl Sync for Storage {}
static REGISTRY: Storage = Storage(UnsafeCell::new(Registry::EMPTY));
pub fn raw() -> *mut Registry {
    REGISTRY.0.get()
}
pub(crate) unsafe fn core(task: *mut Task) -> *mut TaskCore {
    unsafe { (*task).core.get() }
}
pub(super) unsafe fn reset() {
    unsafe { raw().write(Registry::EMPTY) }
}
pub(super) unsafe fn queue(cpu: u32) -> *mut ReadyQueue {
    let index = if cpu < MAX_CPUS as u32 {
        cpu as usize
    } else {
        0
    };
    unsafe { ptr::addr_of_mut!((*raw()).ready[index]) }
}
unsafe fn queue_for(task: *mut Task) -> *mut ReadyQueue {
    unsafe { queue(u32::from((*core(task)).home_cpu)) }
}
pub(super) unsafe fn bump_epoch() {
    unsafe {
        (*raw()).epoch = (*raw()).epoch.wrapping_add(1);
        if (*raw()).epoch == 0 {
            (*raw()).epoch = 1;
        }
    }
}
pub(super) unsafe fn contains(task: *const Task) -> bool {
    unsafe {
        let mut cursor = (*raw()).head;
        while !cursor.is_null() {
            if ptr::eq(cursor, task) {
                return true;
            }
            cursor = (*core(cursor)).registry_next;
        }
        false
    }
}
pub(super) unsafe fn find_id(id: u32) -> *mut Task {
    unsafe {
        let mut cursor = (*raw()).head;
        while !cursor.is_null() {
            if (*core(cursor)).id == id {
                return cursor;
            }
            cursor = (*core(cursor)).registry_next;
        }
        null_mut()
    }
}
pub(super) unsafe fn find_identity(id: u32, generation: u64) -> *mut Task {
    unsafe {
        let found = find_id(id);
        if !found.is_null() && (*core(found)).generation == generation {
            found
        } else {
            null_mut()
        }
    }
}
pub(super) unsafe fn allocate_id() -> Option<u32> {
    unsafe {
        let mut candidate = if (*raw()).next_id == 0 {
            1
        } else {
            (*raw()).next_id
        };
        let start = candidate;
        loop {
            if find_id(candidate).is_null() {
                (*raw()).next_id = candidate.wrapping_add(1);
                if (*raw()).next_id == 0 {
                    (*raw()).next_id = 1;
                }
                return Some(candidate);
            }
            candidate = candidate.wrapping_add(1);
            if candidate == 0 {
                candidate = 1;
            }
            if candidate == start {
                return None;
            }
        }
    }
}
pub(super) unsafe fn allocate_generation() -> Option<u64> {
    unsafe {
        if (*raw()).generation_exhausted || (*raw()).next_generation == 0 {
            return None;
        }
        let value = (*raw()).next_generation;
        (*raw()).next_generation = value.wrapping_add(1);
        if (*raw()).next_generation == 0 {
            (*raw()).generation_exhausted = true;
        }
        Some(value)
    }
}
fn high(role: Role) -> bool {
    matches!(role, Role::Input | Role::ShortCompletion)
}
fn policy(role: Role) -> (super::task_types::Priority, u32, u16) {
    use super::task_types::Priority;
    match role {
        Role::Input => (Priority::High, 2, 4),
        Role::ShortCompletion => (Priority::High, 4, 4),
        Role::Interactive => (Priority::Normal, 0, 0),
        Role::Batch => (Priority::Low, 0, 0),
    }
}
// Scalar TaskView callers may need no role metadata at all. Keep the pure
// calculation visible so its unused projection can be removed.
#[inline(always)]
pub(super) unsafe fn inherited_rank(task: *mut Task) -> u8 {
    unsafe {
        for index in 0..ROLE_COUNT {
            if (*core(task)).inherited_rank_counts[index] != 0 {
                return index as u8;
            }
        }
        NO_RANK
    }
}
// Scalar TaskView callers may need no role metadata at all. Keep the pure
// calculation visible so its unused projection can be removed.
#[inline(always)]
pub(crate) unsafe fn dispatch_rank(task: *mut Task) -> u8 {
    unsafe {
        let c = core(task);
        let role_boost = high((*c).role)
            && (*c).role_budget_ticks_remaining != 0
            && (*c).role_budget_dispatches_remaining != 0;
        let base = match (*c).role {
            Role::Input if role_boost => 0,
            Role::ShortCompletion if role_boost => 1,
            Role::Input | Role::ShortCompletion | Role::Interactive => 2,
            Role::Batch => 3,
        };
        let inherited = inherited_rank(task);
        if inherited != NO_RANK
            && (*c).donation_budget_ticks_remaining != 0
            && (*c).donation_budget_dispatches_remaining != 0
        {
            base.min(inherited)
        } else {
            base
        }
    }
}
pub(crate) unsafe fn wake_rank(task: *mut Task) -> u8 {
    unsafe { (*core(task)).role as u8 }
}
pub(super) unsafe fn configure_role(task: *mut Task, role: Role) {
    unsafe {
        let (priority, ticks, dispatches) = policy(role);
        let c = core(task);
        (*c).role = role;
        (*c).priority = priority;
        (*c).role_budget_ticks_remaining = ticks;
        (*c).role_budget_dispatches_remaining = dispatches;
        (*c).role_budget_exhausted_recorded = false;
    }
}
unsafe extern "C" fn ready_before(a: *const c_void, b: *const c_void) -> bool {
    unsafe { (*core(a.cast_mut().cast())).ready_order < (*core(b.cast_mut().cast())).ready_order }
}
unsafe extern "C" fn timeout_before(a: *const c_void, b: *const c_void) -> bool {
    unsafe {
        let a = core(a.cast_mut().cast());
        let b = core(b.cast_mut().cast());
        (*a).wake_tick < (*b).wake_tick
            || ((*a).wake_tick == (*b).wake_tick && (*a).timeout_sequence < (*b).timeout_sequence)
    }
}
const READY_OFFSET: usize = offset_of!(Task, core) + offset_of!(TaskCore, ready_index);
const TIMEOUT_OFFSET: usize = offset_of!(Task, core) + offset_of!(TaskCore, timeout_index);
unsafe fn adjust_rank(task: *mut Task, old: u8, new: u8) {
    unsafe {
        if !(*core(task)).ready_linked || old == new {
            return;
        }
        let q = queue_for(task);
        task_index::remove(
            ptr::addr_of_mut!((*q).ranks[(*core(task)).ready_rank as usize]),
            task.cast(),
            READY_OFFSET,
        );
        (*core(task)).ready_rank = new;
        task_index::insert(
            ptr::addr_of_mut!((*q).ranks[new as usize]),
            task.cast(),
            READY_OFFSET,
            ready_before,
        );
        if (old as usize) < ROLE_COUNT && (*q).dispatch_counts[old as usize] != 0 {
            (*q).dispatch_counts[old as usize] -= 1;
        }
        if (new as usize) < ROLE_COUNT {
            (*q).dispatch_counts[new as usize] += 1;
        }
    }
}
pub(super) unsafe fn reset_activation(task: *mut Task) {
    unsafe {
        let old = dispatch_rank(task);
        let role = (*core(task)).role;
        configure_role(task, role);
        (*core(task)).role_activations = (*core(task)).role_activations.wrapping_add(1);
        (*raw()).role_activations[role as usize] =
            (*raw()).role_activations[role as usize].wrapping_add(1);
        adjust_rank(task, old, dispatch_rank(task));
    }
}
unsafe fn role_exhausted(task: *mut Task) {
    unsafe {
        let c = core(task);
        if (*c).role_budget_exhausted_recorded || !high((*c).role) {
            return;
        }
        if (*c).role_budget_ticks_remaining != 0 && (*c).role_budget_dispatches_remaining != 0 {
            return;
        }
        (*c).role_budget_exhausted_recorded = true;
        (*c).role_budget_exhaustions = (*c).role_budget_exhaustions.wrapping_add(1);
        let role = (*c).role as usize;
        (*raw()).role_exhaustions[role] = (*raw()).role_exhaustions[role].wrapping_add(1);
    }
}
unsafe fn donation_exhausted(task: *mut Task) {
    unsafe {
        let c = core(task);
        if (*c).donation_budget_exhausted_recorded || inherited_rank(task) == NO_RANK {
            return;
        }
        if (*c).donation_budget_ticks_remaining != 0
            && (*c).donation_budget_dispatches_remaining != 0
        {
            return;
        }
        (*c).donation_budget_exhausted_recorded = true;
        (*c).donation_budget_exhaustions = (*c).donation_budget_exhaustions.wrapping_add(1);
        (*raw()).donation_exhaustions = (*raw()).donation_exhaustions.wrapping_add(1);
    }
}
pub(super) unsafe fn consume_dispatch(task: *mut Task) {
    unsafe {
        let old = dispatch_rank(task);
        let c = core(task);
        if high((*c).role) && (*c).role_budget_dispatches_remaining != 0 {
            (*c).role_budget_dispatches_remaining -= 1;
            role_exhausted(task);
        }
        if inherited_rank(task) != NO_RANK && (*c).donation_budget_dispatches_remaining != 0 {
            (*c).donation_budget_dispatches_remaining -= 1;
            donation_exhausted(task);
        }
        adjust_rank(task, old, dispatch_rank(task));
    }
}
pub(super) unsafe fn consume_run(task: *mut Task, elapsed: u64) {
    unsafe {
        if elapsed == 0 {
            return;
        }
        let c = core(task);
        if high((*c).role) && (*c).role_budget_ticks_remaining != 0 {
            (*c).role_budget_ticks_remaining -=
                elapsed.min(u64::from((*c).role_budget_ticks_remaining)) as u32;
            role_exhausted(task);
        }
        if inherited_rank(task) != NO_RANK && (*c).donation_budget_ticks_remaining != 0 {
            (*c).donation_budget_ticks_remaining -=
                elapsed.min(u64::from((*c).donation_budget_ticks_remaining)) as u32;
            donation_exhausted(task);
        }
    }
}
pub(super) unsafe fn link_ready(task: *mut Task) {
    unsafe {
        let c = core(task);
        if (*c).ready_linked {
            return;
        }
        (*c).ready_order = (*raw()).next_ready_order;
        (*raw()).next_ready_order += 1;
        append_ready(task);
    }
}
/// Caller has assigned ready_order. Migration uses this only for an empty
/// target queue, preserving both global ready age and per-rank ordering.
pub(super) unsafe fn append_ready(task: *mut Task) {
    unsafe {
        let c = core(task);
        assert!(!(*c).ready_linked);
        let q = queue_for(task);
        (*c).ready_rank = dispatch_rank(task);
        task_index::insert(
            ptr::addr_of_mut!((*q).ranks[(*c).ready_rank as usize]),
            task.cast(),
            READY_OFFSET,
            ready_before,
        );
        (*c).ready_prev = (*q).tail;
        (*c).ready_next = null_mut();
        if (*q).tail.is_null() {
            (*q).head = task;
        } else {
            (*core((*q).tail)).ready_next = task;
        }
        (*q).tail = task;
        (*c).ready_linked = true;
        if (*q).warning_cursor.is_null() {
            (*q).warning_cursor = task;
        }
        if (*q).balance_cursor.is_null() {
            (*q).balance_cursor = task;
        }
        (*q).count += 1;
        (*q).dispatch_counts[dispatch_rank(task) as usize] += 1;
    }
}
pub(super) unsafe fn unlink_ready(task: *mut Task) {
    unsafe {
        let c = core(task);
        if !(*c).ready_linked {
            return;
        }
        let q = queue_for(task);
        task_index::remove(
            ptr::addr_of_mut!((*q).ranks[(*c).ready_rank as usize]),
            task.cast(),
            READY_OFFSET,
        );
        if (*c).ready_prev.is_null() {
            (*q).head = (*c).ready_next;
        } else {
            (*core((*c).ready_prev)).ready_next = (*c).ready_next;
        }
        if (*c).ready_next.is_null() {
            (*q).tail = (*c).ready_prev;
        } else {
            (*core((*c).ready_next)).ready_prev = (*c).ready_prev;
        }
        if (*q).warning_cursor == task {
            (*q).warning_cursor = if (*c).ready_next.is_null() {
                (*q).head
            } else {
                (*c).ready_next
            };
        }
        if (*q).balance_cursor == task {
            (*q).balance_cursor = if (*c).ready_next.is_null() {
                (*q).head
            } else {
                (*c).ready_next
            };
        }
        (*c).ready_prev = null_mut();
        (*c).ready_next = null_mut();
        (*c).ready_linked = false;
        if (*q).count != 0 {
            (*q).count -= 1;
        }
        let rank = (*c).ready_rank as usize;
        if (*q).dispatch_counts[rank] != 0 {
            (*q).dispatch_counts[rank] -= 1;
        }
    }
}
pub(super) unsafe fn select_home(task: *mut Task) {
    unsafe {
        let c = core(task);
        if !(*c).smp_eligible {
            (*c).home_cpu = 0;
            (*c).home_cpu_bound = true;
            return;
        }
        if (*c).cpu_affinity_bound
            || ((*c).home_cpu_bound
                && percpu::r4os_rust_percpu_control(13, u32::from((*c).home_cpu), 0, 0) != 0)
        {
            return;
        }
        let mut loads = [0usize; MAX_CPUS];
        for (index, load) in loads.iter_mut().enumerate() {
            *load = (*raw()).ready[index].count
                + usize::from(percpu::r4os_rust_percpu_control(12, index as u32, 0, 0) != 0);
        }
        let mask = percpu::r4os_rust_percpu_control(14, 0, 0, 0);
        (*c).home_cpu =
            smp_policy::r4os_rust_smp_least_loaded(mask, loads.as_ptr(), loads.len()) as u8;
        (*c).home_cpu_bound = true;
    }
}
pub(super) unsafe fn link_timeout(task: *mut Task) {
    unsafe {
        let c = core(task);
        if (*c).timeout_linked || (*c).wake_tick == 0 {
            return;
        }
        if (*c).timeout_sequence == 0 {
            (*c).timeout_sequence = (*raw()).next_timeout_sequence;
            (*raw()).next_timeout_sequence += 1;
        }
        let neighbors = task_index::insert(
            ptr::addr_of_mut!((*raw()).timeout_index),
            task.cast(),
            TIMEOUT_OFFSET,
            timeout_before,
        );
        (*c).timeout_prev = neighbors.previous.cast();
        (*c).timeout_next = neighbors.following.cast();
        if neighbors.previous.is_null() {
            (*raw()).timeout_head = task;
        } else {
            (*core(neighbors.previous.cast())).timeout_next = task;
        }
        if neighbors.following.is_null() {
            (*raw()).timeout_tail = task;
        } else {
            (*core(neighbors.following.cast())).timeout_prev = task;
        }
        (*c).timeout_linked = true;
        (*raw()).timeout_count += 1;
    }
}
pub(super) unsafe fn unlink_timeout(task: *mut Task) {
    unsafe {
        let c = core(task);
        if !(*c).timeout_linked {
            return;
        }
        task_index::remove(
            ptr::addr_of_mut!((*raw()).timeout_index),
            task.cast(),
            TIMEOUT_OFFSET,
        );
        if (*c).timeout_prev.is_null() {
            (*raw()).timeout_head = (*c).timeout_next;
        } else {
            (*core((*c).timeout_prev)).timeout_next = (*c).timeout_next;
        }
        if (*c).timeout_next.is_null() {
            (*raw()).timeout_tail = (*c).timeout_prev;
        } else {
            (*core((*c).timeout_next)).timeout_prev = (*c).timeout_prev;
        }
        (*c).timeout_prev = null_mut();
        (*c).timeout_next = null_mut();
        (*c).timeout_linked = false;
        (*c).timeout_sequence = 0;
        if (*raw()).timeout_count != 0 {
            (*raw()).timeout_count -= 1;
        }
    }
}
pub(super) unsafe fn link_reap(task: *mut Task) {
    unsafe {
        let c = core(task);
        if (*c).reap_linked || !(*c).retire_pending || (*c).release_in_progress {
            return;
        }
        (*c).reap_prev = (*raw()).reap_tail;
        (*c).reap_next = null_mut();
        if (*raw()).reap_tail.is_null() {
            (*raw()).reap_head = task;
        } else {
            (*core((*raw()).reap_tail)).reap_next = task;
        }
        (*raw()).reap_tail = task;
        (*c).reap_linked = true;
        (*raw()).reap_count += 1;
    }
}
pub(super) unsafe fn unlink_reap(task: *mut Task) {
    unsafe {
        let c = core(task);
        if !(*c).reap_linked {
            return;
        }
        if (*c).reap_prev.is_null() {
            (*raw()).reap_head = (*c).reap_next;
        } else {
            (*core((*c).reap_prev)).reap_next = (*c).reap_next;
        }
        if (*c).reap_next.is_null() {
            (*raw()).reap_tail = (*c).reap_prev;
        } else {
            (*core((*c).reap_next)).reap_prev = (*c).reap_prev;
        }
        (*c).reap_prev = null_mut();
        (*c).reap_next = null_mut();
        (*c).reap_linked = false;
        if (*raw()).reap_count != 0 {
            (*raw()).reap_count -= 1;
        }
    }
}
pub(super) unsafe fn rotate_reap(task: *mut Task) {
    unsafe {
        if (*core(task)).reap_linked && (*raw()).reap_head == task && (*raw()).reap_tail != task {
            unlink_reap(task);
            link_reap(task);
        }
    }
}
pub(super) unsafe fn link_registry(task: *mut Task) {
    unsafe {
        let c = core(task);
        (*c).registry_prev = (*raw()).tail;
        (*c).registry_next = null_mut();
        if (*raw()).tail.is_null() {
            (*raw()).head = task;
        } else {
            (*core((*raw()).tail)).registry_next = task;
        }
        (*raw()).tail = task;
        (*raw()).count += 1;
        (*raw()).peak = (*raw()).peak.max((*raw()).count);
        if (*c).state == State::Ready {
            link_ready(task);
        }
        if (*c).state == State::Blocked && (*c).wake_tick != 0 {
            link_timeout(task);
        }
        if (*c).retire_pending && !(*c).release_in_progress {
            link_reap(task);
        }
        bump_epoch();
    }
}
pub(super) unsafe fn unlink_registry(task: *mut Task) {
    unsafe {
        unlink_ready(task);
        unlink_timeout(task);
        unlink_reap(task);
        let c = core(task);
        if (*c).registry_prev.is_null() {
            (*raw()).head = (*c).registry_next;
        } else {
            (*core((*c).registry_prev)).registry_next = (*c).registry_next;
        }
        if (*c).registry_next.is_null() {
            (*raw()).tail = (*c).registry_prev;
        } else {
            (*core((*c).registry_next)).registry_prev = (*c).registry_prev;
        }
        (*c).registry_prev = null_mut();
        (*c).registry_next = null_mut();
        if (*raw()).count != 0 {
            (*raw()).count -= 1;
        }
        bump_epoch();
    }
}

pub(super) unsafe fn mark_ready(task: *mut Task, now: u64) {
    unsafe {
        let c = core(task);
        if matches!((*c).state, State::Unused | State::Dead) {
            return;
        }
        let blocked = (*c).state == State::Blocked;
        unlink_timeout(task);
        (*c).state = State::Ready;
        (*c).running_cpu = 0xff;
        (*c).ready_since_tick = now;
        if blocked {
            reset_activation(task);
            select_home(task);
        }
        link_ready(task);
        if blocked {
            bump_epoch();
        }
    }
}
pub(super) unsafe fn mark_running(task: *mut Task) {
    unsafe {
        let c = core(task);
        if matches!((*c).state, State::Unused | State::Dead) {
            return;
        }
        let blocked = (*c).state == State::Blocked;
        unlink_ready(task);
        unlink_timeout(task);
        let cpu = percpu::current_index();
        assert!(!(*c).cpu_affinity_bound || u32::from((*c).home_cpu) == cpu);
        (*c).home_cpu = cpu as u8;
        (*c).home_cpu_bound = true;
        (*c).running_cpu = cpu as u8;
        percpu::r4os_rust_percpu_control(11, cpu, u64::from(!(*c).cpu_idle), 0);
        (*c).state = State::Running;
        (*c).ready_since_tick = 0;
        if blocked {
            bump_epoch();
        }
    }
}
pub(super) unsafe fn park_idle(task: *mut Task) {
    unsafe {
        let c = core(task);
        if !(*c).cpu_idle || matches!((*c).state, State::Dead | State::Unused) {
            return;
        }
        unlink_ready(task);
        unlink_timeout(task);
        (*c).state = State::Blocked;
        (*c).running_cpu = 0xff;
        (*c).ready_since_tick = 0;
    }
}
pub(super) unsafe fn switched_out(task: *mut Task) {
    unsafe {
        let c = core(task);
        if (*c).state == State::Running {
            return;
        }
        (*c).running_cpu = 0xff;
        if (*c).state == State::Ready && !(*c).ready_linked {
            select_home(task);
            link_ready(task);
        }
    }
}
pub(super) unsafe fn record_scheduled(task: *mut Task, now: u64) -> u64 {
    unsafe {
        let c = core(task);
        let latency = if (*c).ready_since_tick != 0 {
            now.saturating_sub((*c).ready_since_tick)
        } else {
            0
        };
        (*c).last_ready_latency_ticks = latency;
        (*c).max_ready_latency_ticks = (*c).max_ready_latency_ticks.max(latency);
        (*c).ready_since_tick = 0;
        (*c).last_scheduled_tick = now;
        (*c).last_accounted_tick = now;
        (*c).switches_in = (*c).switches_in.wrapping_add(1);
        consume_dispatch(task);
        latency
    }
}
pub(super) unsafe fn record_run_tick(task: *mut Task, now: u64) -> u64 {
    unsafe {
        let c = core(task);
        let elapsed = now.saturating_sub((*c).last_accounted_tick);
        (*c).run_ticks = (*c).run_ticks.saturating_add(elapsed);
        (*c).last_accounted_tick = now;
        consume_run(task, elapsed);
        let ticks = if (*c).last_scheduled_tick == 0 {
            0
        } else {
            now.saturating_sub((*c).last_scheduled_tick)
        };
        (*c).max_run_without_switch_ticks = (*c).max_run_without_switch_ticks.max(ticks);
        ticks
    }
}
pub(super) unsafe fn preempt_disable(task: *mut Task) -> u32 {
    unsafe {
        let c = core(task);
        (*c).preempt_disable_depth = (*c).preempt_disable_depth.saturating_add(1);
        (*c).preempt_disable_max_depth = (*c)
            .preempt_disable_max_depth
            .max((*c).preempt_disable_depth);
        (*c).preempt_disable_depth
    }
}
pub(super) unsafe fn preempt_enable(task: *mut Task) -> bool {
    unsafe {
        let c = core(task);
        if (*c).preempt_disable_depth == 0 {
            return false;
        }
        (*c).preempt_disable_depth -= 1;
        true
    }
}
pub(super) unsafe fn begin_wait(
    task: *mut Task,
    wake: u64,
    now: u64,
    reason: super::task_types::Bytes,
    object: u64,
) {
    unsafe {
        unlink_ready(task);
        unlink_timeout(task);
        let c = core(task);
        (*c).wake_tick = wake;
        (*c).blocked_since_tick = now;
        (*c).wait_reason = reason;
        (*c).wait_object = object;
        (*c).wait_result = super::task_types::WaitResult::None;
        (*c).ready_since_tick = 0;
        (*c).state = State::Blocked;
        link_timeout(task);
        bump_epoch();
    }
}
pub(super) unsafe fn finish_wait(
    task: *mut Task,
    now: u64,
    result: super::task_types::WaitResult,
) -> u64 {
    unsafe {
        let c = core(task);
        crate::sched::wait_node::detach(ptr::addr_of_mut!((*c).wait_node));
        let ticks = if (*c).blocked_since_tick == 0 {
            0
        } else {
            now.saturating_sub((*c).blocked_since_tick)
        };
        (*c).last_wait_ticks = ticks;
        (*c).max_wait_ticks = (*c).max_wait_ticks.max(ticks);
        unlink_timeout(task);
        (*c).state = State::Ready;
        (*c).ready_since_tick = now;
        (*c).wake_tick = 0;
        (*c).blocked_since_tick = 0;
        (*c).wait_reason = super::task_types::Bytes::default();
        (*c).wait_object = 0;
        (*c).wait_result = result;
        reset_activation(task);
        if (*c).running_cpu == 0xff {
            select_home(task);
            link_ready(task);
        } else {
            (*c).home_cpu = (*c).running_cpu;
        }
        bump_epoch();
        ticks
    }
}
pub(super) unsafe fn transition_dead(
    task: *mut Task,
    result: super::task_types::WaitResult,
    retire: bool,
) {
    unsafe {
        let c = core(task);
        crate::sched::wait_node::detach(ptr::addr_of_mut!((*c).wait_node));
        unlink_ready(task);
        unlink_timeout(task);
        (*c).state = State::Dead;
        // The exact executing CPU remains bound until switched_out().
        (*c).ready_since_tick = 0;
        (*c).wake_tick = 0;
        (*c).blocked_since_tick = 0;
        (*c).wait_reason = super::task_types::Bytes::default();
        (*c).wait_object = 0;
        (*c).wait_result = result;
        if retire {
            (*c).retire_pending = true;
            link_reap(task);
        }
        bump_epoch();
    }
}
pub(crate) unsafe fn donate(id: u32, generation: u64, rank: u8) -> bool {
    unsafe {
        if rank as usize >= ROLE_COUNT {
            return false;
        }
        (*raw()).donation_requests = (*raw()).donation_requests.wrapping_add(1);
        let target = find_identity(id, generation);
        if target.is_null() || matches!((*core(target)).state, State::Unused | State::Dead) {
            return false;
        }
        let c = core(target);
        let old = dispatch_rank(target);
        let best = inherited_rank(target);
        if (*c).inherited_rank_counts[rank as usize] == u16::MAX {
            return false;
        }
        (*c).inherited_rank_counts[rank as usize] += 1;
        if best == NO_RANK || rank < best {
            (*c).donation_budget_ticks_remaining = 4;
            (*c).donation_budget_dispatches_remaining = 4;
            (*c).donation_budget_exhausted_recorded = false;
        }
        (*raw()).donation_applied = (*raw()).donation_applied.wrapping_add(1);
        adjust_rank(target, old, dispatch_rank(target));
        true
    }
}
pub(crate) unsafe fn remove_donation(task: *mut Task, rank: u8) -> bool {
    unsafe {
        if rank as usize >= ROLE_COUNT || !contains(task) || (*core(task)).state == State::Unused {
            return false;
        }
        let c = core(task);
        if (*c).inherited_rank_counts[rank as usize] == 0 {
            return false;
        }
        let old = dispatch_rank(task);
        (*c).inherited_rank_counts[rank as usize] -= 1;
        if inherited_rank(task) == NO_RANK {
            (*c).donation_budget_ticks_remaining = 0;
            (*c).donation_budget_dispatches_remaining = 0;
            (*c).donation_budget_exhausted_recorded = false;
        }
        (*raw()).donation_released = (*raw()).donation_released.wrapping_add(1);
        adjust_rank(task, old, dispatch_rank(task));
        true
    }
}
pub(super) unsafe fn assign_role(task: *mut Task, role: Role) -> bool {
    unsafe {
        if !contains(task) || matches!((*core(task)).state, State::Unused | State::Dead) {
            return false;
        }
        if (*core(task)).role != role {
            let old = dispatch_rank(task);
            configure_role(task, role);
            adjust_rank(task, old, dispatch_rank(task));
        }
        true
    }
}
pub(super) unsafe fn enqueue_rollback(task: *mut Task) {
    unsafe {
        let c = core(task);
        (*c).release_in_progress = false;
        if !(*c).rollback_pending {
            (*c).rollback_pending = true;
            (*c).rollback_next = (*raw()).rollback_head;
            (*raw()).rollback_head = task;
            (*raw()).rollback_count += 1;
        }
    }
}
pub(super) unsafe fn unlink_rollback(task: *mut Task) -> bool {
    unsafe {
        let mut previous: *mut Task = null_mut();
        let mut cursor = (*raw()).rollback_head;
        while !cursor.is_null() {
            if cursor == task {
                let c = core(cursor);
                if previous.is_null() {
                    (*raw()).rollback_head = (*c).rollback_next;
                } else {
                    (*core(previous)).rollback_next = (*c).rollback_next;
                }
                (*c).rollback_next = null_mut();
                (*c).rollback_pending = false;
                if (*raw()).rollback_count != 0 {
                    (*raw()).rollback_count -= 1;
                }
                return true;
            }
            previous = cursor;
            cursor = (*core(cursor)).rollback_next;
        }
        false
    }
}
