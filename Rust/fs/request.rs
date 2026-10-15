//! Original bounded filesystem requests, mutation lanes and diagnostics.
//! Only native sync owns the exact wait-spanning guards. Metadata snapshots
//! use independent atomic scalars, never borrow mutable state across a wait.
use super::request_scope::{Plan, drive_code, r4os_rust_fs_scope_plan};
use crate::{
    arch::pit::DEFAULT_HZ,
    platform::timer,
    sched::{
        scheduler, sync_locks,
        sync_types::{Text, UnwindGuard, WaitQueue},
        task, task_context,
        wait_node::QueueCore,
    },
    storage::{
        access_runtime::{r4os_rust_storage_access_current, r4os_rust_storage_access_runtime},
        access_state::{Input, Owner, Reference},
    },
};
use core::{
    cell::UnsafeCell,
    mem::{MaybeUninit, size_of},
    ptr,
    sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering::Relaxed},
};
const LANES: usize = 26;
const SLICE_LIMIT: u32 = 12;
const SLICE_TICKS: u64 = 5 * DEFAULT_HZ as u64;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Summary {
    pub requests: u64,
    pub completed: u64,
    pub failed: u64,
    pub read_requests: u64,
    pub write_requests: u64,
    pub metadata_requests: u64,
    pub stream_requests: u64,
    pub lock_acquires: u64,
    pub lock_contention_waits: u64,
    pub lock_timeouts: u64,
    pub boot_bypass: u64,
    pub total_ticks: u64,
    pub max_ticks: u64,
    pub last_ticks: u64,
    pub active_kind: u32,
    pub last_kind: u32,
    pub active_drive: u32,
    pub last_drive: u32,
    pub active_phase: u32,
    pub active_progress_sequence: u32,
    pub active_progress: u64,
    pub active_progress_total: u64,
    pub single_drive_requests: u64,
    pub cross_drive_requests: u64,
    pub global_requests: u64,
    pub active_requests: u32,
    pub parallel_active_max: u32,
}
struct Counters {
    requests: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    read_requests: AtomicU64,
    write_requests: AtomicU64,
    metadata_requests: AtomicU64,
    stream_requests: AtomicU64,
    lock_acquires: AtomicU64,
    lock_contention_waits: AtomicU64,
    lock_timeouts: AtomicU64,
    boot_bypass: AtomicU64,
    total_ticks: AtomicU64,
    max_ticks: AtomicU64,
    last_ticks: AtomicU64,
    active_kind: AtomicU32,
    last_kind: AtomicU32,
    active_drive: AtomicU32,
    last_drive: AtomicU32,
    active_phase: AtomicU32,
    active_progress_sequence: AtomicU32,
    active_progress: AtomicU64,
    active_progress_total: AtomicU64,
    single_drive_requests: AtomicU64,
    cross_drive_requests: AtomicU64,
    global_requests: AtomicU64,
    active_requests: AtomicU32,
    parallel_active_max: AtomicU32,
}
impl Counters {
    const fn new() -> Self {
        Self {
            requests: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            read_requests: AtomicU64::new(0),
            write_requests: AtomicU64::new(0),
            metadata_requests: AtomicU64::new(0),
            stream_requests: AtomicU64::new(0),
            lock_acquires: AtomicU64::new(0),
            lock_contention_waits: AtomicU64::new(0),
            lock_timeouts: AtomicU64::new(0),
            boot_bypass: AtomicU64::new(0),
            total_ticks: AtomicU64::new(0),
            max_ticks: AtomicU64::new(0),
            last_ticks: AtomicU64::new(0),
            active_kind: AtomicU32::new(0),
            last_kind: AtomicU32::new(0),
            active_drive: AtomicU32::new(0),
            last_drive: AtomicU32::new(0),
            active_phase: AtomicU32::new(0),
            active_progress_sequence: AtomicU32::new(0),
            active_progress: AtomicU64::new(0),
            active_progress_total: AtomicU64::new(0),
            single_drive_requests: AtomicU64::new(0),
            cross_drive_requests: AtomicU64::new(0),
            global_requests: AtomicU64::new(0),
            active_requests: AtomicU32::new(0),
            parallel_active_max: AtomicU32::new(0),
        }
    }
    fn clear(&self) {
        self.requests.store(0, Relaxed);
        self.completed.store(0, Relaxed);
        self.failed.store(0, Relaxed);
        self.read_requests.store(0, Relaxed);
        self.write_requests.store(0, Relaxed);
        self.metadata_requests.store(0, Relaxed);
        self.stream_requests.store(0, Relaxed);
        self.lock_acquires.store(0, Relaxed);
        self.lock_contention_waits.store(0, Relaxed);
        self.lock_timeouts.store(0, Relaxed);
        self.boot_bypass.store(0, Relaxed);
        self.total_ticks.store(0, Relaxed);
        self.max_ticks.store(0, Relaxed);
        self.last_ticks.store(0, Relaxed);
        self.active_kind.store(0, Relaxed);
        self.last_kind.store(0, Relaxed);
        self.active_drive.store(0, Relaxed);
        self.last_drive.store(0, Relaxed);
        self.active_phase.store(0, Relaxed);
        self.active_progress_sequence.store(0, Relaxed);
        self.active_progress.store(0, Relaxed);
        self.active_progress_total.store(0, Relaxed);
        self.single_drive_requests.store(0, Relaxed);
        self.cross_drive_requests.store(0, Relaxed);
        self.global_requests.store(0, Relaxed);
        self.active_requests.store(0, Relaxed);
        self.parallel_active_max.store(0, Relaxed);
    }
    fn snapshot(&self) -> Summary {
        Summary {
            requests: self.requests.load(Relaxed),
            completed: self.completed.load(Relaxed),
            failed: self.failed.load(Relaxed),
            read_requests: self.read_requests.load(Relaxed),
            write_requests: self.write_requests.load(Relaxed),
            metadata_requests: self.metadata_requests.load(Relaxed),
            stream_requests: self.stream_requests.load(Relaxed),
            lock_acquires: self.lock_acquires.load(Relaxed),
            lock_contention_waits: self.lock_contention_waits.load(Relaxed),
            lock_timeouts: self.lock_timeouts.load(Relaxed),
            boot_bypass: self.boot_bypass.load(Relaxed),
            total_ticks: self.total_ticks.load(Relaxed),
            max_ticks: self.max_ticks.load(Relaxed),
            last_ticks: self.last_ticks.load(Relaxed),
            active_kind: self.active_kind.load(Relaxed),
            last_kind: self.last_kind.load(Relaxed),
            active_drive: self.active_drive.load(Relaxed),
            last_drive: self.last_drive.load(Relaxed),
            active_phase: self.active_phase.load(Relaxed),
            active_progress_sequence: self.active_progress_sequence.load(Relaxed),
            active_progress: self.active_progress.load(Relaxed),
            active_progress_total: self.active_progress_total.load(Relaxed),
            single_drive_requests: self.single_drive_requests.load(Relaxed),
            cross_drive_requests: self.cross_drive_requests.load(Relaxed),
            global_requests: self.global_requests.load(Relaxed),
            active_requests: self.active_requests.load(Relaxed),
            parallel_active_max: self.parallel_active_max.load(Relaxed),
        }
    }
}
static STATS: Counters = Counters::new();

