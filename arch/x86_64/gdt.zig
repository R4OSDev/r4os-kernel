const k = @import("../../kernel/log.zig");
const percpu = @import("percpu.zig");
const tss = @import("tss.zig");

const CODE_SELECTOR: u16 = 0x08;
const DATA_SELECTOR: u16 = 0x10;
const TSS_SELECTOR: u16 = 0x18;

const DescriptorTablePointer = packed struct {
    limit: u16,
    base: u64,
};

extern fn r4os_load_gdt(gdtr: *const DescriptorTablePointer) callconv(.c) void;
extern fn r4os_load_tss(selector: u16) callconv(.c) void;

const initial_gdt = [5]u64{
    0x0000000000000000,
    0x00AF9A000000FFFF,
    0x00CF92000000FFFF,
    0x0000000000000000,
    0x0000000000000000,
};

var cpu_gdts: [percpu.max_cpus][5]u64 align(8) = .{initial_gdt} ** percpu.max_cpus;

pub fn init() void {
    initCurrent(0);
    k.puts("  GDT loaded ");
    k.puts("[OK]\r\n");
    k.puts("  TSS loaded ");
    k.puts("[OK]\r\n");
}

pub fn initCurrent(index: u32) void {
    if (index >= percpu.max_cpus) return;
    const slot: usize = @intCast(index);
    cpu_gdts[slot] = initial_gdt;
    tss.init(index);
    setTssDescriptor(&cpu_gdts[slot], tss.base(index), tss.limit());

    const gdtr = DescriptorTablePointer{
        .limit = @sizeOf(@TypeOf(cpu_gdts[slot])) - 1,
        .base = @intFromPtr(&cpu_gdts[slot]),
    };
    r4os_load_gdt(&gdtr);
    r4os_load_tss(TSS_SELECTOR);
}

pub fn codeSelector() u16 {
    return CODE_SELECTOR;
}

pub fn dataSelector() u16 {
    return DATA_SELECTOR;
}

fn setTssDescriptor(table: *[5]u64, base: u64, limit: u32) void {
    var low: u64 = 0;
    low |= @as(u64, limit & 0xFFFF);
    low |= (base & 0xFFFFFF) << 16;
    low |= @as(u64, 0x89) << 40; // present, ring 0, available 64-bit TSS
    low |= @as(u64, (limit >> 16) & 0xF) << 48;
    low |= ((base >> 24) & 0xFF) << 56;

    const high: u64 = base >> 32;

    table[3] = low;
    table[4] = high;
}
