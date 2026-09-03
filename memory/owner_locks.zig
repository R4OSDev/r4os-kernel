const io = @import("../arch/x86_64/io.zig");
const percpu = @import("../arch/x86_64/percpu.zig");

const RFLAGS_IF: u64 = 1 << 9;

pub const Class = enum(u8) {
    virtual_memory,
    page_tables,
    physical_memory,
};

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

        if (previous_rank > self.rank) {
            _ = @atomicRmw(u64, &self.order_violations, .Add, 1, .monotonic);
        }

        var spins: u64 = 0;
        var collided = false;
        while (@cmpxchgWeak(u8, &self.state, 0, 1, .acquire, .monotonic)) |_| {
            collided = true;
            spins +|= 1;
            asm volatile ("pause");
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

pub var virtual_memory = Lock{ .class = .virtual_memory, .rank = 10 };
pub var page_tables = Lock{ .class = .page_tables, .rank = 20 };
pub var physical_memory = Lock{ .class = .physical_memory, .rank = 30 };

pub fn combinedStats() Stats {
    var result: Stats = .{};
    addStats(&result, virtual_memory.stats());
    addStats(&result, page_tables.stats());
    addStats(&result, physical_memory.stats());
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
