const virt = @import("../memory/virt.zig");
const heap = @import("../memory/heap.zig");
const diag_screen = @import("../kernel/diag_screen.zig");
const k = @import("../kernel/log.zig");
const timer = @import("../kernel/timer.zig");
const fpu = @import("../arch/x86_64/fpu.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const task_context = @import("task_context.zig");
const wait_node = @import("wait_node.zig");

// 0.56.15: 32K -> 64K. Der Guard-Wachhund hat unter Gate-FTP-Last einen
// ECHTEN net-rx-Overflow gefangen (STACK GUARD HIT task=net-rx,
// rip=net.core.sendTcpForConnection+0x24, cr2 72 Bytes unter stack_base) -
// 32K reichen dem TCP-Sendepfad nicht. Mit Heap-Stacks lief derselbe
// Overflow still in den Nachbar-Heap (Kandidat fuer die historischen
// rip=0-Crashes aus 0.56.9).
const STACK_SIZE: usize = 64 * 1024;
// 0.56.15: Kernel-Task-Stacks kommen aus dem kernel_stack-Fenster mit
// uncommittetem Guard-Bereich am unteren Ende. Ein Overflow trifft den
// Guard und erzeugt einen sauberen Page-Fault (IST-gestuetzt) mit Crash-
// Report statt stiller Korruption. 16K statt 4K, damit auch grosse
// Stack-Frames (>4K, z.B. sendTcpForConnection ~1K, Diag-Puffer) den
// Guard nicht ueberspringen koennen; uncommitted = kostet nur Adressraum.
const STACK_GUARD_SIZE: usize = 16 * 1024;
const SAVED_REG_COUNT: usize = 6;
const STACK_CACHE_LIMIT: usize = 8;
const CRITICAL_RESERVE_COUNT: usize = 4;
const NO_CRITICAL_RESERVE_SLOT: u8 = 0xFF;

// Zero is the legacy diagnostic spelling for resource-limited/dynamic. It is
// deliberately not a capacity and must never be used to size another table.
pub const max_tasks: usize = 0;
pub const stack_size: usize = STACK_SIZE;
pub const stack_guard_size: usize = STACK_GUARD_SIZE;

pub const State = enum(u8) {
    unused,
    ready,
    running,
    blocked,
    dead,
};

// 0.56.18: Prioritaetsklassen (Befund 4.1). HIGH fuer Eingabe-Pfade
// (usb-hid-poll), NORMAL fuer alles andere; LOW wird erst vergeben, wenn
// ein echter Batch-Konsument existiert. Auswahl in scheduler.nextReadyTask.
pub const Priority = enum(u8) {
    high = 0,
    normal = 1,
    low = 2,
};

pub const WaitResult = enum(u32) {
    none = 0,
    signaled = 1,
    timeout = 2,
    cancelled = 3,
    killed = 4,
    failed = 5,
};

pub const Entry = *const fn () callconv(.c) void;

pub const HeldLockRecord = struct {
    object_id: u64 = 0,
    rank: u16 = 0,
    mode_no_sleep: bool = false,
    active: bool = false,
};

pub const CreateFailure = enum(u8) {
    none = 0,
    task_metadata = 1,
    stack = 2,
    fpu = 3,
    waiter = 4,
    memory = 5,
};

pub const CreateFailureStats = struct {
    task_metadata: u64 = 0,
    stack: u64 = 0,
    fpu: u64 = 0,
    waiter: u64 = 0,
    memory: u64 = 0,
    last: CreateFailure = .none,
};

pub const RetireResult = enum(u8) {
    gone,
    pending,
    released,
};

pub const Task = struct {
    registry_prev: ?*Task = null,
    registry_next: ?*Task = null,
    generation: u64 = 0,
    pin_count: u32 = 0,
    retire_pending: bool = false,
    // Resource teardown may yield. Keep the dead descriptor published while
    // one releaser owns the transaction so a failed teardown always retains
    // the same generation-safe retry anchor.
    release_in_progress: bool = false,
    // Unpublished construction failures reuse their already allocated Task
    // object as an unbounded intrusive rollback record. They never enter the
    // public registry but remain retryable without another allocation.
    rollback_pending: bool = false,
    rollback_next: ?*Task = null,
    critical_reserve_slot: u8 = NO_CRITICAL_RESERVE_SLOT,
    id: u32 = 0,
    name: []const u8 = "",
    state: State = .unused,
    rsp: u64 = 0,
    wake_tick: u64 = 0,
    blocked_since_tick: u64 = 0,
    wait_reason: []const u8 = "",
    wait_object: u64 = 0,
    wait_result: WaitResult = .none,
    wait_node: wait_node.Node = .{},
    // Correctness state, independent of the bounded diagnostic records below.
    // It counts distinct sync.Mutex objects owned by this exact task.
    held_lock_count: u32 = 0,
    // Recursive, generation-safe owners that intentionally span scheduler
    // waits (for example the serialized filesystem request transaction).
    // They block hard kill/reap but are not reported as sleep-under-lock.
    unwind_guard_count: u32 = 0,
    held_locks: [8]HeldLockRecord = .{HeldLockRecord{}} ** 8,
    stack_base: u64 = 0,
    stack_top: u64 = 0,
    stack_range_id: u32 = 0,
    priority: Priority = .normal,
    entry: ?Entry = null,
    created_tick: u64 = 0,
    ready_since_tick: u64 = 0,
    last_scheduled_tick: u64 = 0,
    last_yield_tick: u64 = 0,
    last_ready_latency_ticks: u64 = 0,
    max_ready_latency_ticks: u64 = 0,
    last_wait_ticks: u64 = 0,
    max_wait_ticks: u64 = 0,
    run_ticks: u64 = 0,
    switches_in: u64 = 0,
    max_run_without_switch_ticks: u64 = 0,
    preempt_disable_depth: u32 = 0,
    preempt_disable_max_depth: u32 = 0,
    preemption_probe_hits: u64 = 0,
    preemption_deferred_ticks: u64 = 0,
    max_preemption_deferred_ticks: u64 = 0,
    long_run_warnings: u64 = 0,
    starvation_warnings: u64 = 0,
    last_long_run_warning_tick: u64 = 0,
    last_starvation_warning_tick: u64 = 0,
    fpu_state: ?[]u8 = null,
    fpu_state_bytes: u32 = 0,
    fpu_state_valid: bool = false,
    // 0.56.30 Lazy-FPU: false nur fuer reine Kernel-Tasks - der Kernel ist
    // soft_float OHNE SSE/AVX (Code/build.zig subtrahiert die Features),
    // solche Tasks koennen FPU-State weder nutzen noch korrumpieren.
    // Default true (sicher); createKernelThread setzt false, weil dessen
    // Aufrufer ausschliesslich Kernel-Worker sind (net-rx, usb-hid-poll,
    // r4d-work, async-I/O, Selbsttests). Echte R4X-Ausfuehrung laeuft ueber
    // createKernelThreadBlocked und behaelt true; ein initial blockierter,
    // reiner Kernel-Worker nutzt createKernelWorkerBlocked.
    uses_fpu: bool = false,
};

pub const Summary = struct {
    max_tasks: u32 = 0,
    total: u32 = 0,
    ready: u32 = 0,
    running: u32 = 0,
    blocked: u32 = 0,
    dead: u32 = 0,
    workers: u32 = 0,
};

pub const InventorySnapshot = struct {
    task_id: u32 = 0,
    state: State = .unused,
    generation: u64 = 0,
    created_tick: u64 = 0,
    last_run_tick: u64 = 0,
    wake_tick: u64 = 0,
    runtime_ticks: u64 = 0,
};

pub const InventoryPage = struct {
    epoch: u64 = 0,
    total: u32 = 0,
    returned: u32 = 0,
    has_more: bool = false,
};

var registry_head: ?*Task = null;
var registry_tail: ?*Task = null;
var rollback_retry_head: ?*Task = null;
var rollback_retry_count: usize = 0;
var task_count: usize = 0;
var task_peak: usize = 0;
var inventory_mutation_epoch: u64 = 1;
var next_id: u32 = 1;
var next_generation: u64 = 1;
var generation_exhausted = false;
var initialized = false;
var create_failure_stats: CreateFailureStats = .{};
var forced_next_create_failure: CreateFailure = .none;
var kill_held_lock_deferrals: u64 = 0;

pub fn createFailureStats() CreateFailureStats {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return create_failure_stats;
}

pub fn killHeldLockDeferrals() u64 {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return kill_held_lock_deferrals;
}

pub fn noteCreateFailure(failure: CreateFailure) void {
    if (failure == .none) return;
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    create_failure_stats.last = failure;
    switch (failure) {
        .none => {},
        .task_metadata => create_failure_stats.task_metadata +%= 1,
        .stack => create_failure_stats.stack +%= 1,
        .fpu => create_failure_stats.fpu +%= 1,
        .waiter => create_failure_stats.waiter +%= 1,
        .memory => create_failure_stats.memory +%= 1,
    }
}

fn recordCreateFailure(failure_out: *CreateFailure, failure: CreateFailure) void {
    failure_out.* = failure;
    noteCreateFailure(failure);
}

// Deterministic one-shot seam for the boot-option-gated TASKREGISTRY runtime
// acceptance. Only normal admission consumes this injection; the separately
// preallocated critical reserve must remain usable while normal creation is
// rejected. Production paths never arm it.
pub fn armNextCreateFailureForTest(failure: CreateFailure) bool {
    if (failure == .none) return false;
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (forced_next_create_failure != .none) return false;
    forced_next_create_failure = failure;
    return true;
}

fn takeForcedCreateFailure() CreateFailure {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const failure = forced_next_create_failure;
    forced_next_create_failure = .none;
    return failure;
}

pub fn init() bool {
    initialized = false;
    registry_head = null;
    registry_tail = null;
    rollback_retry_head = null;
    rollback_retry_count = 0;
    task_count = 0;
    task_peak = 0;
    inventory_mutation_epoch = 1;
    next_id = 1;
    next_generation = 1;
    generation_exhausted = false;
    stack_cache_count = 0;
    stack_cache_hits = 0;
    stack_cache_misses = 0;
    stack_cache_reclaims = 0;
    stack_cache_release_failures = 0;
    critical_reserve_in_use = 0;
    create_failure_stats = .{};
    forced_next_create_failure = .none;
    kill_held_lock_deferrals = 0;
    min_wake_tick = NO_WAKE_TICK;

    if (!prepareCriticalReserve()) return false;
    initialized = true;
    var create_failure: CreateFailure = .none;
    _ = createTask("kernel-main", .running, false, null, &create_failure) orelse {
        initialized = false;
        cleanupCriticalReserve();
        return false;
    };
    // 0.56.15: Boot-Marker fuer die Guard-Page-Abnahme.
    k.puts("[TASKSTACK] guard-pages active: guard=");
    k.putDec(STACK_GUARD_SIZE);
    k.puts(" stack=");
    k.putDec(STACK_SIZE);
    k.puts(" registry=dynamic stack_cache=");
    k.putDec(STACK_CACHE_LIMIT);
    k.puts(" critical_reserve=");
    k.putDec(CRITICAL_RESERVE_COUNT);
    k.puts("\r\n");
    return true;
}

pub fn createKernelTask(name: []const u8, state: State) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelTaskWithFailure(name, state, &failure);
}

