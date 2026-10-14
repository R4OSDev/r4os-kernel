// Temporary stateless VM facade cut. Rust owns task construction, retries,
// caches and resource identity. Each call runs at the existing memory owner.
const virt = @import("../memory/virt.zig");
const k = @import("../kernel/log.zig");

export fn r4os_zig_task_vm(operation: u8, id: u32, output: ?*u64) callconv(.c) u32 {
    switch (operation) {
        0 => {
            const value = virt.reserve(.{ .window = .kernel_stack, .len = 80 * 1024, .name = "kernel-task-stack" }) catch |err| {
                k.puts("Task stack reserve failed: ");
                k.puts(@errorName(err));
                k.puts("\r\n");
                return 1;
            };
            output.?.* = value;
        },
        1 => {
            const info = virt.rangeInfo(id) orelse return 1;
            output.?.* = info.base;
        },
        2 => virt.protectGuard(id, 0, 16 * 1024) catch |err| {
            k.puts("Task stack guard failed: ");
            k.puts(@errorName(err));
            k.puts("\r\n");
            return 1;
        },
        3 => virt.commit(id, 16 * 1024, 64 * 1024) catch |err| {
            k.puts("Task stack commit failed: ");
            k.puts(@errorName(err));
            k.puts("\r\n");
            return 1;
        },
        4 => virt.release(id) catch |err| {
            k.puts("Task stack release failed: ");
            k.puts(@errorName(err));
            k.puts("\r\n");
            return 1;
        },
        else => unreachable,
    }
    return 0;
}