struct Lane {
    kind: AtomicU32,
    drive: AtomicU8,
    active_depth: AtomicU32,
    phase: AtomicU32,
    sequence: AtomicU32,
    progress: AtomicU64,
    total: AtomicU64,
}
impl Lane {
    const fn new() -> Self {
        Self {
            kind: AtomicU32::new(12),
            drive: AtomicU8::new(0),
            active_depth: AtomicU32::new(0),
            phase: AtomicU32::new(0),
            sequence: AtomicU32::new(0),
            progress: AtomicU64::new(0),
            total: AtomicU64::new(0),
        }
    }
    fn clear(&self) {
        self.kind.store(12, Relaxed);
        self.drive.store(0, Relaxed);
        self.active_depth.store(0, Relaxed);
        self.phase.store(0, Relaxed);
        self.sequence.store(0, Relaxed);
        self.progress.store(0, Relaxed);
        self.total.store(0, Relaxed);
    }
}
static STATES: [Lane; LANES] = [const { Lane::new() }; LANES];
static MUTATION_PROGRESS: [AtomicU64; LANES] = [const { AtomicU64::new(0) }; LANES];
const fn guard(name: &'static [u8]) -> UnwindGuard {
    UnwindGuard {
        owner: 0,
        owner_generation: 0,
        boot_owner: false,
        depth: 0,
        name: Text::literal(name),
        queue: WaitQueue {
            core: QueueCore {
                head: ptr::null_mut(),
                tail: ptr::null_mut(),
                count: 0,
                closing: false,
                generation: 1,
            },
        },
    }
}
struct Gates(UnsafeCell<[UnwindGuard; LANES]>);
// SAFETY: Native sync serializes every guard mutation/owner snapshot under
// its existing runtime owner. Storage is resident and stable through waits.
unsafe impl Sync for Gates {}
static BACKEND: Gates = Gates(UnsafeCell::new([const { guard(b"fs-drive") }; LANES]));
static MUTATION: Gates = Gates(UnsafeCell::new([const { guard(b"fs-mutation") }; LANES]));
fn gate_ptr(lane: usize, mutation: bool) -> *mut UnwindGuard {
    assert!(lane < LANES);
    let table = if mutation { &MUTATION } else { &BACKEND };
    unsafe { table.0.get().cast::<UnwindGuard>().add(lane) }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BoundMount {
    pub reference: Reference,
    pub present: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Use {
    pub id: u64,
    pub owner: Owner,
    pub present: bool,
}
impl Use {
    const EMPTY: Self = Self {
        id: 0,
        owner: Owner {
            task: 0,
            task_generation: 0,
            program: 0,
            program_generation: 0,
        },
        present: false,
    };
}
#[repr(C)]
pub struct Guard {
    pub kind: u32,
    pub drive: u8,
    pub second_drive: u8,
    pub lanes: [u8; LANES],
    pub lane_count: u8,
    pub gates_locked: bool,
    pub mutations_locked: bool,
    pub lanes_active: bool,
    pub start_tick: u64,
    pub active: bool,
    pub uses: [Use; LANES],
    pub unwind: task_context::Unwind,
}
#[repr(C)]
pub struct GateSnapshot {
    pub owner_task_id: u32,
    pub owner_task_generation: u64,
    pub kind: u32,
    pub drive: u8,
    pub depth: u32,
    pub active_depth: u32,
}
#[repr(C)]
pub struct Stall {
    pub slices: u32,
    pub owner: u32,
    pub depth: u32,
    pub kind: u32,
    pub drive: u32,
    pub phase: u32,
    pub progress: u64,
    pub total: u64,
}
unsafe extern "C" {
    // 0 = absent volume, 1 = volume with exact mounted reference,
    // 2 = existing volume without an admitted mounted reference.
    fn r4os_zig_fs_request_volume(letter: u8, output: *mut Reference) -> u8;
    fn r4os_zig_fs_request_stall(snapshot: *const Stall);
}
fn read_only(kind: u32) -> bool {
    matches!(kind, 1 | 2 | 3 | 10 | 11 | 12 | 19 | 20)
}
unsafe fn gate_op(lane: u8, mutation: bool, op: u8, timeout: u64) -> bool {
    unsafe { sync_locks::r4os_rust_sync_guard(gate_ptr(usize::from(lane), mutation), op, timeout) }
}
unsafe fn begin_use(reference: Reference) -> Option<Use> {
    let mut owner = Owner::default();
    if !unsafe { r4os_rust_storage_access_current(&mut owner) } {
        return None;
    }
    let input = Input {
        reference,
        owner,
        kind: 0,
        ..Input::default()
    };
    let mut id = 0;
    if unsafe { r4os_rust_storage_access_runtime(10, false, &input, &mut id, ptr::null_mut()) } != 0
    {
        return None;
    }
    Some(Use {
        id,
        owner,
        present: true,
    })
}
unsafe fn end_use(token: Use) {
    let input = Input {
        id: token.id,
        owner: token.owner,
        ..Input::default()
    };
    let mut ignored = 0;
    let _ = unsafe {
        r4os_rust_storage_access_runtime(12, false, &input, &mut ignored, ptr::null_mut())
    };
}
unsafe fn release_plan(plan: &Plan, mut acquired: u8) {
    while acquired != 0 {
        acquired -= 1;
        let _ = unsafe { gate_op(plan.lanes[usize::from(acquired)], false, 3, 0) };
    }
}
unsafe fn cancel_admission(
    plan: &Plan,
    mut mutations: u8,
    uses: &[Use; LANES],
    unwind: task_context::Unwind,
) {
    while mutations != 0 {
        mutations -= 1;
        let _ = unsafe { gate_op(plan.lanes[usize::from(mutations)], true, 3, 0) };
    }
    for token in uses {
        if token.present {
            unsafe { end_use(*token) };
        }
    }
    let _ = unsafe { task_context::leave(unwind) };
}
unsafe fn acquire_gate(lane: u8, mutation: bool) -> bool {
    if scheduler::current().is_null() {
        return false;
    }
    if unsafe { gate_op(lane, mutation, 1, 0) } {
        return true;
    }
    STATS.lock_contention_waits.fetch_add(1, Relaxed);
    let index = usize::from(lane);
    let progress = || {
        if mutation {
            MUTATION_PROGRESS[index].load(Relaxed)
        } else {
            u64::from(STATES[index].sequence.load(Relaxed))
        }
    };
    let mut sequence = progress();
    let mut slices = 0u32;
    loop {
        if unsafe { gate_op(lane, mutation, 2, SLICE_TICKS) } {
            return true;
        }
        let observed = progress();
        if observed != sequence {
            sequence = observed;
            slices = 0;
            continue;
        }
        slices += 1;
        if slices == 1 || slices.is_power_of_two() || slices == SLICE_LIMIT {
            let owner = unsafe { sync_locks::guard_metadata(gate_ptr(index, mutation)) };
            let state = &STATES[index];
            let snapshot = Stall {
                slices,
                owner: owner.owner,
                depth: owner.depth,
                kind: state.kind.load(Relaxed),
                drive: drive_code(state.drive.load(Relaxed)),
                phase: state.phase.load(Relaxed),
                progress: state.progress.load(Relaxed),
                total: state.total.load(Relaxed),
            };
            unsafe { r4os_zig_fs_request_stall(&snapshot) };
        }
        if slices >= SLICE_LIMIT {
            return false;
        }
    }
}

/// # Safety
/// Original boot-only reset before any admitted request, use, waiter or guard.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_init() {
    for index in 0..LANES {
        unsafe {
            gate_ptr(index, false).write(guard(b"fs-drive"));
            gate_ptr(index, true).write(guard(b"fs-mutation"));
        }
        STATES[index].clear();
        MUTATION_PROGRESS[index].store(0, Relaxed);
    }
    STATS.clear();
}

/// # Safety
/// Exclusive aligned output is resident for this bounded passive snapshot.
/// Atomic metadata is advisory, retaining the original first-active-lane view.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_summary(output: *mut Summary) {
    let mut out = STATS.snapshot();
    out.active_kind = 0;
    out.active_drive = 0;
    out.active_phase = 0;
    out.active_progress = 0;
    out.active_progress_total = 0;
    for state in &STATES {
        if state.active_depth.load(Relaxed) == 0 {
            continue;
        }
        if out.active_kind == 0 {
            out.active_kind = state.kind.load(Relaxed);
            out.active_drive = drive_code(state.drive.load(Relaxed));
            out.active_phase = state.phase.load(Relaxed);
            out.active_progress_sequence = state.sequence.load(Relaxed);
            out.active_progress = state.progress.load(Relaxed);
            out.active_progress_total = state.total.load(Relaxed);
        }
    }
    unsafe { output.write(out) };
}

