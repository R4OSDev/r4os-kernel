const io = @import("../arch/x86_64/io.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const policy = @import("owner_lock_policy.zig");
const layout = @import("layout.zig");
const config = @import("config");

const RFLAGS_IF: u64 = 1 << 9;

pub const Class = enum(u8) {
    program_state,
    storage,
    network,
    driver_work,
    heap,
    virtual_memory,
    page_tables,
    physical_memory,
    boot_log,
    serial_output,
};

pub const class_count: u32 = 10;

pub const Token = extern struct {
    flags: u64 = 0,
    cpu_index: u32 = 0,
    previous_rank: u8 = 0,
    outermost: bool = false,
};

pub const Stats = extern struct {
    acquisitions: u64 = 0,
    nested_acquisitions: u64 = 0,
    collisions: u64 = 0,
    wait_spins: u64 = 0,
    max_wait_spins: u64 = 0,
    hold_cycles: u64 = 0,
    max_hold_cycles: u64 = 0,
    order_violations: u64 = 0,
};

pub const Lock = extern struct {
    class: Class,
    rank: u8,
    state: u8 = 0,
    owner_cpu_plus_one: u8 = 0,
    depth: [percpu.max_cpus]u16 = .{0} ** percpu.max_cpus,
    acquired_tsc: [percpu.max_cpus]u64 = .{0} ** percpu.max_cpus,
    acquisitions: u64 = 0,
    nested_acquisitions: u64 = 0,
    collisions: u64 = 0,
    wait_spins: u64 = 0,
    max_wait_spins: u64 = 0,
    hold_cycles: u64 = 0,
    max_hold_cycles: u64 = 0,
    order_violations: u64 = 0,

    pub fn acquire(self: *Lock) Token {
        var result: Token = undefined;
        r4os_rust_owner_acquire(self, &result);
        return result;
    }
    pub fn release(self: *Lock, token: Token) void {
        r4os_rust_owner_release(self, &token);
    }
    pub fn heldByCurrent(self: *Lock) bool {
        return r4os_rust_owner_held(self);
    }
    pub fn stats(self: *const Lock) Stats {
        var result: Stats = undefined;
        r4os_rust_owner_stats(self, &result);
        return result;
    }
};

// Rust owns every mutation, including independently declared Lock instances.
// The remaining Zig owners retain their resident storage/identity until their
// own migrations; no native struct or mutable reference crosses this C layout.
extern fn r4os_rust_owner_acquire(*const Lock, *Token) callconv(.c) void;
extern fn r4os_rust_owner_release(*const Lock, *const Token) callconv(.c) void;
extern fn r4os_rust_owner_held(*const Lock) callconv(.c) bool;
extern fn r4os_rust_owner_stats(*const Lock, *Stats) callconv(.c) void;
extern fn r4os_rust_owner_fault_allowed() callconv(.c) bool;
extern fn r4os_rust_owner_prepare_stack() callconv(.c) void;
comptime {
    if (percpu.max_cpus != 32 or @sizeOf(Token) != 16 or @offsetOf(Token, "cpu_index") != 8 or
        @sizeOf(Lock) != 392 or @offsetOf(Lock, "depth") != 4 or
        @offsetOf(Lock, "acquired_tsc") != 72 or @offsetOf(Lock, "acquisitions") != 328 or
        @sizeOf(Stats) != 64) @compileError("Rust memory-owner C layout differs");
    if (layout.APP_STACK_BASE != 0xffff_ff40_0000_0000 or layout.APP_STACK_BYTES != 8 * layout.GiB)
        @compileError("Rust stack preflight window differs");
}
export fn r4os_zig_owner_cpu() callconv(.c) u32 {
    return percpu.currentIndex();
}
export fn r4os_zig_owner_runtime_depth() callconv(.c) u32 {
    return percpu.runtimeCriticalDepth().*;
}
export fn r4os_zig_named_owner(class: Class) callconv(.c) *const Lock {
    return forClass(class);
}
pub const STACK_HEADROOM: u64 = 64 * layout.KiB;
pub fn faultResolutionAllowed() bool {
    return r4os_rust_owner_fault_allowed();
}
pub fn prepareCurrentStack() void {
    r4os_rust_owner_prepare_stack();
}

// The small ranks are leaf-domain entry points.  They may call into the
// heap/VM stack, while that stack never calls back into the originating
// registry or I/O owner.  Logging is terminal and therefore comes last.
pub var program_state = Lock{ .class = .program_state, .rank = policy.Rank.program_state };
pub var storage = Lock{ .class = .storage, .rank = policy.Rank.io_owner };
pub var network = Lock{ .class = .network, .rank = policy.Rank.io_owner };
pub var driver_work = Lock{ .class = .driver_work, .rank = policy.Rank.io_owner };
pub var heap = Lock{ .class = .heap, .rank = policy.Rank.heap };
pub var virtual_memory = Lock{ .class = .virtual_memory, .rank = policy.Rank.virtual_memory };
pub var page_tables = Lock{ .class = .page_tables, .rank = policy.Rank.page_tables };
pub var physical_memory = Lock{ .class = .physical_memory, .rank = policy.Rank.physical_memory };
pub var boot_log = Lock{ .class = .boot_log, .rank = policy.Rank.boot_log };
pub var serial_output = Lock{ .class = .serial_output, .rank = policy.Rank.serial_output };

pub fn forClass(class: Class) *const Lock {
    return switch (class) {
        .program_state => &program_state,
        .storage => &storage,
        .network => &network,
        .driver_work => &driver_work,
        .heap => &heap,
        .virtual_memory => &virtual_memory,
        .page_tables => &page_tables,
        .physical_memory => &physical_memory,
        .boot_log => &boot_log,
        .serial_output => &serial_output,
    };
}

pub fn combinedStats() Stats {
    var result: Stats = .{};
    addStats(&result, program_state.stats());
    addStats(&result, storage.stats());
    addStats(&result, network.stats());
    addStats(&result, driver_work.stats());
    addStats(&result, heap.stats());
    addStats(&result, virtual_memory.stats());
    addStats(&result, page_tables.stats());
    addStats(&result, physical_memory.stats());
    addStats(&result, boot_log.stats());
    addStats(&result, serial_output.stats());
    return result;
}

fn addStats(target: *Stats, source: Stats) void {
    target.acquisitions +%= source.acquisitions;
    target.nested_acquisitions +%= source.nested_acquisitions;
    target.collisions +%= source.collisions;
    target.wait_spins +%= source.wait_spins;
    target.max_wait_spins = @max(target.max_wait_spins, source.max_wait_spins);
    target.hold_cycles +%= source.hold_cycles;
    target.max_hold_cycles = @max(target.max_hold_cycles, source.max_hold_cycles);
    target.order_violations +%= source.order_violations;
}
