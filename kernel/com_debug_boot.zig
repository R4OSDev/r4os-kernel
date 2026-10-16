// Native COM1 configuration and output routing share one typed owner.
extern fn r4os_rust_com_debug_boot() callconv(.c) void;
pub fn init() void {r4os_rust_com_debug_boot();}
