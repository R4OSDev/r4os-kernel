// Thin compatibility facade. Per-CPU GDT/TSS storage belongs to Rust.
const k = @import("../../kernel/log.zig");
const percpu = @import("percpu.zig");
comptime {
    if (percpu.max_cpus != 32) @compileError("Rust GDT CPU capacity differs");
}
extern fn r4os_rust_gdt_init(index: u32) callconv(.c) void;
pub fn init() void {
    initCurrent(0);
    k.puts("  GDT loaded [OK]\r\n");
    k.puts("  TSS loaded [OK]\r\n");
}
pub fn initCurrent(index: u32) void {
    r4os_rust_gdt_init(index);
}
pub fn codeSelector() u16 {
    return 0x08;
}
pub fn dataSelector() u16 {
    return 0x10;
}
