// Native dynamic Task owner. The private interface transports opaque handles
// and fixed C values; no native Rust layout or mutable mirror crosses it.
const k = @import("../kernel/log.zig");
const diag_screen = @import("../kernel/diag_screen.zig");
const timer = @import("../kernel/timer.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const wait_node = @import("wait_node.zig");
comptime {
    _ = @import("task_vm_bridge.zig");
}
// Preserve the original indirect host fixture discovery after moving Task
// construction into Rust. These are existing owner cases, with no new gate.
comptime {
    if (@import("builtin").is_test) {
        _ = @import("task_index.zig");
        _ = @import("wait_node.zig");
        _ = @import("initial_stack.zig");
        _ = @import("../kernel/smp_policy.zig");
        _ = @import("../memory/heap_policy.zig");
    }
}
pub const max_tasks: usize = 0;
pub const stack_size: usize = 64 * 1024;
pub const stack_guard_size: usize = 16 * 1024;
pub const held_lock_capacity: usize = 8;
pub const Task = opaque {};
pub const State = enum(u8) {
    unused,
    ready,
    running,
    blocked,
    dead,
};

// Public modules cannot select scheduler policy. Kernel owners assign one of
// these internal roles before publishing a Task ready; Priority remains the
// compact diagnostic lane derived from that role.
pub const Priority = enum(u8) {
    high = 0,
    normal = 1,
    low = 2,
};

pub const Role = enum(u8) {
    input = 0,
    short_completion = 1,
    interactive = 2,
    batch = 3,
};

pub const role_count: usize = 4;
pub const no_dispatch_rank: u8 = 0xFF;

pub const RolePolicy = struct {
    priority: Priority,
    run_budget_ticks: u32,
    dispatch_budget: u16,
};

// Input and short completions may jump normal work only for a small activation
// budget. Once either bound is consumed they run as normal interactive work
// until they block and are woken again. The every-eighth scheduler fairness
// turn remains an independent system-wide bound for repeated activations.
pub fn rolePolicy(role: Role) RolePolicy {
    return switch (role) {
        .input => .{ .priority = .high, .run_budget_ticks = 2, .dispatch_budget = 4 },
        .short_completion => .{ .priority = .high, .run_budget_ticks = 4, .dispatch_budget = 4 },
        .interactive => .{ .priority = .normal, .run_budget_ticks = 0, .dispatch_budget = 0 },
        .batch => .{ .priority = .low, .run_budget_ticks = 0, .dispatch_budget = 0 },
    };
}

pub fn roleName(role: Role) []const u8 {
    return switch (role) {
        .input => "input",
        .short_completion => "completion",
        .interactive => "interactive",
        .batch => "batch",
    };
}

pub const WaitResult = enum(u32) {
    none = 0,
    signaled = 1,
    timeout = 2,
    cancelled = 3,
    killed = 4,
    failed = 5,
};

// The scheduler-facing owner relation is immutable after a blocked task is
// published.  Keeping the untyped pointer here avoids a Task <-> R4X import
// cycle while letting program/r4x.zig recover its exact stable owner in O(1).
pub const ExecutionOwnerKind = enum(u8) {
    none,
    program_thread,
    async_io,
    driver_thread,
};

pub const ExecutionOwner = struct {
    kind: ExecutionOwnerKind = .none,
    context: ?*anyopaque = null,
};

pub const Entry = *const fn () callconv(.c) void;

pub const HeldLockRecord = struct {
    object_id: u64 = 0,
    name: []const u8 = "",
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

pub const CreateFailureStats = extern struct {
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

pub const Summary = extern struct {
    max_tasks: u32 = 0,
    total: u32 = 0,
    ready: u32 = 0,
    running: u32 = 0,
    blocked: u32 = 0,
    dead: u32 = 0,
    workers: u32 = 0,
};

pub const InventorySnapshot = extern struct {
    task_id: u32 = 0,
    state: State = .unused,
    generation: u64 = 0,
    created_tick: u64 = 0,
    last_run_tick: u64 = 0,
    wake_tick: u64 = 0,
    runtime_ticks: u64 = 0,
};

pub const InventoryPage = extern struct {
    epoch: u64 = 0,
    total: u32 = 0,
    returned: u32 = 0,
    has_more: bool = false,
};

pub const StackTelemetryStats = extern struct {
    created: u64 = 0,
    released: u64 = 0,
    high_water_total_bytes: u64 = 0,
    high_water_max_bytes: u64 = 0,
    create_cycles_total: u64 = 0,
    create_cycles_max: u64 = 0,
    release_cycles_total: u64 = 0,
    release_cycles_max: u64 = 0,
};

pub const DeadmanTaskSnapshot = extern struct {
    id: u32 = 0,
    held_lock_count: u32 = 0,
    unwind_guard_count: u32 = 0,
    blocked_for_ticks: u64 = 0,
    wait_object: u64 = 0,
    suspicious_mutex_holder: bool = false,
    name_len: u8 = 0,
    reason_len: u8 = 0,
    name: [20]u8 = .{0} ** 20,
    reason: [20]u8 = .{0} ** 20,
};

pub const DeadmanSnapshot = extern struct {
    captured_tick: u64 = 0,
    wedged_mutex_holder: bool = false,
    blocked_total: u32 = 0,
    omitted: u32 = 0,
    count: usize = 0,
    tasks: [6]DeadmanTaskSnapshot =
        .{DeadmanTaskSnapshot{}} ** 6,
};

const Span = extern struct { pointer: ?[*]const u8 = null, len: usize = 0 };
const CreateRequest = extern struct {
    name: [*]const u8,
    name_len: usize,
    state: State,
    role: Role,
    needs_fpu: bool,
    smp_eligible: bool,
    smp_r4x: bool,
    owner_kind: ExecutionOwnerKind,
    owner_context: ?*anyopaque,
    entry: ?Entry,
};
extern fn r4os_rust_task_create(request: *const CreateRequest, critical: bool, failure: *CreateFailure) ?*Task;
extern fn r4os_rust_task_mutate(operation: u8, task: ?*Task, a: u64, b: u64, text: ?[*]const u8, len: usize) u64;
extern fn r4os_rust_task_control(operation: u8, a: u64, b: u64) u64;
extern fn r4os_rust_task_identity(operation: u8, id: u32, generation: u64, context: ?*anyopaque, argument: u64) u64;
extern fn r4os_rust_task_snapshot(operation: u8, argument: u64, output: ?*anyopaque, len: usize) u64;
extern fn r4os_rust_task_inventory(after: u64, output: [*]InventorySnapshot, len: usize, page: *InventoryPage) void;
extern fn r4os_rust_task_deadman(now: u64, threshold: u64, output: *DeadmanSnapshot) void;
extern fn r4os_rust_task_field(task: *const Task, field: u8) u64;
extern fn r4os_rust_task_set_field(task: *Task, field: u8, value: u64) void;
extern fn r4os_rust_task_span(task: *const Task, field: u8, output: *Span) bool;
fn mutate(op: u8, target: ?*Task, a: u64, b: u64) u64 {
    return r4os_rust_task_mutate(op, target, a, b, null, 0);
}
fn ptr(value: u64) ?*Task {
    return if (value == 0) null else @ptrFromInt(value);
}
fn create(name: []const u8, state: State, needs_fpu: bool, entry: ?Entry, role: Role, parallel: bool, r4x: bool, failure: *CreateFailure, owner: ExecutionOwner, critical: bool) ?*Task {
    const request: CreateRequest = .{ .name = name.ptr, .name_len = name.len, .state = state, .role = role, .needs_fpu = needs_fpu, .smp_eligible = parallel, .smp_r4x = r4x, .owner_kind = owner.kind, .owner_context = owner.context, .entry = entry };
    return r4os_rust_task_create(&request, critical, failure);
}
pub fn init() bool {
    return r4os_rust_task_control(0, 0, 0) != 0;
}
pub fn createKernelThread(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelThreadWithFailure(name, entry, &failure);
}

pub fn createKernelThreadWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return create(name, .ready, true, entry, .interactive, false, false, failure_out, .{}, false);
}

pub fn createKernelThreadWithRole(name: []const u8, entry: Entry, role: Role) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .ready, true, entry, role, false, false, &failure, .{}, false);
}