/// # Safety
/// Exclusive resident output is published only for one exact occupied lane.
/// Sync owns coherent holder snapshots; no Task pointer escapes this query.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_gate(letter: u8, output: *mut GateSnapshot) -> bool {
    let mut plan = MaybeUninit::<Plan>::uninit();
    unsafe { r4os_rust_fs_scope_plan(0, letter, 0, plan.as_mut_ptr()) };
    let plan = unsafe { plan.assume_init() };
    if plan.count != 1 {
        return false;
    }
    let index = usize::from(plan.lanes[0]);
    let first = unsafe { sync_locks::guard_metadata(gate_ptr(index, false)) };
    if first.owner == 0 || first.generation == 0 || first.depth == 0 || first.boot_owner {
        return false;
    }
    let state = &STATES[index];
    let result = GateSnapshot {
        owner_task_id: first.owner,
        owner_task_generation: first.generation,
        kind: state.kind.load(Relaxed),
        drive: state.drive.load(Relaxed),
        depth: first.depth,
        active_depth: state.active_depth.load(Relaxed),
    };
    let second = unsafe { sync_locks::guard_metadata(gate_ptr(index, false)) };
    if second.owner != first.owner
        || second.generation != first.generation
        || second.depth != first.depth
    {
        return false;
    }
    unsafe { output.write(result) };
    true
}