pub fn createKernelTaskWithFailure(name: []const u8, state: State, failure_out: *CreateFailure) ?*Task {
    return createTask(name, state, true, null, failure_out);
}

fn createTask(name: []const u8, state: State, needs_fpu: bool, entry: ?Entry, failure_out: *CreateFailure) ?*Task {
    failure_out.* = .none;
    if (!initialized) {
        recordCreateFailure(failure_out, .memory);
        return null;
    }
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) {
        recordCreateFailure(failure_out, .memory);
        return null;
    }
    defer _ = task_context.leaveUnwind(unwind);

    const forced_failure = takeForcedCreateFailure();
    if (forced_failure != .none) {
        recordCreateFailure(failure_out, forced_failure);
        return null;
    }

    // Nothing is published until all independently fallible resources exist.
    // This keeps scans, IRQ attribution and rollback from observing a partial
    // task and makes every failure path the exact reverse of construction.
    const task_memory = heap.alloc(@sizeOf(Task), @alignOf(Task)) orelse {
        recordCreateFailure(failure_out, .task_metadata);
        return null;
    };
    const new_task: *Task = @ptrCast(@alignCast(task_memory.ptr));
    new_task.* = .{};
    if (!wait_node.isDetached(&new_task.wait_node)) {
        recordCreateFailure(failure_out, .waiter);
        rollbackUnpublishedTask(new_task);
        return null;
    }

    const stack = allocGuardedStack(new_task) orelse {
        recordCreateFailure(failure_out, .stack);
        rollbackUnpublishedTask(new_task);
        return null;
    };
    new_task.stack_base = stack.base;
    new_task.stack_top = stack.top;
    new_task.stack_range_id = stack.range_id;

    var fpu_memory: ?[]u8 = null;
    if (needs_fpu) {
        const state_bytes = fpu.activeStateBytes();
        if (state_bytes == 0) {
            recordCreateFailure(failure_out, .fpu);
            rollbackUnpublishedTask(new_task);
            return null;
        }
        const memory = heap.alloc(state_bytes, fpu.state_storage_align) orelse {
            recordCreateFailure(failure_out, .fpu);
            rollbackUnpublishedTask(new_task);
            return null;
        };
        new_task.fpu_state = memory;
        new_task.fpu_state_bytes = @intCast(memory.len);
        if (!fpu.initTaskState(memory)) {
            recordCreateFailure(failure_out, .fpu);
            rollbackUnpublishedTask(new_task);
            return null;
        }
        fpu_memory = memory;
    }

    const now = timer.tickCount();
    new_task.* = .{
        .name = name,
        .state = state,
        .rsp = if (entry != null) initialRsp(stack.top) else 0,
        .stack_base = stack.base,
        .stack_top = stack.top,
        .stack_range_id = stack.range_id,
        .created_tick = now,
        .ready_since_tick = if (state == .ready) now else 0,
        .last_scheduled_tick = if (state == .running) now else 0,
        .switches_in = if (state == .running) 1 else 0,
        .entry = entry,
        .fpu_state = fpu_memory,
        .fpu_state_bytes = if (fpu_memory) |memory| @intCast(memory.len) else 0,
        .fpu_state_valid = fpu_memory != null,
        .uses_fpu = needs_fpu,
    };

    const irq_flags = interrupts.saveAndDisable();
    const id = allocateTaskIdLocked() orelse {
        recordCreateFailure(failure_out, .memory);
        interrupts.restore(irq_flags);
        rollbackUnpublishedTask(new_task);
        return null;
    };
    const generation = allocateTaskGenerationLocked() orelse {
        recordCreateFailure(failure_out, .memory);
        interrupts.restore(irq_flags);
        rollbackUnpublishedTask(new_task);
        return null;
    };
    new_task.id = id;
    new_task.generation = generation;
    linkRegistryLocked(new_task);
    interrupts.restore(irq_flags);
    return new_task;
}

fn rollbackUnpublishedTask(t: *Task) void {
    if (releaseUnpublishedResources(t) and freeTaskMemory(t)) return;
    enqueueRollbackRetry(t);
}

fn releaseUnpublishedResources(t: *Task) bool {
    if (t.fpu_state) |memory| {
        if (heap.free(memory) != .ok) {
            k.puts("Unpublished Task FPU release pending retry\r\n");
            return false;
        }
        t.fpu_state = null;
        t.fpu_state_bytes = 0;
        t.fpu_state_valid = false;
    }
    if (t.stack_range_id != 0) {
        const stack: GuardedStack = .{
            .range_id = t.stack_range_id,
            .base = t.stack_base,
            .top = t.stack_top,
        };
        if (!releaseGuardedStackDirect(stack)) return false;
        t.stack_range_id = 0;
        t.stack_base = 0;
        t.stack_top = 0;
    }
    return true;
}

fn enqueueRollbackRetry(t: *Task) void {
    const irq_flags = interrupts.saveAndDisable();
    t.release_in_progress = false;
    if (!t.rollback_pending) {
        t.rollback_pending = true;
        t.rollback_next = rollback_retry_head;
        rollback_retry_head = t;
        rollback_retry_count += 1;
    }
    interrupts.restore(irq_flags);
    k.puts("Unpublished Task rollback pending retry\r\n");
}

fn retryOneUnpublishedRollback() bool {
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) return false;
    defer _ = task_context.leaveUnwind(unwind);

    const irq_flags = interrupts.saveAndDisable();
    var candidate: ?*Task = null;
    var cursor = rollback_retry_head;
    while (cursor) |pending| : (cursor = pending.rollback_next) {
        if (!pending.release_in_progress) {
            pending.release_in_progress = true;
            candidate = pending;
            break;
        }
    }
    interrupts.restore(irq_flags);

    const t = candidate orelse return false;
    if (!releaseUnpublishedResources(t)) {
        const retry_flags = interrupts.saveAndDisable();
        t.release_in_progress = false;
        interrupts.restore(retry_flags);
        return false;
    }

    const unlink_flags = interrupts.saveAndDisable();
    if (!unlinkRollbackRetryLocked(t)) {
        t.release_in_progress = false;
        interrupts.restore(unlink_flags);
        k.puts("Unpublished Task rollback anchor missing\r\n");
        return false;
    }
    interrupts.restore(unlink_flags);

    if (freeTaskMemory(t)) return true;
    enqueueRollbackRetry(t);
    return false;
}

