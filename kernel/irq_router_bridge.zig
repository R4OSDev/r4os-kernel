// Original bounded bad-handler presentation only; native owner counts/reports.
const k = @import("log.zig");
export fn r4os_zig_irq_router_bad_handler(where_code: u8, irq: u8, address: u64, owner: u32) callconv(.c) void {
    k.puts("IRQROUTER BAD HANDLER ");
    k.puts(switch (where_code) {
        0 => "register",
        1 => "dispatch",
        else => unreachable,
    });
    k.puts(" irq=");
    k.putDec(irq);
    k.puts(" ptr=0x");
    k.putHex(address, 16);
    k.puts(" owner=");
    k.putDec(owner);
    k.puts("\r\n");
}