/// # Safety
/// Kind is an original valid Kind (1..25); selection0/1 means single/pair.
/// Bound references, when supplied, are initialized resident records disjoint
/// from output. The caller preserves the exact volume/use identity throughout
/// admission and backend work. Output is exclusive resident guard storage.
/// No borrowed table, metadata, gate or caller output reference spans a wait.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_begin(
    kind: u32,
    drive: u8,
    second_drive: u8,
    selection: u8,
    immediate: bool,
    refs: *const BoundMount,
    ref_count: usize,
    has_bound: bool,
    output: *mut Guard,
) -> bool {
    assert!((1..=25).contains(&kind));
    // Original beginVolume/beginPairVolumes reject a missing mount reference
    // before entering unwind admission. The facade only projects the copied
    // Volume value's immutable mount_ref; it owns no admission decision.
    if has_bound && ref_count <= LANES && !scheduler::current().is_null() {
        for index in 0..ref_count {
            if !unsafe { refs.add(index).read() }.present {
                return false;
            }
        }
    }
    let mut plan = MaybeUninit::<Plan>::uninit();
    unsafe { r4os_rust_fs_scope_plan(selection, drive, second_drive, plan.as_mut_ptr()) };
    let plan = unsafe { plan.assume_init() };
    let unwind = unsafe { task_context::enter() };
    if !unwind.admitted() {
        return false;
    }
    let mut uses = [Use::EMPTY; LANES];
    let mut mutations = 0u8;
    let runtime_owned = !scheduler::current().is_null();
    if runtime_owned {
        if has_bound {
            if ref_count > LANES {
                unsafe { cancel_admission(&plan, mutations, &uses, unwind) };
                return false;
            }
            for index in 0..ref_count {
                let bound = unsafe { refs.add(index).read() };
                if bound.present {
                    let Some(token) = (unsafe { begin_use(bound.reference) }) else {
                        unsafe { cancel_admission(&plan, mutations, &uses, unwind) };
                        return false;
                    };
                    uses[index] = token;
                }
            }
        } else {
            for index in 0..usize::from(plan.count) {
                let mut reference = Reference::default();
                let result =
                    unsafe { r4os_zig_fs_request_volume(b'A' + plan.lanes[index], &mut reference) };
                if result == 0 {
                    continue;
                }
                if result != 1 {
                    unsafe { cancel_admission(&plan, mutations, &uses, unwind) };
                    return false;
                }
                let Some(token) = (unsafe { begin_use(reference) }) else {
                    unsafe { cancel_admission(&plan, mutations, &uses, unwind) };
                    return false;
                };
                uses[index] = token;
            }
        }
        if !read_only(kind) {
            let mut nested_backend = false;
            for index in 0..LANES {
                if unsafe { gate_op(index as u8, false, 0, 0) } {
                    nested_backend = true;
                    break;
                }
            }
            while mutations < plan.count {
                let lane = plan.lanes[usize::from(mutations)];
                let admitted = if immediate || nested_backend {
                    unsafe { gate_op(lane, true, 1, 0) }
                } else {
                    unsafe { acquire_gate(lane, true) }
                };
                if !admitted {
                    unsafe { cancel_admission(&plan, mutations, &uses, unwind) };
                    return false;
                }
                mutations += 1;
            }
        }
        let mut acquired = 0u8;
        while acquired < plan.count {
            let lane = plan.lanes[usize::from(acquired)];
            let admitted = if immediate {
                unsafe { gate_op(lane, false, 1, 0) }
            } else {
                unsafe { acquire_gate(lane, false) }
            };
            if !admitted {
                if !immediate {
                    STATS.lock_timeouts.fetch_add(1, Relaxed);
                }
                unsafe {
                    release_plan(&plan, acquired);
                    cancel_admission(&plan, mutations, &uses, unwind);
                }
                return false;
            }
            acquired += 1;
        }
        STATS.lock_acquires.fetch_add(1, Relaxed);
    } else {
        STATS.boot_bypass.fetch_add(1, Relaxed);
    }
    STATS.requests.fetch_add(1, Relaxed);
    match kind {
        2 | 3 | 19 | 20 => {
            STATS.read_requests.fetch_add(1, Relaxed);
        }
        4 | 22 | 5 | 13 | 16 | 17 | 18 | 23 | 24 | 25 | 21 => {
            STATS.write_requests.fetch_add(1, Relaxed);
        }
        6..=9 => {
            STATS.stream_requests.fetch_add(1, Relaxed);
        }
        _ => {
            STATS.metadata_requests.fetch_add(1, Relaxed);
        }
    }
    if usize::from(plan.count) == LANES {
        STATS.global_requests.fetch_add(1, Relaxed);
    } else if plan.count == 2 {
        STATS.cross_drive_requests.fetch_add(1, Relaxed);
    } else {
        STATS.single_drive_requests.fetch_add(1, Relaxed);
    }
    let active = STATS
        .active_requests
        .try_update(Relaxed, Relaxed, |value| Some(value.saturating_add(1)))
        .unwrap()
        .saturating_add(1);
    STATS.parallel_active_max.fetch_max(active, Relaxed);
    for offset in 0..usize::from(plan.count) {
        let lane = plan.lanes[offset];
        let state = &STATES[usize::from(lane)];
        state.kind.store(kind, Relaxed);
        state.drive.store(b'A' + lane, Relaxed);
        let _ = state
            .active_depth
            .try_update(Relaxed, Relaxed, |value| Some(value.saturating_add(1)));
        state.phase.store(0, Relaxed);
        state.progress.store(0, Relaxed);
        state.total.store(0, Relaxed);
        state.sequence.fetch_add(1, Relaxed);
    }
    let result = Guard {
        kind,
        drive,
        second_drive,
        lanes: plan.lanes,
        lane_count: plan.count,
        gates_locked: runtime_owned,
        mutations_locked: runtime_owned && !read_only(kind),
        lanes_active: true,
        start_tick: timer::tick_count(),
        active: true,
        uses,
        unwind,
    };
    unsafe { output.write(result) };
    true
}