fn unlinkRollbackRetryLocked(t: *Task) bool {
    var previous: ?*Task = null;
    var cursor = rollback_retry_head;
    while (cursor) |pending| : (cursor = pending.rollback_next) {
        if (pending == t) {
            if (previous) |before| {
                before.rollback_next = pending.rollback_next;
            } else {
                rollback_retry_head = pending.rollback_next;
            }
            pending.rollback_next = null;
            pending.rollback_pending = false;
            if (rollback_retry_count != 0) rollback_retry_count -= 1;
            return true;
        }
        previous = pending;
    }
    return false;
}

pub fn unpublishedRollbackPending() u32 {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return @intCast(@min(rollback_retry_count, @as(usize, 0xFFFF_FFFF)));
}

fn allocateTaskIdLocked() ?u32 {
    var candidate = if (next_id == 0) @as(u32, 1) else next_id;
    const start = candidate;
    while (true) {
        if (findByIdLocked(candidate) == null) {
            next_id = candidate +% 1;
            if (next_id == 0) next_id = 1;
            return candidate;
        }
        candidate +%= 1;
        if (candidate == 0) candidate = 1;
        if (candidate == start) return null;
    }
}

fn allocateTaskGenerationLocked() ?u64 {
    if (generation_exhausted or next_generation == 0) return null;
    const generation = next_generation;
    next_generation +%= 1;
    if (next_generation == 0) generation_exhausted = true;
    return generation;
}

fn linkRegistryLocked(t: *Task) void {
    t.registry_prev = registry_tail;
    t.registry_next = null;
    if (registry_tail) |tail| tail.registry_next = t else registry_head = t;
    registry_tail = t;
    task_count += 1;
    if (task_count > task_peak) task_peak = task_count;
    bumpInventoryMutationEpochLocked();
}

fn unlinkRegistryLocked(t: *Task) void {
    if (t.registry_prev) |previous| previous.registry_next = t.registry_next else registry_head = t.registry_next;
    if (t.registry_next) |following| following.registry_prev = t.registry_prev else registry_tail = t.registry_prev;
    t.registry_prev = null;
    t.registry_next = null;
    if (task_count != 0) task_count -= 1;
    bumpInventoryMutationEpochLocked();
}

fn bumpInventoryMutationEpochLocked() void {
    inventory_mutation_epoch +%= 1;
    if (inventory_mutation_epoch == 0) inventory_mutation_epoch = 1;
}

fn bumpInventoryMutationEpoch() void {
    const irq_flags = interrupts.saveAndDisable();
    bumpInventoryMutationEpochLocked();
    interrupts.restore(irq_flags);
}

const GuardedStack = struct {
    range_id: u32 = 0,
    base: u64 = 0,
    top: u64 = 0,
    // A failed virt.release may already have unmapped part of the range. Such
    // an entry remains a retry anchor but must never be handed to a new task.
    release_pending: bool = false,
    release_in_progress: bool = false,
};

const CriticalBundle = struct {
    task: ?*Task = null,
    stack: GuardedStack = .{},
    in_use: bool = false,
};

var critical_reserve: [CRITICAL_RESERVE_COUNT]CriticalBundle = .{CriticalBundle{}} ** CRITICAL_RESERVE_COUNT;
var critical_reserve_in_use: usize = 0;

pub fn criticalReserveStats() struct { total: u32, available: u32, in_use: u32 } {
    const used: u32 = @intCast(critical_reserve_in_use);
    return .{
        .total = CRITICAL_RESERVE_COUNT,
        .available = @intCast(CRITICAL_RESERVE_COUNT - critical_reserve_in_use),
        .in_use = used,
    };
}

fn prepareCriticalReserve() bool {
    critical_reserve = .{CriticalBundle{}} ** CRITICAL_RESERVE_COUNT;
    critical_reserve_in_use = 0;
    for (&critical_reserve, 0..) |*bundle, index| {
        const task_memory = heap.alloc(@sizeOf(Task), @alignOf(Task)) orelse {
            noteCreateFailure(.task_metadata);
            cleanupCriticalReserve();
            return false;
        };
        const reserved_task: *Task = @ptrCast(@alignCast(task_memory.ptr));
        reserved_task.* = .{ .critical_reserve_slot = @intCast(index) };
        const stack = allocGuardedStack(reserved_task) orelse {
            noteCreateFailure(.stack);
            rollbackUnpublishedTask(reserved_task);
            cleanupCriticalReserve();
            return false;
        };
        reserved_task.stack_base = stack.base;
        reserved_task.stack_top = stack.top;
        reserved_task.stack_range_id = stack.range_id;
        bundle.* = .{
            .task = reserved_task,
            .stack = stack,
        };
    }
    return true;
}

fn cleanupCriticalReserve() void {
    for (&critical_reserve) |*bundle| {
        const reserved_task = bundle.task orelse continue;
        rollbackUnpublishedTask(reserved_task);
        bundle.* = .{};
    }
    critical_reserve_in_use = 0;
}

// Independent churn optimization: at most eight 80-KB ranges (64 KB resident)
// are retained. This budget is unrelated to task capacity and can be drained
// explicitly under pressure.
var stack_cache: [STACK_CACHE_LIMIT]GuardedStack = undefined;
var stack_cache_count: usize = 0;
var stack_cache_hits: u64 = 0;
var stack_cache_misses: u64 = 0;
var stack_cache_reclaims: u64 = 0;
var stack_cache_release_failures: u64 = 0;

pub fn stackCacheStats() struct { cached: u64, hits: u64, misses: u64, reclaims: u64, release_failures: u64 } {
    return .{
        .cached = stack_cache_count,
        .hits = stack_cache_hits,
        .misses = stack_cache_misses,
        .reclaims = stack_cache_reclaims,
        .release_failures = stack_cache_release_failures,
    };
}

fn allocGuardedStack(retry_owner: *Task) ?GuardedStack {
    const irq_flags = interrupts.saveAndDisable();
    var reusable_index: ?usize = null;
    var scan = stack_cache_count;
    while (scan > 0) {
        scan -= 1;
        if (!stack_cache[scan].release_pending and !stack_cache[scan].release_in_progress) {
            reusable_index = scan;
            break;
        }
    }
    if (reusable_index) |index| {
        const last = stack_cache_count - 1;
        const cached = stack_cache[index];
        if (index != last) stack_cache[index] = stack_cache[last];
        stack_cache_count = last;
        stack_cache_hits +%= 1;
        interrupts.restore(irq_flags);
        return cached;
    }
    stack_cache_misses +%= 1;
    interrupts.restore(irq_flags);
    const range_id = virt.reserve(.{
        .window = .kernel_stack,
        .len = STACK_GUARD_SIZE + STACK_SIZE,
        .name = "kernel-task-stack",
    }) catch |err| {
        k.puts("Task stack reserve failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        return null;
    };
    const info = virt.rangeInfo(range_id) orelse {
        anchorFreshStackRelease(retry_owner, .{ .range_id = range_id });
        return null;
    };
    virt.protectGuard(range_id, 0, STACK_GUARD_SIZE) catch |err| {
        k.puts("Task stack guard failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        anchorFreshStackRelease(retry_owner, .{
            .range_id = range_id,
            .base = info.base + STACK_GUARD_SIZE,
            .top = info.base + STACK_GUARD_SIZE + STACK_SIZE,
        });
        return null;
    };
    virt.commit(range_id, STACK_GUARD_SIZE, STACK_SIZE) catch |err| {
        k.puts("Task stack commit failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        anchorFreshStackRelease(retry_owner, .{
            .range_id = range_id,
            .base = info.base + STACK_GUARD_SIZE,
            .top = info.base + STACK_GUARD_SIZE + STACK_SIZE,
        });
        return null;
    };
    return .{
        .range_id = range_id,
        .base = info.base + STACK_GUARD_SIZE,
        .top = info.base + STACK_GUARD_SIZE + STACK_SIZE,
    };
}

fn anchorFreshStackRelease(retry_owner: *Task, stack: GuardedStack) void {
    if (releaseGuardedStackDirect(stack)) return;
    retry_owner.stack_range_id = stack.range_id;
    retry_owner.stack_base = stack.base;
    retry_owner.stack_top = stack.top;
}