pub fn createParallelKernelThreadWithRole(name: []const u8, entry: Entry, role: Role) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .ready, true, entry, role, true, false, &failure, .{}, false);
}

pub fn createKernelThreadBlocked(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelThreadBlockedWithFailure(name, entry, &failure);
}

pub fn createKernelThreadBlockedWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return create(name, .blocked, true, entry, .interactive, false, false, failure_out, .{}, false);
}

pub fn createKernelThreadBlockedWithRole(name: []const u8, entry: Entry, role: Role) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .blocked, true, entry, role, false, false, &failure, .{}, false);
}

// Module-capable stack/FPU construction, independent of shared Driver Work.
// The provider owns the initial unwind count until its callback epilogue.
pub fn createDriverThreadBlocked(entry: Entry, context: *anyopaque, parallel: bool) ?*Task {
    var failure: CreateFailure = .none;
    return create("r4d-thread", .blocked, true, entry, .interactive, parallel, parallel, &failure, .{ .kind = .driver_thread, .context = context }, false);
}

// Only an unpublished blocked driver Task may abandon its construction
// guard. A published/running callback must return and release it itself.
pub fn abandonDriverThread(id: u32, generation: u64, context: *const anyopaque) bool {
    return r4os_rust_task_identity(8, id, generation, @constCast(context), 0) != 0;
}

pub fn createParallelThreadBlocked(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .blocked, true, entry, .interactive, true, true, &failure, .{}, false);
}

pub fn createParallelThreadBlockedWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return create(name, .blocked, true, entry, .interactive, true, true, failure_out, .{}, false);
}

// R4X owners that still depend on global singleton state remain on the BSP
// until their complete call graph has been audited.  This is an internal
// ownership policy, not a public CPU-affinity ABI.
pub fn createLegacyThreadBlocked(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .blocked, true, entry, .interactive, false, false, &failure, .{}, false);
}

pub fn createLegacyThreadBlockedWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return create(name, .blocked, true, entry, .interactive, false, false, failure_out, .{}, false);
}

pub fn createKernelWorkerBlocked(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelWorkerBlockedWithFailure(name, entry, &failure);
}

pub fn createKernelWorkerBlockedWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return create(name, .blocked, true, entry, .interactive, false, false, failure_out, .{}, false);
}

pub fn createKernelWorkerBlockedWithRole(name: []const u8, entry: Entry, role: Role) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .blocked, true, entry, role, false, false, &failure, .{}, false);
}

pub fn createParallelWorkerBlocked(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .blocked, true, entry, .interactive, true, false, &failure, .{}, false);
}

pub fn createParallelWorkerBlockedWithRole(name: []const u8, entry: Entry, role: Role) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .blocked, true, entry, role, true, false, &failure, .{}, false);
}

pub fn createCpuIdleTask(cpu_index: u32) ?*Task {
    return ptr(r4os_rust_task_control(32, cpu_index, 0));
}

// Critical workers consume one of four boot-time Task+Stack+FPU bundles. Normal
// admission can never borrow these bundles, so an allocator/VM pressure event
// cannot prevent an already designed recovery, I/O or reaper path from making
// progress. Storage/reaper callbacks may enter modules, so their FPU buffers
// are also reserved at boot and retained across reuse, without late allocation.
pub fn createKernelThreadCritical(name: []const u8, entry: Entry) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelThreadCriticalWithFailure(name, entry, &failure);
}

pub fn createKernelThreadCriticalWithFailure(name: []const u8, entry: Entry, failure_out: *CreateFailure) ?*Task {
    return create(name, .ready, true, entry, .interactive, false, false, failure_out, .{}, true);
}

pub fn createKernelThreadCriticalWithRole(name: []const u8, entry: Entry, role: Role) ?*Task {
    var failure: CreateFailure = .none;
    return create(name, .ready, true, entry, role, false, false, &failure, .{}, true);
}

pub fn createKernelTask(name: []const u8, state: State) ?*Task {
    var failure: CreateFailure = .none;
    return createKernelTaskWithFailure(name, state, &failure);
}

pub fn createKernelTaskWithFailure(name: []const u8, state: State, failure_out: *CreateFailure) ?*Task {
    return create(name, state, true, null, .interactive, false, false, failure_out, .{}, false);
}

pub const Field = enum(u8) {
    registry_prev = 0,
    registry_next = 1,
    ready_prev = 2,
    ready_next = 3,
    ready_linked = 4,
    ready_order = 5,
    ready_rank = 6,
    ready_index = 7,
    timeout_prev = 8,
    timeout_next = 9,
    timeout_linked = 10,
    timeout_sequence = 11,
    timeout_index = 12,
    reap_prev = 13,
    reap_next = 14,
    reap_linked = 15,
    generation = 16,
    pin_count = 17,
    retire_pending = 18,
    release_in_progress = 19,
    rollback_pending = 20,
    rollback_next = 21,
    critical_reserve_slot = 22,
    id = 23,
    home_cpu = 24,
    home_cpu_bound = 25,
    running_cpu = 26,
    smp_eligible = 27,
    smp_r4x_work = 28,
    cpu_idle = 29,
    name = 30,
    state = 31,
    rsp = 32,
    wake_tick = 33,
    blocked_since_tick = 34,
    wait_reason = 35,
    wait_object = 36,
    wait_result = 37,
    wait_node = 38,
    held_lock_count = 39,
    unwind_guard_count = 40,
    wait_handoff_guard_pending = 41,
    held_locks = 42,
    stack_base = 43,
    stack_top = 44,
    stack_range_id = 45,
    stack_high_water_bytes = 46,
    stack_high_water_measured = 47,
    role = 48,
    priority = 49,
    role_budget_ticks_remaining = 50,
    role_budget_dispatches_remaining = 51,
    role_budget_exhausted_recorded = 52,
    inherited_rank_counts = 53,
    donation_budget_ticks_remaining = 54,
    donation_budget_dispatches_remaining = 55,
    donation_budget_exhausted_recorded = 56,
    role_activations = 57,
    role_budget_exhaustions = 58,
    donation_budget_exhaustions = 59,
    execution_owner_kind = 60,
    execution_owner_context = 61,
    entry = 62,
    created_tick = 63,
    ready_since_tick = 64,
    last_scheduled_tick = 65,
    last_accounted_tick = 66,
    last_yield_tick = 67,
    last_ready_latency_ticks = 68,
    max_ready_latency_ticks = 69,
    last_wait_ticks = 70,
    max_wait_ticks = 71,
    run_ticks = 72,
    switches_in = 73,
    max_run_without_switch_ticks = 74,
    preempt_disable_depth = 75,
    preempt_disable_max_depth = 76,
    preemption_probe_hits = 77,
    preemption_deferred_ticks = 78,
    max_preemption_deferred_ticks = 79,
    long_run_warnings = 80,
    starvation_warnings = 81,
    last_long_run_warning_tick = 82,
    last_starvation_warning_tick = 83,
    fpu_state = 84,
    fpu_state_bytes = 85,
    fpu_state_valid = 86,
    uses_fpu = 87,
};