unsafe fn finish_lanes(guard: *mut Guard) {
    unsafe {
        if !(*guard).lanes_active {
            return;
        }
        let mut remaining = (*guard).lane_count;
        while remaining != 0 {
            remaining -= 1;
            let lane = (*guard).lanes[usize::from(remaining)];
            let state = &STATES[usize::from(lane)];
            let old = state
                .active_depth
                .try_update(Relaxed, Relaxed, |value| Some(value.saturating_sub(1)))
                .unwrap();
            if old <= 1 {
                state.phase.store(0, Relaxed);
                state.progress.store(0, Relaxed);
                state.total.store(0, Relaxed);
                state.sequence.fetch_add(1, Relaxed);
            }
            if (*guard).gates_locked {
                let _ = gate_op(lane, false, 3, 0);
            }
        }
        (*guard).lanes_active = false;
        (*guard).gates_locked = false;
    }
}

/// # Safety
/// Exact initialized exclusive guard retains its original task/use/unwind
/// identity until this explicit terminal release. No second completion owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_finish(guard: *mut Guard, ok: bool) {
    unsafe {
        if !(*guard).active {
            return;
        }
        let elapsed = timer::tick_count().saturating_sub((*guard).start_tick);
        STATS.completed.fetch_add(1, Relaxed);
        if !ok {
            STATS.failed.fetch_add(1, Relaxed);
        }
        STATS.last_ticks.store(elapsed, Relaxed);
        STATS.total_ticks.fetch_add(elapsed, Relaxed);
        STATS.max_ticks.fetch_max(elapsed, Relaxed);
        STATS.last_kind.store((*guard).kind, Relaxed);
        STATS.last_drive.store(drive_code((*guard).drive), Relaxed);
        let _ = STATS
            .active_requests
            .try_update(Relaxed, Relaxed, |value| Some(value.saturating_sub(1)));
        STATS.active_progress_sequence.fetch_add(1, Relaxed);
        finish_lanes(guard);
        if (*guard).mutations_locked {
            let mut remaining = (*guard).lane_count;
            while remaining != 0 {
                remaining -= 1;
                let _ = gate_op((*guard).lanes[usize::from(remaining)], true, 3, 0);
            }
            (*guard).mutations_locked = false;
        }
        (*guard).active = false;
        let uses = ptr::addr_of_mut!((*guard).uses).cast::<Use>();
        for index in 0..LANES {
            let token = uses.add(index).read();
            if token.present {
                end_use(token);
                (*uses.add(index)).present = false;
            }
        }
        let _ = task_context::leave((*guard).unwind);
    }
}

