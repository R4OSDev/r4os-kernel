const STACK_SIZE: usize = 16 * 1024;
const percpu = @import("percpu.zig");

pub const DOUBLE_FAULT_IST: u8 = 1;
pub const FAULT_IST: u8 = 2;

pub const Tss = packed struct {
    reserved0: u32 = 0,
    rsp0: u64 = 0,
    rsp1: u64 = 0,
    rsp2: u64 = 0,
    reserved1: u64 = 0,
    ist1: u64 = 0,
    ist2: u64 = 0,
    ist3: u64 = 0,
    ist4: u64 = 0,
    ist5: u64 = 0,
    ist6: u64 = 0,
    ist7: u64 = 0,
    reserved2: u64 = 0,
    reserved3: u16 = 0,
    iomap_base: u16 = @sizeOf(Tss),
};

var cpu_tss: [percpu.max_cpus]Tss align(16) = .{Tss{}} ** percpu.max_cpus;
var double_fault_stacks: [percpu.max_cpus][STACK_SIZE]u8 align(16) = undefined;
var fault_stacks: [percpu.max_cpus][STACK_SIZE]u8 align(16) = undefined;

pub fn init(index: u32) void {
    if (index >= percpu.max_cpus) return;
    const slot: usize = @intCast(index);
    cpu_tss[slot] = .{};
    cpu_tss[slot].rsp0 = stackTop(&fault_stacks[slot]);
    cpu_tss[slot].ist1 = stackTop(&double_fault_stacks[slot]);
    cpu_tss[slot].ist2 = stackTop(&fault_stacks[slot]);
}

pub fn base(index: u32) u64 {
    if (index >= percpu.max_cpus) return 0;
    return @intFromPtr(&cpu_tss[index]);
}

pub fn limit() u32 {
    return @sizeOf(Tss) - 1;
}

fn stackTop(stack: *[STACK_SIZE]u8) u64 {
    return @intFromPtr(stack) + stack.len;
}