const Pair = extern struct { low: u64, high: u64 };
extern fn r4os_rust_task_sequence(task: *const Task, timeout: bool, output: *Pair) void;
extern fn r4os_rust_task_donation_counts(task: *const Task, output: *[4]u16) void;
extern fn r4os_rust_task_set_span(task: *Task, field: u8, span: Span, present: bool) void;
extern fn r4os_rust_task_rank(task: *const Task, wake: bool) u8;
const RawHeldLock = extern struct { object_id: u64, name: Span, rank: u16, mode_no_sleep: bool, active: bool };
extern fn r4os_rust_task_held_lock(task: *const Task, index: usize, write: bool, record: *RawHeldLock) void;
pub fn heldLock(target: *const Task, index: usize) HeldLockRecord {
    var raw: RawHeldLock = undefined;
    r4os_rust_task_held_lock(target, index, false, &raw);
    return .{ .object_id = raw.object_id, .name = if (raw.name.len == 0) "" else raw.name.pointer.?[0..raw.name.len], .rank = raw.rank, .mode_no_sleep = raw.mode_no_sleep, .active = raw.active };
}
pub fn setHeldLock(target: *Task, index: usize, value: HeldLockRecord) void {
    var raw: RawHeldLock = .{ .object_id = value.object_id, .name = .{ .pointer = value.name.ptr, .len = value.name.len }, .rank = value.rank, .mode_no_sleep = value.mode_no_sleep, .active = value.active };
    r4os_rust_task_held_lock(target, index, true, &raw);
}
pub fn FieldType(comptime field: Field) type {
    return switch (field) {
        .registry_prev => ?*Task,
        .registry_next => ?*Task,
        .ready_prev => ?*Task,
        .ready_next => ?*Task,
        .ready_linked => bool,
        .ready_order => u128,
        .ready_rank => u8,
        .ready_index => @compileError("Native index is private to task owner"),
        .timeout_prev => ?*Task,
        .timeout_next => ?*Task,
        .timeout_linked => bool,
        .timeout_sequence => u128,
        .timeout_index => @compileError("Native index is private to task owner"),
        .reap_prev => ?*Task,
        .reap_next => ?*Task,
        .reap_linked => bool,
        .generation => u64,
        .pin_count => u32,
        .retire_pending => bool,
        .release_in_progress => bool,
        .rollback_pending => bool,
        .rollback_next => ?*Task,
        .critical_reserve_slot => u8,
        .id => u32,
        .home_cpu => u8,
        .home_cpu_bound => bool,
        .running_cpu => u8,
        .smp_eligible => bool,
        .smp_r4x_work => bool,
        .cpu_idle => bool,
        .name => []const u8,
        .state => State,
        .rsp => u64,
        .wake_tick => u64,
        .blocked_since_tick => u64,
        .wait_reason => []const u8,
        .wait_object => u64,
        .wait_result => WaitResult,
        .wait_node => *wait_node.Node,
        .held_lock_count => u32,
        .unwind_guard_count => u32,
        .wait_handoff_guard_pending => bool,
        .held_locks => [8]HeldLockRecord,
        .stack_base => u64,
        .stack_top => u64,
        .stack_range_id => u32,
        .stack_high_water_bytes => u64,
        .stack_high_water_measured => bool,
        .role => Role,
        .priority => Priority,
        .role_budget_ticks_remaining => u32,
        .role_budget_dispatches_remaining => u16,
        .role_budget_exhausted_recorded => bool,
        .inherited_rank_counts => [4]u16,
        .donation_budget_ticks_remaining => u32,
        .donation_budget_dispatches_remaining => u16,
        .donation_budget_exhausted_recorded => bool,
        .role_activations => u64,
        .role_budget_exhaustions => u64,
        .donation_budget_exhaustions => u64,
        .execution_owner_kind => ExecutionOwnerKind,
        .execution_owner_context => ?*anyopaque,
        .entry => ?Entry,
        .created_tick => u64,
        .ready_since_tick => u64,
        .last_scheduled_tick => u64,
        .last_accounted_tick => u64,
        .last_yield_tick => u64,
        .last_ready_latency_ticks => u64,
        .max_ready_latency_ticks => u64,
        .last_wait_ticks => u64,
        .max_wait_ticks => u64,
        .run_ticks => u64,
        .switches_in => u64,
        .max_run_without_switch_ticks => u64,
        .preempt_disable_depth => u32,
        .preempt_disable_max_depth => u32,
        .preemption_probe_hits => u64,
        .preemption_deferred_ticks => u64,
        .max_preemption_deferred_ticks => u64,
        .long_run_warnings => u64,
        .starvation_warnings => u64,
        .last_long_run_warning_tick => u64,
        .last_starvation_warning_tick => u64,
        .fpu_state => ?[]u8,
        .fpu_state_bytes => u32,
        .fpu_state_valid => bool,
        .uses_fpu => bool,
    };
}
pub fn get(target: *const Task, comptime field: Field) FieldType(field) {
    const T = FieldType(field);
    if (T == []const u8 or T == ?[]u8) {
        var span: Span = .{};
        const present = r4os_rust_task_span(target, @intFromEnum(field), &span);
        if (T == ?[]u8) {
            if (!present) return null;
            return if (span.len == 0) @constCast("") else @constCast(span.pointer.?[0..span.len]);
        }
        return if (span.len == 0) "" else span.pointer.?[0..span.len];
    }
    if (T == [8]HeldLockRecord) {
        var records: [8]HeldLockRecord = undefined;
        for (&records, 0..) |*record, index| record.* = heldLock(target, index);
        return records;
    }
    if (T == [4]u16) {
        var counts: [4]u16 = undefined;
        r4os_rust_task_donation_counts(target, &counts);
        return counts;
    }
    if (T == u128) {
        var pair: Pair = undefined;
        r4os_rust_task_sequence(target, field == .timeout_sequence, &pair);
        return (@as(u128, pair.high) << 64) | pair.low;
    }
    const value = r4os_rust_task_field(target, @intFromEnum(field));
    if (T == bool) return value != 0;
    switch (@typeInfo(T)) {
        .int => return @intCast(value),
        .@"enum" => return @enumFromInt(value),
        .pointer => return @ptrFromInt(value),
        .optional => |optional| {
            if (value == 0) return null;
            switch (@typeInfo(optional.child)) {
                .pointer => return @ptrFromInt(value),
                else => @compileError("Unsupported private task optional projection"),
            }
        },
        else => @compileError("Unsupported private task projection"),
    }
}
pub fn set(target: *Task, comptime field: Field, value: FieldType(field)) void {
    const T = FieldType(field);
    if (T == []const u8) {
        r4os_rust_task_set_span(target, @intFromEnum(field), .{ .pointer = value.ptr, .len = value.len }, true);
        return;
    }
    if (T == ?[]u8) {
        const span: Span = if (value) |bytes| .{ .pointer = bytes.ptr, .len = bytes.len } else .{};
        r4os_rust_task_set_span(target, @intFromEnum(field), span, value != null);
        return;
    }
    const raw: u64 = switch (@typeInfo(T)) {
        .int => @intCast(value),
        .bool => @intFromBool(value),
        .@"enum" => @intFromEnum(value),
        .pointer => @intFromPtr(value),
        .optional => if (value) |pointer| @intFromPtr(pointer) else 0,
        else => @compileError("Task field needs owner operation"),
    };
    r4os_rust_task_set_field(target, @intFromEnum(field), raw);
}