fn releaseGuardedStackDirect(stack: GuardedStack) bool {
    if (stack.range_id == 0) return true;
    virt.release(stack.range_id) catch |err| {
        k.puts("Task stack release failed: ");
        k.puts(@errorName(err));
        k.puts("\r\n");
        return false;
    };
    return true;
}

pub fn reclaimStackCache(requested_stacks_raw: u32) u32 {
    const unwind = task_context.enterUnwind();
    if (!unwind.admitted()) return 0;
    defer _ = task_context.leaveUnwind(unwind);

    const requested_stacks: usize = if (requested_stacks_raw == 0) stack_cache.len else @intCast(requested_stacks_raw);
    var reclaimed: u32 = 0;
    while (@as(usize, reclaimed) < requested_stacks) {
        const irq_flags = interrupts.saveAndDisable();
        if (stack_cache_count == 0) {
            interrupts.restore(irq_flags);
            break;
        }
        var candidate_index: ?usize = null;
        var index: usize = 0;
        while (index < stack_cache_count) : (index += 1) {
            if (!stack_cache[index].release_in_progress) {
                candidate_index = index;
                break;
            }
        }
        const selected = candidate_index orelse {
            interrupts.restore(irq_flags);
            break;
        };
        stack_cache[selected].release_pending = true;
        stack_cache[selected].release_in_progress = true;
        const stack = stack_cache[selected];
        interrupts.restore(irq_flags);

        if (!releaseGuardedStackDirect(stack)) {
            const retry_flags = interrupts.saveAndDisable();
            const retry_index = findCachedStackLocked(stack.range_id);
            if (retry_index) |found| {
                stack_cache[found].release_pending = true;
                stack_cache[found].release_in_progress = false;
            }
            stack_cache_release_failures +%= 1;
            interrupts.restore(retry_flags);
            k.puts("Task stack cache release pending retry\r\n");
            break;
        }
        const commit_flags = interrupts.saveAndDisable();
        const commit_index = findCachedStackLocked(stack.range_id) orelse {
            interrupts.restore(commit_flags);
            k.puts("Task stack cache release anchor missing\r\n");
            break;
        };
        const last = stack_cache_count - 1;
        if (commit_index != last) stack_cache[commit_index] = stack_cache[last];
        stack_cache_count = last;
        reclaimed += 1;
        stack_cache_reclaims +%= 1;
        interrupts.restore(commit_flags);
    }
    return reclaimed;
}

fn findCachedStackLocked(range_id: u32) ?usize {
    var index: usize = 0;
    while (index < stack_cache_count) : (index += 1) {
        if (stack_cache[index].range_id == range_id) return index;
    }
    return null;
}

// 0.56.15: Faellt eine Fault-Adresse in die Guard-Page eines Task-Stacks,
// ist das ein Kernel-Stack-Overflow dieses Tasks (Crash-Report-Diagnose).
pub const GuardHit = struct {
    name: []const u8,
    id: u32,
    stack_base: u64,
};

pub fn stackGuardHit(addr: u64) ?GuardHit {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var cursor = registry_head;
    while (cursor) |t| : (cursor = t.registry_next) {
        if (t.state == .unused or t.stack_base == 0) continue;
        const guard_base = t.stack_base - STACK_GUARD_SIZE;
        if (addr >= guard_base and addr < t.stack_base) {
            return .{ .name = t.name, .id = t.id, .stack_base = t.stack_base };
        }
    }
    return null;
}

// A dead task still executes on its stack until the context switch has moved
// current_task. Deferred callers therefore observe a loud refusal instead of
// freeing its stable object or stack too early.
var current_provider: ?*const fn () ?*Task = null;
var recycle_guard_hits: u64 = 0;

pub fn setCurrentProvider(provider: ?*const fn () ?*Task) void {
    current_provider = provider;
}

pub fn recycleGuardHits() u64 {
    return recycle_guard_hits;
}

fn isCurrentTask(t: *Task) bool {
    const provider = current_provider orelse return false;
    const current = provider() orelse return false;
    return current == t;
}

pub fn createKernelThread(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelThreadWithFailure(name, entry, &failure);
}

pub fn createKernelThreadWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return createTask(name, .ready, false, entry, failure_out);
}

pub fn createKernelThreadBlocked(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelThreadBlockedWithFailure(name, entry, &failure);
}

pub fn createKernelThreadBlockedWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return createTask(name, .blocked, true, entry, failure_out);
}

pub fn createKernelWorkerBlocked(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelWorkerBlockedWithFailure(name, entry, &failure);
}

pub fn createKernelWorkerBlockedWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return createTask(name, .blocked, false, entry, failure_out);
}

// Critical workers consume one of four boot-time Task+Stack bundles. Normal
// admission can never borrow these bundles, so an allocator/VM pressure event
// cannot prevent an already designed recovery, I/O or reaper path from making
// progress. Critical workers are kernel-only and therefore use soft-float.
pub fn createKernelThreadCritical(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelThreadCriticalWithFailure(name, entry, &failure);
}

pub fn createKernelThreadCriticalWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return createCriticalTask(name, .ready, entry, failure_out);
}

fn createCriticalTask(name: []const u8, state: State, entry: Entry, failure_out: *CreateFailure) ?*Task {
    failure_out.* = .none;
    if (!initialized) {
        recordCreateFailure(failure_out, .memory);
        return null;
    }

    const irq_flags = interrupts.saveAndDisable();
    var selected_index: ?usize = null;
    for (&critical_reserve, 0..) |*bundle, index| {
        if (!bundle.in_use) {
            selected_index = index;
            break;
        }
    }
    const index = selected_index orelse {
        recordCreateFailure(failure_out, .task_metadata);
        interrupts.restore(irq_flags);
        return null;
    };
    const bundle = &critical_reserve[index];
    const new_task = bundle.task orelse {
        recordCreateFailure(failure_out, .task_metadata);
        interrupts.restore(irq_flags);
        return null;
    };
    if (!wait_node.isDetached(&new_task.wait_node)) {
        recordCreateFailure(failure_out, .waiter);
        interrupts.restore(irq_flags);
        return null;
    }

    const now = timer.tickCount();
    new_task.* = .{
        .critical_reserve_slot = @intCast(index),
        .name = name,
        .state = state,
        .rsp = initialRsp(bundle.stack.top),
        .stack_base = bundle.stack.base,
        .stack_top = bundle.stack.top,
        .stack_range_id = bundle.stack.range_id,
        .created_tick = now,
        .ready_since_tick = if (state == .ready) now else 0,
        .last_scheduled_tick = if (state == .running) now else 0,
        .switches_in = if (state == .running) 1 else 0,
        .entry = entry,
        .uses_fpu = false,
    };
    const id = allocateTaskIdLocked() orelse {
        recordCreateFailure(failure_out, .memory);
        resetCriticalTaskLocked(bundle, index);
        interrupts.restore(irq_flags);
        return null;
    };
    const generation = allocateTaskGenerationLocked() orelse {
        recordCreateFailure(failure_out, .memory);
        resetCriticalTaskLocked(bundle, index);
        interrupts.restore(irq_flags);
        return null;
    };
    new_task.id = id;
    new_task.generation = generation;
    bundle.in_use = true;
    critical_reserve_in_use += 1;
    linkRegistryLocked(new_task);
    interrupts.restore(irq_flags);
    return new_task;
}

fn resetCriticalTaskLocked(bundle: *CriticalBundle, index: usize) void {
    const reserved_task = bundle.task orelse return;
    reserved_task.* = .{
        .critical_reserve_slot = @intCast(index),
        .stack_base = bundle.stack.base,
        .stack_top = bundle.stack.top,
        .stack_range_id = bundle.stack.range_id,
    };
}

pub fn count() usize {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return task_count;
}

pub fn summary() Summary {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var out = Summary{ .total = @intCast(@min(task_count, @as(usize, 0xFFFF_FFFF))) };
    var cursor = registry_head;
    while (cursor) |t| : (cursor = t.registry_next) {
        if (t.entry != null and t.state != .unused and t.state != .dead) out.workers += 1;
        switch (t.state) {
            .ready => out.ready += 1,
            .running => out.running += 1,
            .blocked => out.blocked += 1,
            .dead => out.dead += 1,
            .unused => {},
        }
    }
    return out;
}

pub fn inventoryEpoch() u64 {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return inventory_mutation_epoch;
}

pub fn inventoryPeak() u32 {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    return @intCast(@min(task_peak, @as(usize, 0xFFFF_FFFF)));
}

