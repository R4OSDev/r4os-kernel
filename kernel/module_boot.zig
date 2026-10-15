const log = @import("log.zig");
const modules = @import("modules.zig");

// Stateless private C facade; Rust owns the one bootstrap-ready flag.
const NativePeers = extern struct {
    registry: *const fn () callconv(.c) void,
    log: *const fn ([*]const u8, usize) callconv(.c) void,
};
extern fn r4os_rust_module_boot_init(*const NativePeers) void;
extern fn r4os_rust_module_boot_ready() bool;

fn registry() callconv(.c) void {
    modules.init();
}
fn puts(data: [*]const u8, len: usize) callconv(.c) void {
    log.puts(data[0..len]);
}
const peers = NativePeers{ .registry = registry, .log = puts };

pub fn init() void {
    r4os_rust_module_boot_init(&peers);
}

pub fn isInitialized() bool {
    return r4os_rust_module_boot_ready();
}

comptime {
    if (@sizeOf(NativePeers) != 16 or @offsetOf(NativePeers, "registry") != 0 or @offsetOf(NativePeers, "log") != 8) @compileError("module boot private C layout drift");
}
