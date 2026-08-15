const k = @import("../../kernel/log.zig");
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

var gdt: [5]u64 align(8) = .{
    0x0000000000000000, // null
    0x00AF9A000000FFFF, // kernel code: base 0, limit ignored in long mode
    0x00CF92000000FFFF, // kernel data
    0x0000000000000000, // TSS low
    0x0000000000000000, // TSS high
};

pub fn init() void {
    tss.init();
    setTssDescriptor(tss.base(), tss.limit());

    const gdtr = DescriptorTablePointer{
        .limit = @sizeOf(@TypeOf(gdt)) - 1,
        .base = @intFromPtr(&gdt),
    };
    r4os_load_gdt(&gdtr);
    r4os_load_tss(TSS_SELECTOR);
    k.puts("  GDT loaded ");
    k.puts("[OK]\r\n");
    k.puts("  TSS loaded ");
    k.puts("[OK]\r\n");
}

pub fn codeSelector() u16 {
    return CODE_SELECTOR;
}

pub fn dataSelector() u16 {
    return DATA_SELECTOR;
}

fn setTssDescriptor(base: u64, limit: u32) void {
    var low: u64 = 0;
    low |= @as(u64, limit & 0xFFFF);
    low |= (base & 0xFFFFFF) << 16;
    low |= @as(u64, 0x89) << 40; // present, ring 0, available 64-bit TSS
    low |= @as(u64, (limit >> 16) & 0xF) << 48;
    low |= ((base >> 24) & 0xFF) << 56;

    const high: u64 = base >> 32;

    gdt[3] = low;
    gdt[4] = high;
}