fn inventorySnapshot(t: *const Task) InventorySnapshot {
    return .{
        .task_id = t.id,
        .state = t.state,
        .generation = t.generation,
        .created_tick = t.created_tick,
        .last_run_tick = t.last_scheduled_tick,
        .wake_tick = t.wake_tick,
        .runtime_ticks = t.run_ticks,
    };
}

fn inventoryHeapSiftUp(heap_items: []InventorySnapshot, start_index: usize) void {
    var index = start_index;
    while (index != 0) {
        const parent = (index - 1) / 2;
        if (heap_items[parent].generation >= heap_items[index].generation) break;
        const tmp = heap_items[parent];
        heap_items[parent] = heap_items[index];
        heap_items[index] = tmp;
        index = parent;
    }
}

fn inventoryHeapSiftDown(heap_items: []InventorySnapshot, start_index: usize) void {
    var index = start_index;
    while (true) {
        const left = index * 2 + 1;
        if (left >= heap_items.len) return;
        const right = left + 1;
        var largest = left;
        if (right < heap_items.len and heap_items[right].generation > heap_items[left].generation) largest = right;
        if (heap_items[index].generation >= heap_items[largest].generation) return;
        const tmp = heap_items[index];
        heap_items[index] = heap_items[largest];
        heap_items[largest] = tmp;
        index = largest;
    }
}

fn sortInventorySnapshotsByGeneration(items: []InventorySnapshot) void {
    var index: usize = 1;
    while (index < items.len) : (index += 1) {
        const value = items[index];
        var insert = index;
        while (insert != 0 and items[insert - 1].generation > value.generation) : (insert -= 1) {
            items[insert] = items[insert - 1];
        }
        items[insert] = value;
    }
}

/// Copies one bounded page in one registry traversal. Membership plus durable
/// lifecycle transitions are guarded by `epoch`. The ready/running dispatch
/// flip and its run counters are deliberately page-local volatile telemetry;
/// advancing the global epoch on every scheduler tick would make a multi-page
/// inventory impossible under normal load.
pub fn inventoryPage(after_generation: u64, out: []InventorySnapshot) InventoryPage {
    if (out.len == 0) return .{};
    const irq_flags = interrupts.saveAndDisable();
    const epoch = inventory_mutation_epoch;
    var total: u32 = 0;
    var eligible: u32 = 0;
    var selected: usize = 0;
    var cursor = registry_head;
    while (cursor) |candidate| : (cursor = candidate.registry_next) {
        if (candidate.state == .unused) continue;
        total +|= 1;
        if (candidate.generation <= after_generation) continue;
        eligible +|= 1;
        const snapshot = inventorySnapshot(candidate);
        if (selected < out.len) {
            out[selected] = snapshot;
            inventoryHeapSiftUp(out[0 .. selected + 1], selected);
            selected += 1;
        } else if (snapshot.generation < out[0].generation) {
            out[0] = snapshot;
            inventoryHeapSiftDown(out[0..selected], 0);
        }
    }
    interrupts.restore(irq_flags);
    sortInventorySnapshotsByGeneration(out[0..selected]);
    return .{
        .epoch = epoch,
        .total = total,
        .returned = @intCast(selected),
        .has_more = eligible > @as(u32, @intCast(selected)),
    };
}

pub fn pinByOrdinal(index: usize) ?*Task {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var ordinal: usize = 0;
    var cursor = registry_head;
    while (cursor) |t| : (cursor = t.registry_next) {
        if (ordinal == index) {
            if (t.state == .unused or t.release_in_progress or t.pin_count == 0xFFFF_FFFF) return null;
            t.pin_count += 1;
            return t;
        }
        ordinal += 1;
    }
    return null;
}

pub fn first() ?*Task {
    return registry_head;
}

pub fn next(t: *const Task) ?*Task {
    return t.registry_next;
}

pub fn nextCircular(t: *const Task) ?*Task {
    return t.registry_next orelse registry_head;
}

pub fn ordinalOf(wanted: *const Task) ?usize {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var ordinal: usize = 0;
    var cursor = registry_head;
    while (cursor) |t| : (cursor = t.registry_next) {
        if (t == wanted) return ordinal;
        ordinal += 1;
    }
    return null;
}

pub var fpu_lazy_saves: u64 = 0;
pub var fpu_lazy_skips: u64 = 0;

pub fn saveFpuState(t: *Task) void {
    if (!t.uses_fpu) {
        fpu_lazy_skips +%= 1;
        return;
    }
    if (!t.fpu_state_valid) return;
    const state = t.fpu_state orelse return;
    fpu_lazy_saves +%= 1;
    fpu.saveTaskState(state);
}

pub fn restoreFpuState(t: *Task) void {
    if (!t.uses_fpu) return;
    if (!t.fpu_state_valid) return;
    const state = t.fpu_state orelse return;
    fpu.restoreTaskState(state);
}

pub fn validFpuStateCount() u32 {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    var count_valid: u32 = 0;
    var cursor = registry_head;
    while (cursor) |t| : (cursor = t.registry_next) {
        if (t.state != .unused and t.state != .dead and t.fpu_state_valid) {
            count_valid += 1;
        }
    }
    return count_valid;
}

pub fn markReady(t: *Task, now: u64) void {
    const was_durably_blocked = t.state == .blocked;
    t.state = .ready;
    t.ready_since_tick = now;
    if (was_durably_blocked) bumpInventoryMutationEpoch();
}

pub fn markRunning(t: *Task) void {
    const was_durably_blocked = t.state == .blocked;
    t.state = .running;
    t.ready_since_tick = 0;
    if (was_durably_blocked) bumpInventoryMutationEpoch();
}

pub fn recordScheduled(t: *Task, now: u64) u64 {
    const latency = if (t.ready_since_tick != 0 and now >= t.ready_since_tick)
        now - t.ready_since_tick
    else
        0;
    t.last_ready_latency_ticks = latency;
    if (latency > t.max_ready_latency_ticks) t.max_ready_latency_ticks = latency;
    t.ready_since_tick = 0;
    t.last_scheduled_tick = now;
    t.switches_in +%= 1;
    return latency;
}

pub fn recordYield(t: *Task, now: u64) void {
    t.last_yield_tick = now;
}

pub fn recordRunTick(t: *Task, now: u64) u64 {
    t.run_ticks +%= 1;
    const run_ticks = ticksSince(now, t.last_scheduled_tick);
    if (run_ticks > t.max_run_without_switch_ticks) t.max_run_without_switch_ticks = run_ticks;
    return run_ticks;
}

pub fn recordPreemptDisable(t: *Task) u32 {
    t.preempt_disable_depth +|= 1;
    if (t.preempt_disable_depth > t.preempt_disable_max_depth) {
        t.preempt_disable_max_depth = t.preempt_disable_depth;
    }
    return t.preempt_disable_depth;
}

pub fn recordPreemptEnable(t: *Task) bool {
    if (t.preempt_disable_depth == 0) return false;
    t.preempt_disable_depth -= 1;
    return true;
}

pub fn recordPreemptionProbe(t: *Task) void {
    t.preemption_probe_hits +%= 1;
}

pub fn recordPreemptionDeferred(t: *Task, scheduled_ticks: u64) void {
    t.preemption_deferred_ticks +%= 1;
    if (scheduled_ticks > t.max_preemption_deferred_ticks) {
        t.max_preemption_deferred_ticks = scheduled_ticks;
    }
}

pub fn recordLongRunWarning(t: *Task, now: u64) void {
    t.long_run_warnings +%= 1;
    t.last_long_run_warning_tick = now;
}

pub fn recordStarvationWarning(t: *Task, now: u64) void {
    t.starvation_warnings +%= 1;
    t.last_starvation_warning_tick = now;
}

fn findByIdLocked(id: u32) ?*Task {
    var cursor = registry_head;
    while (cursor) |t| : (cursor = t.registry_next) {
        if (t.id == id) return t;
    }
    return null;
}

fn findByIdentityLocked(id: u32, generation: u64) ?*Task {
    if (id == 0 or generation == 0) return null;
    const found = findByIdLocked(id) orelse return null;
    if (found.generation != generation) return null;
    return found;
}

fn containsTaskLocked(wanted: *const Task) bool {
    var cursor = registry_head;
    while (cursor) |candidate| : (cursor = candidate.registry_next) {
        if (candidate == wanted) return true;
    }
    return false;
}

pub fn pin(t: *Task) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    if (!containsTaskLocked(t) or t.state == .unused or t.release_in_progress) return false;
    if (t.pin_count == 0xFFFF_FFFF) {
        noteCreateFailure(.memory);
        return false;
    }
    t.pin_count += 1;
    return true;
}