/// # Safety
/// Guard is the current caller's exact active copy/move request. Only its
/// backend borrow is returned; mounted uses, mutation and unwind stay retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_readers(guard: *mut Guard) -> bool {
    unsafe {
        if !(*guard).active
            || !(*guard).mutations_locked
            || !(*guard).gates_locked
            || !matches!((*guard).kind, 17 | 18)
        {
            return false;
        }
        for index in 0..usize::from((*guard).lane_count) {
            let lane = (*guard).lanes[index];
            if !gate_op(lane, true, 0, 0)
                || !gate_op(lane, false, 0, 0)
                || sync_locks::guard_metadata(gate_ptr(usize::from(lane), false)).depth != 1
            {
                return false;
            }
        }
        finish_lanes(guard);
        true
    }
}

/// # Safety
/// Readable exact guard remains owned by the caller, no concurrent finish.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_copy_progress(guard: *const Guard) {
    unsafe {
        if !(*guard).mutations_locked {
            return;
        }
        for index in 0..usize::from((*guard).lane_count) {
            MUTATION_PROGRESS[usize::from((*guard).lanes[index])].fetch_add(1, Relaxed);
        }
    }
}

/// # Safety
/// Original current task context; phase is one original AtomicProgressPhase.
/// Notification updates only a lane owned by this task's checked sync owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_request_atomic_progress(
    phase: u32,
    completed: u64,
    total: u64,
) {
    let current = scheduler::current();
    if current.is_null() {
        return;
    }
    let (id, _) = unsafe { task::immutable_identity(current) };
    for index in 0..LANES {
        let owner = unsafe { sync_locks::guard_metadata(gate_ptr(index, false)) };
        if owner.owner != id {
            continue;
        }
        let state = &STATES[index];
        if state.active_depth.load(Relaxed) == 0 || state.kind.load(Relaxed) != 25 {
            continue;
        }
        state.phase.store(phase, Relaxed);
        state.progress.store(completed, Relaxed);
        state.total.store(total, Relaxed);
        state.sequence.fetch_add(1, Relaxed);
        MUTATION_PROGRESS[index].fetch_add(1, Relaxed);
        STATS.active_progress_sequence.fetch_add(1, Relaxed);
    }
}

const _: () = {
    assert!(size_of::<Summary>() == 184);
    assert!(size_of::<BoundMount>() == 24);
    assert!(size_of::<Use>() == 48);
    assert!(size_of::<Guard>() == 1320);
    assert!(size_of::<GateSnapshot>() == 32);
    assert!(size_of::<Stall>() == 40);
};
