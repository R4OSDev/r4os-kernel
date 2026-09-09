const io = @import("../arch/x86_64/io.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const policy = @import("owner_lock_policy.zig");
const layout = @import("layout.zig");

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

pub const Token = struct {
    flags: u64 = 0,
    cpu_index: u32 = 0,
    previous_rank: u8 = 0,
    outermost: bool = false,
};

pub const Stats = struct {
    acquisitions: u64 = 0,
    nested_acquisitions: u64 = 0,
    collisions: u64 = 0,
    wait_spins: u64 = 0,
    max_wait_spins: u64 = 0,
    hold_cycles: u64 = 0,
    max_hold_cycles: u64 = 0,
    order_violations: u64 = 0,
};

pub const Lock = struct {
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
        const flags = io.readRflags();
        io.cli();
        prepareCurrentStack();
        const cpu_index = percpu.currentIndex();
        const slot: usize = @intCast(cpu_index);
        const owner: u8 = @intCast(cpu_index + 1);
        const previous_rank = held_rank[slot];

        if (@atomicLoad(u8, &self.owner_cpu_plus_one, .acquire) == owner and self.depth[slot] != 0) {
            self.depth[slot] +|= 1;
            _ = @atomicRmw(u64, &self.nested_acquisitions, .Add, 1, .monotonic);
            _ = @atomicRmw(u64, &self.acquisitions, .Add, 1, .monotonic);
            return .{
                .flags = flags,
                .cpu_index = cpu_index,
                .previous_rank = previous_rank,
                .outermost = false,
            };
        }

        if (!policy.orderAllowed(previous_rank, self.rank)) {
            _ = @atomicRmw(u64, &self.order_violations, .Add, 1, .monotonic);
        }

        var spins: u64 = 0;
        var collided = false;
        while (@cmpxchgWeak(u8, &self.state, 0, 1, .acquire, .monotonic)) |_| {
            collided = true;
            spins +|= 1;
            // An ordinary task waiting for an owner must remain able to
            // acknowledge the higher-priority cross-CPU TLB IPI.  Callers
            // which deliberately entered with IF=0 (notably IRQ paths) keep
            // that state and never acquire interruptibility implicitly.
            if ((flags & RFLAGS_IF) != 0) {
                io.sti();
                asm volatile ("pause");
                io.cli();
            } else {
                asm volatile ("pause");
            }
        }
        if (collided) _ = @atomicRmw(u64, &self.collisions, .Add, 1, .monotonic);
        _ = @atomicRmw(u64, &self.wait_spins, .Add, spins, .monotonic);
        _ = @atomicRmw(u64, &self.max_wait_spins, .Max, spins, .monotonic);
        _ = @atomicRmw(u64, &self.acquisitions, .Add, 1, .monotonic);

        @atomicStore(u8, &self.owner_cpu_plus_one, owner, .release);
        self.depth[slot] = 1;
        self.acquired_tsc[slot] = readTsc();
        held_rank[slot] = self.rank;
        return .{
            .flags = flags,
            .cpu_index = cpu_index,
            .previous_rank = previous_rank,
            .outermost = true,
        };
    }

    pub fn release(self: *Lock, token: Token) void {
        const cpu_index = percpu.currentIndex();
        const slot: usize = @intCast(cpu_index);
        const owner: u8 = @intCast(cpu_index + 1);
        if (token.cpu_index != cpu_index or
            @atomicLoad(u8, &self.owner_cpu_plus_one, .acquire) != owner or
            self.depth[slot] == 0)
        {
            _ = @atomicRmw(u64, &self.order_violations, .Add, 1, .monotonic);
            restoreLocal(token.flags);
            return;
        }

        self.depth[slot] -= 1;
        if (self.depth[slot] == 0) {
            const cycles = readTsc() -% self.acquired_tsc[slot];
            _ = @atomicRmw(u64, &self.hold_cycles, .Add, cycles, .monotonic);
            _ = @atomicRmw(u64, &self.max_hold_cycles, .Max, cycles, .monotonic);
            held_rank[slot] = token.previous_rank;
            @atomicStore(u8, &self.owner_cpu_plus_one, 0, .release);
            @atomicStore(u8, &self.state, 0, .release);
        }
        restoreLocal(token.flags);
    }

    pub fn heldByCurrent(self: *Lock) bool {
        const cpu_index = percpu.currentIndex();
        const owner: u8 = @intCast(cpu_index + 1);
        return @atomicLoad(u8, &self.owner_cpu_plus_one, .acquire) == owner and
            self.depth[@intCast(cpu_index)] != 0;
    }

    pub fn stats(self: *const Lock) Stats {
        return .{
            .acquisitions = @atomicLoad(u64, &self.acquisitions, .monotonic),
            .nested_acquisitions = @atomicLoad(u64, &self.nested_acquisitions, .monotonic),
            .collisions = @atomicLoad(u64, &self.collisions, .monotonic),
            .wait_spins = @atomicLoad(u64, &self.wait_spins, .monotonic),
            .max_wait_spins = @atomicLoad(u64, &self.max_wait_spins, .monotonic),
            .hold_cycles = @atomicLoad(u64, &self.hold_cycles, .monotonic),
            .max_hold_cycles = @atomicLoad(u64, &self.max_hold_cycles, .monotonic),
            .order_violations = @atomicLoad(u64, &self.order_violations, .monotonic),
        };
    }
};

var held_rank: [percpu.max_cpus]u8 = .{0} ** percpu.max_cpus;

pub const STACK_HEADROOM: u64 = 64 * layout.KiB;

/// Page faults cannot reenter a half-mutated registry, heap, AVL tree or
/// scheduler projection. Called at exception entry, before fault resolution
/// acquires any of those owners itself.
pub fn faultResolutionAllowed() bool {
    return held_rank[percpu.currentIndex()] == 0 and percpu.runtimeCriticalDepth().* == 0;
}

/// R4X calls kernel APIs on its demand-grown execution stack. Prepare the
/// same resident call budget as a kernel Task stack BEFORE taking the first
/// no-sleep/runtime owner. IRQs are already disabled; a synchronous fault
/// uses the separate resident IST stack and may safely finish the growth.
/// Probe each page so a terminal guard cannot be skipped into another stack.
/// Reads preserve stack canaries; nested owners neither probe nor allocate.
pub fn prepareCurrentStack() void {
    if (!faultResolutionAllowed()) return;
    const rsp = asm volatile ("mov %%rsp, %[value]"
        : [value] "=r" (-> u64),
    );
    if (rsp < layout.APP_STACK_BASE or rsp >= layout.APP_STACK_BASE + layout.APP_STACK_BYTES) return;
    var offset: u64 = layout.PAGE_SIZE;
    while (offset <= STACK_HEADROOM) : (offset += layout.PAGE_SIZE) {
        const page: *const volatile u8 = @ptrFromInt(rsp - offset);
        _ = page.*;
    }
    // Fault handlers may have updated stack metadata while a probe ran.
    asm volatile ("" ::: .{ .memory = true });
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

fn restoreLocal(flags: u64) void {
    if ((flags & RFLAGS_IF) != 0) {
        io.sti();
    } else {
        io.cli();
    }
}

fn readTsc() u64 {
    var low: u32 = undefined;
    var high: u32 = undefined;
    asm volatile ("rdtsc"
        : [low] "={eax}" (low),
          [high] "={edx}" (high),
    );
    return (@as(u64, high) << 32) | low;
}