pub fn pinByIdentity(id: u32, generation: u64) ?*Task {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const target = findByIdentityLocked(id, generation) orelse return null;
    if (target.state == .unused or target.release_in_progress or target.pin_count == 0xFFFF_FFFF) return null;
    target.pin_count += 1;
    return target;
}

pub fn unpin(t: *Task) bool {
    const irq_flags = interrupts.saveAndDisable();
    if (!containsTaskLocked(t) or t.pin_count == 0) {
        interrupts.restore(irq_flags);
        return false;
    }
    t.pin_count -= 1;
    const release_token = claimTaskReleaseLocked(t);
    interrupts.restore(irq_flags);
    if (release_token) |token| _ = completeTaskRelease(t, token);
    return true;
}

pub fn kill(id: u32) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const target = findByIdLocked(id) orelse return false;
    if (target.state == .unused or target.state == .dead) return false;
    if (isCurrentTask(target)) return false;
    // Reserved workers are the recovery/I/O/reaper progress floor.  A public
    // hard kill cannot run their defers and would both strand owned work and
    // consume the reserve bundle forever.  Natural return still transitions
    // them to .dead and the normal release path returns the bundle.
    if (target.critical_reserve_slot != NO_CRITICAL_RESERVE_SLOT) return false;
    // Hard-dead transition cannot unwind Zig defers. Refuse it while the
    // exact task still owns a Mutex so the owner can resume, unwind and be
    // retried without leaving a permanently owned synchronization object.
    if (target.held_lock_count != 0 or target.unwind_guard_count != 0) {
        kill_held_lock_deferrals +%= 1;
        return false;
    }
    _ = wait_node.detach(&target.wait_node);
    target.state = .dead;
    target.ready_since_tick = 0;
    target.wake_tick = 0;
    target.blocked_since_tick = 0;
    target.wait_reason = "";
    target.wait_object = 0;
    target.wait_result = .killed;
    bumpInventoryMutationEpochLocked();
    return true;
}

pub fn killIdentity(id: u32, generation: u64) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const target = findByIdentityLocked(id, generation) orelse return false;
    if (target.state == .unused or target.state == .dead) return false;
    if (isCurrentTask(target)) return false;
    if (target.critical_reserve_slot != NO_CRITICAL_RESERVE_SLOT) return false;
    if (target.held_lock_count != 0 or target.unwind_guard_count != 0) {
        kill_held_lock_deferrals +%= 1;
        return false;
    }
    _ = wait_node.detach(&target.wait_node);
    target.state = .dead;
    target.ready_since_tick = 0;
    target.wake_tick = 0;
    target.blocked_since_tick = 0;
    target.wait_reason = "";
    target.wait_object = 0;
    target.wait_result = .killed;
    bumpInventoryMutationEpochLocked();
    return true;
}

pub fn detachWait(t: *Task) bool {
    return wait_node.detach(&t.wait_node);
}

pub fn isAlive(id: u32) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const target = findByIdLocked(id) orelse return false;
    return target.state != .unused and target.state != .dead;
}

pub fn isAliveIdentity(id: u32, generation: u64) bool {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const target = findByIdentityLocked(id, generation) orelse return false;
    return target.state != .unused and target.state != .dead;
}

pub fn releaseDead(id: u32) bool {
    const irq_flags = interrupts.saveAndDisable();
    const target = findByIdLocked(id) orelse {
        interrupts.restore(irq_flags);
        return false;
    };
    if (target.state != .dead) {
        interrupts.restore(irq_flags);
        return false;
    }
    target.retire_pending = true;
    if (isCurrentTask(target)) {
        recycle_guard_hits +%= 1;
        k.puts("TASK RECYCLE GUARD: releaseDead deferred for current task (id=");
        k.putDec(target.id);
        k.puts(")\r\n");
        interrupts.restore(irq_flags);
        return false;
    }
    if (target.pin_count != 0 or
        !wait_node.isDetached(&target.wait_node) or
        target.held_lock_count != 0 or
        target.unwind_guard_count != 0)
    {
        interrupts.restore(irq_flags);
        return false;
    }
    const release_token = claimTaskReleaseLocked(target) orelse {
        interrupts.restore(irq_flags);
        return false;
    };
    interrupts.restore(irq_flags);
    return completeTaskRelease(target, release_token);
}

pub fn releaseDeadIdentity(id: u32, generation: u64) bool {
    const irq_flags = interrupts.saveAndDisable();
    const target = findByIdentityLocked(id, generation) orelse {
        interrupts.restore(irq_flags);
        return false;
    };
    if (target.state != .dead) {
        interrupts.restore(irq_flags);
        return false;
    }
    target.retire_pending = true;
    if (isCurrentTask(target)) {
        recycle_guard_hits +%= 1;
        interrupts.restore(irq_flags);
        return false;
    }
    if (target.pin_count != 0 or
        !wait_node.isDetached(&target.wait_node) or
        target.held_lock_count != 0 or
        target.unwind_guard_count != 0)
    {
        interrupts.restore(irq_flags);
        return false;
    }
    const release_token = claimTaskReleaseLocked(target) orelse {
        interrupts.restore(irq_flags);
        return false;
    };
    interrupts.restore(irq_flags);
    return completeTaskRelease(target, release_token);
}

// Generation-safe, single-transaction retirement for external owners. The
// exact identity is resolved, killed and claimed under one IRQ boundary; an
// already absent generation is success rather than a retry loop. Fallible
// resource release remains anchored by retire_pending/release_in_progress.
pub fn retireIdentity(id: u32, generation: u64) RetireResult {
    if (id == 0 or generation == 0) return .gone;
    const irq_flags = interrupts.saveAndDisable();
    const target = findByIdentityLocked(id, generation) orelse {
        interrupts.restore(irq_flags);
        return .gone;
    };
    if (isCurrentTask(target)) {
        interrupts.restore(irq_flags);
        return .pending;
    }
    if (target.state != .dead) {
        if (target.state == .unused or
            target.critical_reserve_slot != NO_CRITICAL_RESERVE_SLOT or
            target.held_lock_count != 0 or
            target.unwind_guard_count != 0)
        {
            if (target.held_lock_count != 0 or target.unwind_guard_count != 0) kill_held_lock_deferrals +%= 1;
            interrupts.restore(irq_flags);
            return .pending;
        }
        _ = wait_node.detach(&target.wait_node);
        target.state = .dead;
        target.ready_since_tick = 0;
        target.wake_tick = 0;
        target.blocked_since_tick = 0;
        target.wait_reason = "";
        target.wait_object = 0;
        target.wait_result = .killed;
        bumpInventoryMutationEpochLocked();
    }
    target.retire_pending = true;
    if (target.pin_count != 0 or
        !wait_node.isDetached(&target.wait_node) or
        target.held_lock_count != 0 or
        target.unwind_guard_count != 0)
    {
        interrupts.restore(irq_flags);
        return .pending;
    }
    const release_token = claimTaskReleaseLocked(target) orelse {
        interrupts.restore(irq_flags);
        return .pending;
    };
    interrupts.restore(irq_flags);
    return if (completeTaskRelease(target, release_token)) .released else .pending;
}

fn canReapLocked(t: *Task) bool {
    return !t.release_in_progress and releaseEligibleLocked(t);
}

fn releaseEligibleLocked(t: *Task) bool {
    return t.retire_pending and
        t.state == .dead and
        t.pin_count == 0 and
        !isCurrentTask(t) and
        wait_node.isDetached(&t.wait_node) and
        t.held_lock_count == 0 and
        t.unwind_guard_count == 0;
}

fn claimTaskReleaseLocked(t: *Task) ?task_context.UnwindToken {
    if (!canReapLocked(t)) return null;
    // Acquire the releaser's hard-kill deferral before changing victim state.
    // VM/heap teardown may yield; without this token an external hard kill
    // could discard the only live pointer while the victim is between phases.
    const token = task_context.enterUnwind();
    if (!token.admitted()) return null;
    t.release_in_progress = true;
    return token;
}

// Called only from normal task context after a context switch or an unpin.
// Fallible child resources are released while the dead Task remains linked.
// On persistent failure this invocation stops after one attempt, avoiding an
// immediate retry spin while preserving retire_pending and the exact Range ID.
pub fn reapDeferred() u32 {
    var reaped: u32 = 0;
    while (true) {
        const irq_flags = interrupts.saveAndDisable();
        var victim: ?*Task = null;
        var release_token: ?task_context.UnwindToken = null;
        var cursor = registry_head;
        while (cursor) |candidate| {
            const following = candidate.registry_next;
            if (claimTaskReleaseLocked(candidate)) |token| {
                victim = candidate;
                release_token = token;
                break;
            }
            cursor = following;
        }
        interrupts.restore(irq_flags);

        const retired = victim orelse break;
        if (!completeTaskRelease(retired, release_token.?)) break;
        reaped +%= 1;
    }
    _ = retryOneUnpublishedRollback();
    return reaped;
}

