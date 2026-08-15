const STACK_SIZE: usize = 16 * 1024;

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

var tss: Tss align(16) = .{};
var double_fault_stack: [STACK_SIZE]u8 align(16) = undefined;
var fault_stack: [STACK_SIZE]u8 align(16) = undefined;

pub fn init() void {
    tss.rsp0 = stackTop(&fault_stack);
    tss.ist1 = stackTop(&double_fault_stack);
    tss.ist2 = stackTop(&fault_stack);
}

pub fn base() u64 {
    return @intFromPtr(&tss);
}

pub fn limit() u32 {
    return @sizeOf(Tss) - 1;
}

fn stackTop(stack: *[STACK_SIZE]u8) u64 {
    return @intFromPtr(stack) + stack.len;
}