pub fn summary() Summary {
    var out: Summary = undefined;
    _ = r4os_rust_task_snapshot(0, 0, &out, 0);
    return out;
}
pub fn count() usize {
    return @intCast(r4os_rust_task_control(1, 0, 0));
}
pub fn inventoryEpoch() u64 {
    return r4os_rust_task_control(2, 0, 0);
}
pub fn inventoryPeak() u32 {
    return @intCast(r4os_rust_task_control(3, 0, 0));
}
pub fn inventoryPage(after: u64, out: []InventorySnapshot) InventoryPage {
    var page: InventoryPage = undefined;
    r4os_rust_task_inventory(after, out.ptr, out.len, &page);
    return page;
}
pub fn pinByOrdinal(index: usize) ?*Task {
    return ptr(r4os_rust_task_snapshot(4, index, null, 0));
}
pub fn ordinalOf(target: *const Task) ?usize {
    const result = r4os_rust_task_snapshot(5, @intFromPtr(target), null, 0);
    return if (result == 0xFFFF_FFFF_FFFF_FFFF) null else @intCast(result);
}
pub fn first() ?*Task {
    return ptr(r4os_rust_task_control(21, 0, 0));
}
pub fn next(target: *const Task) ?*Task {
    return get(target, .registry_next);
}
pub fn nextCircular(target: *const Task) ?*Task {
    return next(target) orelse first();
}
pub fn firstReady(cpu: u32) ?*Task {
    return ptr(r4os_rust_task_control(22, cpu, 0));
}
pub fn nextReady(target: *const Task) ?*Task {
    return get(target, .ready_next);
}
pub fn firstReadyAtRank(cpu: u32, rank: usize) ?*Task {
    return ptr(r4os_rust_task_control(24, cpu, rank));
}
pub fn nextReadyWarning(cpu: u32) ?*Task {
    return ptr(r4os_rust_task_control(25, cpu, 0));
}
pub fn readyCount() usize {
    return @intCast(r4os_rust_task_control(26, 0, 0));
}
pub fn readyCountForCpu(cpu: u32) usize {
    return @intCast(r4os_rust_task_control(23, cpu, 0));
}
pub fn firstTimedWait() ?*Task {
    return ptr(r4os_rust_task_control(27, 0, 0));
}
pub fn nextTimedWait(target: *const Task) ?*Task {
    return get(target, .timeout_next);
}
pub fn timedWaitCount() usize {
    return @intCast(r4os_rust_task_control(28, 0, 0));
}
pub fn deferredReapCount() usize {
    return @intCast(r4os_rust_task_control(29, 0, 0));
}
pub fn minWakeTick() u64 {
    return r4os_rust_task_control(15, 0, 0);
}
pub const NO_WAKE_TICK: u64 = timer.NO_DEADLINE;
pub fn hasReadyMoreUrgentThanRank(cpu: u32, rank: u8) bool {
    return r4os_rust_task_control(31, cpu, rank) != 0;
}
pub fn hasMoreUrgentReady(cpu: u32, target: *const Task) bool {
    return hasReadyMoreUrgentThanRank(cpu, dispatchRank(target));
}
pub fn dispatchRank(target: *const Task) u8 {
    return r4os_rust_task_rank(target, false);
}
pub fn wakeDispatchRank(target: *const Task) u8 {
    return r4os_rust_task_rank(target, true);
}
pub fn effectivePriority(target: *const Task) Priority {
    return switch (dispatchRank(target)) {
        0, 1 => .high,
        2 => .normal,
        else => .low,
    };
}
pub fn assignRole(target: *Task, role: Role) bool {
    return mutate(16, target, @intFromEnum(role), 0) != 0;
}
pub fn markReady(target: *Task, now: u64) void {
    _ = mutate(0, target, now, 0);
}
pub fn markRunning(target: *Task) void {
    _ = mutate(1, target, 0, 0);
}
pub fn parkCpuIdle(target: *Task) void {
    _ = mutate(2, target, 0, 0);
}
pub fn noteSwitchedOut(target: *Task) void {
    _ = mutate(3, target, 0, 0);
}
pub fn recordScheduled(target: *Task, now: u64) u64 {
    return mutate(4, target, now, 0);
}
pub fn recordYield(target: *Task, now: u64) void {
    _ = mutate(5, target, now, 0);
}
pub fn recordRunTick(target: *Task, now: u64) u64 {
    return mutate(6, target, now, 0);
}
pub fn recordPreemptDisable(target: *Task) u32 {
    return @intCast(mutate(7, target, 0, 0));
}
pub fn recordPreemptEnable(target: *Task) bool {
    return mutate(8, target, 0, 0) != 0;
}
pub fn recordPreemptionProbe(target: *Task) void {
    _ = mutate(9, target, 0, 0);
}
pub fn recordPreemptionDeferred(target: *Task, ticks: u64) void {
    _ = mutate(10, target, ticks, 0);
}
pub fn recordLongRunWarning(target: *Task, now: u64) void {
    _ = mutate(11, target, now, 0);
}
pub fn recordStarvationWarning(target: *Task, now: u64) void {
    _ = mutate(12, target, now, 0);
}
pub fn beginWait(target: *Task, wake: u64, reason: []const u8, object: u64) void {
    _ = r4os_rust_task_mutate(13, target, wake, object, reason.ptr, reason.len);
}
pub fn finishWait(target: *Task, result: WaitResult) u64 {
    return mutate(14, target, @intFromEnum(result), 0);
}
pub fn finishCurrentForExit(target: *Task, retire: bool) bool {
    return mutate(15, target, @intFromBool(retire), 0) != 0;
}
pub fn saveFpuState(target: *Task) void {
    _ = mutate(19, target, 0, 0);
}
pub fn restoreFpuState(target: *Task) void {
    _ = mutate(20, target, 0, 0);
}
pub fn validFpuStateCount() u32 {
    return @intCast(r4os_rust_task_control(33, 0, 0));
}
pub fn fpuLazySaves() u64 {
    return r4os_rust_task_control(10, 0, 0);
}
pub fn fpuLazySkips() u64 {
    return r4os_rust_task_control(11, 0, 0);
}
pub fn pin(target: *Task) bool {
    return mutate(17, target, 0, 0) != 0;
}
pub fn unpin(target: *Task) bool {
    return mutate(18, target, 0, 0) != 0;
}
pub fn pinByIdentity(id: u32, generation: u64) ?*Task {
    return ptr(r4os_rust_task_identity(0, id, generation, null, 0));
}
pub fn pinById(id: u32) ?*Task {
    return ptr(r4os_rust_task_identity(1, id, 0, null, 0));
}
pub fn kill(id: u32) bool {
    return r4os_rust_task_identity(2, id, 0, null, 0) != 0;
}
pub fn killIdentity(id: u32, generation: u64) bool {
    return r4os_rust_task_identity(3, id, generation, null, 0) != 0;
}
pub fn releaseDead(id: u32) bool {
    return r4os_rust_task_identity(4, id, 0, null, 0) != 0;
}
pub fn releaseDeadIdentity(id: u32, generation: u64) bool {
    return r4os_rust_task_identity(5, id, generation, null, 0) != 0;
}
pub fn retireIdentity(id: u32, generation: u64) RetireResult {
    return @enumFromInt(r4os_rust_task_identity(6, id, generation, null, 0));
}
pub fn detachWait(target: *Task) bool {
    return mutate(21, target, 0, 0) != 0;
}
pub fn isAlive(id: u32) bool {
    return r4os_rust_task_control(18, id, 0) != 0;
}
pub fn isAliveIdentity(id: u32, generation: u64) bool {
    return r4os_rust_task_control(19, id, generation) != 0;
}
pub fn existsIdentity(id: u32, generation: u64) bool {
    return r4os_rust_task_control(20, id, generation) != 0;
}
pub fn reapDeferred() u32 {
    return @intCast(r4os_rust_task_control(16, 0, 0));
}
pub fn addDispatchDonationByIdentity(id: u32, generation: u64, rank: u8) bool {
    return r4os_rust_task_identity(7, id, generation, null, rank) != 0;
}
pub fn removeDispatchDonation(target: *Task, rank: u8) bool {
    return mutate(22, target, rank, 0) != 0;
}
pub fn bindExecutionOwner(target: *Task, kind: ExecutionOwnerKind, context: *anyopaque) bool {
    return mutate(23, target, @intFromEnum(kind), @intFromPtr(context)) != 0;
}
pub fn bindBlockedHomeCpu(target: *Task, cpu: u32) bool {
    return mutate(24, target, cpu, 0) != 0;
}
pub fn clearExecutionOwner(target: *Task, context: *const anyopaque) bool {
    return mutate(25, target, @intFromPtr(context), 0) != 0;
}
pub fn executionOwner(target: *const Task) ExecutionOwner {
    return .{ .kind = get(target, .execution_owner_kind), .context = get(target, .execution_owner_context) };
}
pub fn criticalReserveStats() struct { total: u32, available: u32, in_use: u32 } {
    const used: u32 = @intCast(r4os_rust_task_control(4, 0, 0));
    return .{ .total = 4, .available = 4 - used, .in_use = used };
}
pub fn stackCacheStats() struct { cached: u64, hits: u64, misses: u64, reclaims: u64, release_failures: u64 } {
    return .{ .cached = r4os_rust_task_control(5, 0, 0), .hits = r4os_rust_task_control(6, 0, 0), .misses = r4os_rust_task_control(7, 0, 0), .reclaims = r4os_rust_task_control(8, 0, 0), .release_failures = r4os_rust_task_control(9, 0, 0) };
}
pub fn reclaimStackCache(requested: u32) u32 {
    return @intCast(r4os_rust_task_control(17, requested, 0));
}
pub fn createFailureStats() CreateFailureStats {
    var out: CreateFailureStats = undefined;
    _ = r4os_rust_task_snapshot(1, 0, &out, 0);
    return out;
}
pub fn armNextCreateFailureForTest(value: CreateFailure) bool {
    return r4os_rust_task_control(30, @intFromEnum(value), 0) != 0;
}
pub fn killHeldLockDeferrals() u64 {
    return r4os_rust_task_control(12, 0, 0);
}
pub fn recycleGuardHits() u64 {
    return r4os_rust_task_control(13, 0, 0);
}
pub fn unpublishedRollbackPending() u32 {
    return @intCast(r4os_rust_task_control(14, 0, 0));
}
pub fn stackTelemetryForRole(role: Role) StackTelemetryStats {
    var out: StackTelemetryStats = undefined;
    _ = r4os_rust_task_snapshot(2, @intFromEnum(role), &out, 0);
    return out;
}
pub fn stackTelemetryStats() StackTelemetryStats {
    var out: StackTelemetryStats = .{};
    for ([_]Role{ .input, .short_completion, .interactive, .batch }) |role| {
        const st = stackTelemetryForRole(role);
        out.created +%= st.created;
        out.released +%= st.released;
        out.high_water_total_bytes +%= st.high_water_total_bytes;
        out.high_water_max_bytes = @max(out.high_water_max_bytes, st.high_water_max_bytes);
        out.create_cycles_total +%= st.create_cycles_total;
        out.create_cycles_max = @max(out.create_cycles_max, st.create_cycles_max);
        out.release_cycles_total +%= st.release_cycles_total;
        out.release_cycles_max = @max(out.release_cycles_max, st.release_cycles_max);
    }
    return out;
}
pub const QueueSnapshot = extern struct { registry: usize, ready: usize, timed: usize, reap: usize, valid: bool };
pub fn queueSnapshot() QueueSnapshot {
    var out: QueueSnapshot = undefined;
    _ = r4os_rust_task_snapshot(3, 0, &out, 0);
    return out;
}
pub const ReapStats = extern struct { passes: u64, candidate_visits: u64, pending: usize };
extern fn r4os_rust_task_reap_stats(output: *ReapStats) void;
pub fn reapStats() ReapStats {
    var out: ReapStats = undefined;
    r4os_rust_task_reap_stats(&out);
    return out;
}
pub const RoleStats = extern struct { activations: [4]u64, budget_exhaustions: [4]u64, donation_requests: u64, donations_applied: u64, donations_released: u64, donation_budget_exhaustions: u64 };
extern fn r4os_rust_task_roles(output: *RoleStats) void;
pub fn roleStats() RoleStats {
    var out: RoleStats = undefined;
    r4os_rust_task_roles(&out);
    return out;
}
pub const GuardHit = struct { name: []const u8, id: u32, stack_base: u64 };
const RawGuardHit = extern struct { name: Span, id: u32, stack_base: u64 };
extern fn r4os_rust_task_guard_hit(address: u64, output: *RawGuardHit) bool;
pub fn stackGuardHit(address: u64) ?GuardHit {
    var out: RawGuardHit = undefined;
    if (!r4os_rust_task_guard_hit(address, &out)) return null;
    return .{ .name = if (out.name.len == 0) "" else out.name.pointer.?[0..out.name.len], .id = out.id, .stack_base = out.stack_base };
}
pub fn captureDeadmanSnapshot(now: u64, threshold: u64) DeadmanSnapshot {
    var out: DeadmanSnapshot = undefined;
    r4os_rust_task_deadman(now, threshold, &out);
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

pub fn dump() void {
    const now = timer.tickCount();
    var ready: u32 = 0;
    var running: u32 = 0;
    var blocked: u32 = 0;
    var dead: u32 = 0;
    var workers: u32 = 0;
    var count_cursor = first();
    while (count_cursor) |t| : (count_cursor = get(t, .registry_next)) {
        if (get(t, .entry) != null and get(t, .state) != .unused and get(t, .state) != .dead) workers += 1;
        switch (get(t, .state)) {
            .ready => ready += 1,
            .running => running += 1,
            .blocked => blocked += 1,
            .dead => dead += 1,
            .unused => {},
        }
    }

    k.puts("  Tasks: ");
    k.putDec(count());
    k.puts(" registry=dynamic");
    k.puts(" stack_guard=");
    k.putDec(stack_guard_size);
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
    var dump_cursor = first();
    while (dump_cursor) |task| : (dump_cursor = get(task, .registry_next)) {
        k.puts("    #");
        k.putDec(get(task, .id));
        k.puts(" ");
        k.puts(get(task, .name));
        k.puts(" ");
        k.puts(stateName(get(task, .state)));
        k.puts(" role=");
        k.puts(roleName(get(task, .role)));
        k.puts(" prio=");
        k.puts(switch (effectivePriority(task)) {
            .high => "high",
            .normal => "normal",
            .low => "low",
        });
        if (rolePolicy(get(task, .role)).priority == .high) {
            k.puts(" budget=");
            k.putDec(get(task, .role_budget_ticks_remaining));
            k.puts("/");
            k.putDec(get(task, .role_budget_dispatches_remaining));
        }
        if (get(task, .entry) != null) {
            k.puts(" worker=yes");
        }
        if (get(task, .preempt_disable_depth) != 0 or get(task, .preempt_disable_max_depth) != 0) {
            k.puts(" preempt=");
            k.putDec(get(task, .preempt_disable_depth));
            k.puts("/");
            k.putDec(get(task, .preempt_disable_max_depth));
        }
        if (get(task, .long_run_warnings) != 0 or get(task, .starvation_warnings) != 0) {
            k.puts(" runtime_warn=");
            k.putDec(get(task, .long_run_warnings));
            k.puts("/");
            k.putDec(get(task, .starvation_warnings));
        }
        if (get(task, .state) == .blocked) {
            k.puts(" wait=");
            if (get(task, .wait_reason).len > 0) {
                k.puts(get(task, .wait_reason));
            } else {
                k.puts("unknown");
            }
            k.puts(" wake=");
            k.putDec(get(task, .wake_tick));
            k.puts(" blocked_for=");
            if (now >= get(task, .blocked_since_tick)) {
                k.putDec(now - get(task, .blocked_since_tick));
            } else {
                k.putDec(0);
            }
        }
        k.puts(" rsp=0x");
        k.putHex(get(task, .rsp), 16);
        k.puts(" stack=0x");
        k.putHex(get(task, .stack_base), 16);
        k.puts("..0x");
        k.putHex(get(task, .stack_top), 16);
        if (get(task, .fpu_state_valid)) {
            k.puts(" fpu=");
            k.putDec(get(task, .fpu_state_bytes));
        }
        k.puts("\r\n");
    }
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

pub fn noteCreateFailure(value: CreateFailure) void {
    _ = r4os_rust_task_control(34, @intFromEnum(value), 0);
}

extern fn r4os_rust_task_slot(target: *const Task, unwind: bool) *anyopaque;
pub fn unwindCounterSlot(target: *const Task) *u32 {
    return @ptrCast(@alignCast(r4os_rust_task_slot(target, true)));
}
pub fn rspSlot(target: *const Task) *u64 {
    return @ptrCast(@alignCast(r4os_rust_task_slot(target, false)));
}

pub fn rawState(target: *const Task) u8 {
    return @intCast(r4os_rust_task_field(target, @intFromEnum(Field.state)));
}