fn completeTaskRelease(t: *Task, release_token: task_context.UnwindToken) bool {
    defer _ = task_context.leaveUnwind(release_token);

    if (!releaseTaskResources(t)) {
        clearTaskReleaseClaim(t);
        return false;
    }

    const irq_flags = interrupts.saveAndDisable();
    if (!containsTaskLocked(t) or !t.release_in_progress or !releaseEligibleLocked(t)) {
        if (containsTaskLocked(t)) t.release_in_progress = false;
        interrupts.restore(irq_flags);
        k.puts("TASK RELEASE INVARIANT: retry anchor changed\r\n");
        return false;
    }

    if (t.critical_reserve_slot != NO_CRITICAL_RESERVE_SLOT) {
        const slot: usize = t.critical_reserve_slot;
        if (slot >= critical_reserve.len) {
            t.release_in_progress = false;
            interrupts.restore(irq_flags);
            k.puts("Critical task reserve slot invalid\r\n");
            return false;
        }
        const bundle = &critical_reserve[slot];
        if (bundle.task != t or !bundle.in_use) {
            t.release_in_progress = false;
            interrupts.restore(irq_flags);
            k.puts("Critical task reserve ownership mismatch\r\n");
            return false;
        }
        unlinkRegistryLocked(t);
        resetCriticalTaskLocked(bundle, slot);
        bundle.in_use = false;
        if (critical_reserve_in_use != 0) critical_reserve_in_use -= 1;
        interrupts.restore(irq_flags);
        return true;
    }

    // The outer UnwindToken is now the temporary anchor across the only
    // necessarily unlinked operation: freeing the Task object itself.
    unlinkRegistryLocked(t);
    interrupts.restore(irq_flags);
    if (freeTaskMemory(t)) return true;

    const retry_flags = interrupts.saveAndDisable();
    t.release_in_progress = false;
    t.retire_pending = true;
    linkRegistryLocked(t);
    interrupts.restore(retry_flags);
    k.puts("Task object release pending retry\r\n");
    return false;
}

fn clearTaskReleaseClaim(t: *Task) void {
    const irq_flags = interrupts.saveAndDisable();
    if (containsTaskLocked(t)) {
        t.release_in_progress = false;
        t.retire_pending = true;
    }
    interrupts.restore(irq_flags);
}

fn releaseTaskResources(t: *Task) bool {
    if (t.held_lock_count != 0 or t.unwind_guard_count != 0) {
        k.puts("TASK RELEASE INVARIANT: owned synchronization remains id=");
        k.putDec(t.id);
        k.puts("\r\n");
        interrupts.haltForever();
    }
    if (!t.wait_node.reset()) return false;
    if (t.fpu_state) |memory| {
        if (heap.free(memory) != .ok) {
            k.puts("Task FPU state release pending retry\r\n");
            return false;
        }
        t.fpu_state = null;
        t.fpu_state_bytes = 0;
        t.fpu_state_valid = false;
    }
    if (t.critical_reserve_slot == NO_CRITICAL_RESERVE_SLOT and !releaseTaskStack(t)) return false;
    return true;
}

fn freeTaskMemory(t: *Task) bool {
    const bytes: [*]u8 = @ptrCast(t);
    if (heap.free(bytes[0..@sizeOf(Task)]) != .ok) {
        k.puts("Task object release failed\r\n");
        return false;
    }
    return true;
}

fn releaseTaskStack(t: *Task) bool {
    if (t.stack_range_id == 0) {
        t.stack_base = 0;
        t.stack_top = 0;
        return true;
    }
    var release_direct = false;
    const stack: GuardedStack = .{
        .range_id = t.stack_range_id,
        .base = t.stack_base,
        .top = t.stack_top,
    };
    const irq_flags = interrupts.saveAndDisable();
    if (stack_cache_count < stack_cache.len) {
        stack_cache[stack_cache_count] = stack;
        stack_cache_count += 1;
    } else {
        release_direct = true;
    }
    interrupts.restore(irq_flags);
    if (release_direct and !releaseGuardedStackDirect(stack)) return false;
    t.stack_range_id = 0;
    t.stack_base = 0;
    t.stack_top = 0;
    return true;
}

// 0.56.18: Untere Schranke fuer den naechsten faelligen Timeout (Befund
// 4.3). onTick ueberspringt den Task-Scan, solange now < min_wake_tick.
// Die Schranke darf zu NIEDRIG sein (weckt nur einen unnoetigen Scan,
// der sie neu berechnet), aber nie zu hoch - deshalb wird sie hier bei
// jedem neuen Timeout nur nach unten gezogen und ausschliesslich vom
// onTick-Scan neu angehoben.
pub const NO_WAKE_TICK: u64 = 0xFFFF_FFFF_FFFF_FFFF;
var min_wake_tick: u64 = NO_WAKE_TICK;

pub fn minWakeTick() u64 {
    return min_wake_tick;
}

pub fn setMinWakeTick(value: u64) void {
    min_wake_tick = value;
}

fn noteWakeTick(wake_tick: u64) void {
    if (wake_tick != 0 and wake_tick < min_wake_tick) min_wake_tick = wake_tick;
}

pub fn beginWait(t: *Task, wake_tick: u64, reason: []const u8, object: u64) void {
    t.wake_tick = wake_tick;
    t.blocked_since_tick = timer.tickCount();
    t.wait_reason = reason;
    t.wait_object = object;
    t.wait_result = .none;
    t.ready_since_tick = 0;
    t.state = .blocked;
    noteWakeTick(wake_tick);
    bumpInventoryMutationEpoch();
}

pub fn finishWait(t: *Task, result: WaitResult) u64 {
    _ = wait_node.detach(&t.wait_node);
    const now = timer.tickCount();
    const wait_ticks = ticksSince(now, t.blocked_since_tick);
    t.last_wait_ticks = wait_ticks;
    if (wait_ticks > t.max_wait_ticks) t.max_wait_ticks = wait_ticks;
    t.state = .ready;
    t.ready_since_tick = now;
    t.wake_tick = 0;
    t.blocked_since_tick = 0;
    t.wait_reason = "";
    t.wait_object = 0;
    t.wait_result = result;
    bumpInventoryMutationEpoch();
    return wait_ticks;
}

const DEADMAN_SNAPSHOT_CAPACITY: usize = 6;
const DEADMAN_TEXT_CAPACITY: usize = 20;

pub const DeadmanTaskSnapshot = struct {
    id: u32 = 0,
    held_lock_count: u32 = 0,
    unwind_guard_count: u32 = 0,
    blocked_for_ticks: u64 = 0,
    wait_object: u64 = 0,
    suspicious_mutex_holder: bool = false,
    name_len: u8 = 0,
    reason_len: u8 = 0,
    name: [DEADMAN_TEXT_CAPACITY]u8 = .{0} ** DEADMAN_TEXT_CAPACITY,
    reason: [DEADMAN_TEXT_CAPACITY]u8 = .{0} ** DEADMAN_TEXT_CAPACITY,
};

pub const DeadmanSnapshot = struct {
    captured_tick: u64 = 0,
    wedged_mutex_holder: bool = false,
    blocked_total: u32 = 0,
    omitted: u32 = 0,
    count: usize = 0,
    tasks: [DEADMAN_SNAPSHOT_CAPACITY]DeadmanTaskSnapshot =
        .{DeadmanTaskSnapshot{}} ** DEADMAN_SNAPSHOT_CAPACITY,
};

/// Takes one coherent, bounded task image for the deadman. Timer IRQ wakeups
/// mutate state/reason/blocked_since, so the probe and the copied diagnostic
/// records share one IRQ-disabled snapshot boundary. Rendering happens later
/// with IRQs restored.
///
/// Only a real Mutex count is a deadlock signal. UnwindGuards are lifetime /
/// hard-kill deferrals and deliberately span normal storage and network waits;
/// they remain informational in the snapshot but never trigger the deadman.
pub fn captureDeadmanSnapshot(now: u64, threshold: u64) DeadmanSnapshot {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);

    var out = DeadmanSnapshot{ .captured_tick = now };
    var cursor = registry_head;
    while (cursor) |candidate| : (cursor = candidate.registry_next) {
        const blocked_for = blockedDurationAt(candidate, now) orelse continue;
        out.blocked_total +|= 1;
        if (candidate.held_lock_count == 0 or blocked_for <= threshold) continue;
        out.wedged_mutex_holder = true;
        appendDeadmanTask(&out, candidate, blocked_for, true);
    }

    // Root-cause Mutex holders were inserted first. Fill the remaining
    // bounded rows with ordinary waiters for context, never displacing them.
    cursor = registry_head;
    while (cursor) |candidate| : (cursor = candidate.registry_next) {
        if (out.count >= out.tasks.len) break;
        const blocked_for = blockedDurationAt(candidate, now) orelse continue;
        if (candidate.held_lock_count != 0 and blocked_for > threshold) continue;
        appendDeadmanTask(&out, candidate, blocked_for, false);
    }

    const captured: u32 = @intCast(out.count);
    out.omitted = if (out.blocked_total > captured) out.blocked_total - captured else 0;
    return out;
}

