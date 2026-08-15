const api = @import("kernel_api");

fn write(_: [*]const u8, _: u32) callconv(.c) i32 {
    return 0;
}

comptime {
    _ = api.buildR4SysTable(.{
        .write = &write,
    });
}