pub fn dumpDeadmanSnapshotToDiag(snapshot: *const DeadmanSnapshot) void {
    diag_screen.line("[DEADMAN] blocked snapshot (! = mutex holder)");
    for (snapshot.tasks[0..snapshot.count]) |entry| {
        diag_screen.write(if (entry.suspicious_mutex_holder) "!#" else " #");
        diag_screen.writeDec(entry.id);
        diag_screen.write(" ");
        diag_screen.write(entry.name[0..@as(usize, entry.name_len)]);
        diag_screen.write(" w=");
        diag_screen.write(entry.reason[0..@as(usize, entry.reason_len)]);
        diag_screen.write(" lk=");
        diag_screen.writeDec(entry.held_lock_count);
        diag_screen.write(" ug=");
        diag_screen.writeDec(entry.unwind_guard_count);
        diag_screen.write(" t=");
        diag_screen.writeDec(entry.blocked_for_ticks);
        diag_screen.endLine();
    }
    if (snapshot.count == 0) diag_screen.line("  (no blocked tasks captured)");
    if (snapshot.omitted != 0) {
        diag_screen.write("  ... blocked tasks omitted=");
        diag_screen.writeDec(snapshot.omitted);
        diag_screen.endLine();
    }
}

pub fn dumpDeadmanSnapshotToLog(snapshot: *const DeadmanSnapshot) void {
    k.puts("[DEADMAN] blocked snapshot total=");
    k.putDec(snapshot.blocked_total);
    k.puts(" captured=");
    k.putDec(snapshot.count);
    k.puts(" omitted=");
    k.putDec(snapshot.omitted);
    k.puts("\r\n");
    for (snapshot.tasks[0..snapshot.count]) |entry| {
        k.puts(if (entry.suspicious_mutex_holder) "  !#" else "   #");
        k.putDec(entry.id);
        k.puts(" ");
        k.puts(entry.name[0..@as(usize, entry.name_len)]);
        k.puts(" wait=");
        k.puts(entry.reason[0..@as(usize, entry.reason_len)]);
        k.puts(" object=0x");
        k.putHex(entry.wait_object, 16);
        k.puts(" lock=");
        k.putDec(entry.held_lock_count);
        k.puts(" unwind=");
        k.putDec(entry.unwind_guard_count);
        k.puts(" blocked_for=");
        k.putDec(entry.blocked_for_ticks);
        k.puts("\r\n");
    }
}

fn blockedDurationAt(candidate: *const Task, now: u64) ?u64 {
    if (candidate.state != .blocked) return null;
    return if (now >= candidate.blocked_since_tick) now - candidate.blocked_since_tick else 0;
}

fn appendDeadmanTask(
    snapshot: *DeadmanSnapshot,
    candidate: *const Task,
    blocked_for: u64,
    suspicious_mutex_holder: bool,
) void {
    if (snapshot.count >= snapshot.tasks.len) return;
    var entry = &snapshot.tasks[snapshot.count];
    entry.* = .{
        .id = candidate.id,
        .held_lock_count = candidate.held_lock_count,
        .unwind_guard_count = candidate.unwind_guard_count,
        .blocked_for_ticks = blocked_for,
        .wait_object = candidate.wait_object,
        .suspicious_mutex_holder = suspicious_mutex_holder,
    };
    entry.name_len = copyDeadmanText(&entry.name, candidate.name);
    entry.reason_len = copyDeadmanText(
        &entry.reason,
        if (candidate.wait_reason.len != 0) candidate.wait_reason else "unknown",
    );
    snapshot.count += 1;
}

fn copyDeadmanText(out: *[DEADMAN_TEXT_CAPACITY]u8, text: []const u8) u8 {
    const len = @min(out.len, text.len);
    @memcpy(out[0..len], text[0..len]);
    return @intCast(len);
}

pub fn dump() void {
    const now = timer.tickCount();
    var ready: u32 = 0;
    var running: u32 = 0;
    var blocked: u32 = 0;
    var dead: u32 = 0;
    var workers: u32 = 0;
    var count_cursor = registry_head;
    while (count_cursor) |t| : (count_cursor = t.registry_next) {
        if (t.entry != null and t.state != .unused and t.state != .dead) workers += 1;
        switch (t.state) {
            .ready => ready += 1,
            .running => running += 1,
            .blocked => blocked += 1,
            .dead => dead += 1,
            .unused => {},
        }
    }

    k.puts("  Tasks: ");
    k.putDec(task_count);
    k.puts(" registry=dynamic");
    k.puts(" stack_guard=");
    k.putDec(STACK_GUARD_SIZE);
    k.puts(" ready=");
    k.putDec(ready);
    k.puts(" running=");
    k.putDec(running);
    k.puts(" blocked=");
    k.putDec(blocked);
    k.puts(" dead=");
    k.putDec(dead);
    k.puts(" workers=");
    k.putDec(workers);
    k.puts("\r\n");
    var dump_cursor = registry_head;
    while (dump_cursor) |task| : (dump_cursor = task.registry_next) {
        k.puts("    #");
        k.putDec(task.id);
        k.puts(" ");
        k.puts(task.name);
        k.puts(" ");
        k.puts(stateName(task.state));
        k.puts(" prio=");
        k.puts(switch (task.priority) {
            .high => "high",
            .normal => "normal",
            .low => "low",
        });
        if (task.entry != null) {
            k.puts(" worker=yes");
        }
        if (task.preempt_disable_depth != 0 or task.preempt_disable_max_depth != 0) {
            k.puts(" preempt=");
            k.putDec(task.preempt_disable_depth);
            k.puts("/");
            k.putDec(task.preempt_disable_max_depth);
        }
        if (task.long_run_warnings != 0 or task.starvation_warnings != 0) {
            k.puts(" runtime_warn=");
            k.putDec(task.long_run_warnings);
            k.puts("/");
            k.putDec(task.starvation_warnings);
        }
        if (task.state == .blocked) {
            k.puts(" wait=");
            if (task.wait_reason.len > 0) {
                k.puts(task.wait_reason);
            } else {
                k.puts("unknown");
            }
            k.puts(" wake=");
            k.putDec(task.wake_tick);
            k.puts(" blocked_for=");
            if (now >= task.blocked_since_tick) {
                k.putDec(now - task.blocked_since_tick);
            } else {
                k.putDec(0);
            }
        }
        k.puts(" rsp=0x");
        k.putHex(task.rsp, 16);
        k.puts(" stack=0x");
        k.putHex(task.stack_base, 16);
        k.puts("..0x");
        k.putHex(task.stack_top, 16);
        if (task.fpu_state_valid) {
            k.puts(" fpu=");
            k.putDec(task.fpu_state_bytes);
        }
        k.puts("\r\n");
    }
}

extern fn taskEntryTrampoline() callconv(.c) noreturn;

fn initialRsp(stack_top: u64) u64 {
    var sp = stack_top & ~@as(u64, 0xF);
    sp = push(sp, @intFromPtr(&taskEntryTrampoline));
    var i: usize = 0;
    while (i < SAVED_REG_COUNT) : (i += 1) {
        sp = push(sp, 0);
    }
    return sp;
}

fn push(sp_in: u64, value: u64) u64 {
    const sp = sp_in - 8;
    const slot: *u64 = @ptrFromInt(sp);
    slot.* = value;
    return sp;
}

fn ticksSince(now: u64, then: u64) u64 {
    if (then == 0 or now < then) return 0;
    return now - then;
}

pub fn stateName(state: State) []const u8 {
    return switch (state) {
        .unused => "unused",
        .ready => "ready",
        .running => "running",
        .blocked => "blocked",
        .dead => "dead",
    };
}

pub fn stateCode(state: State) u32 {
    return switch (state) {
        .unused => 0,
        .ready => 1,
        .running => 2,
        .blocked => 3,
        .dead => 4,
    };
}
