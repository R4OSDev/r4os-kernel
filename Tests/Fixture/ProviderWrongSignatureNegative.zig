const api = @import("kernel_api");

fn wrongWrite(_: [*]const u8) callconv(.c) i32 {
    return 0;
}

comptime {
    var provider: api.R4SysProvider = undefined;
    provider.write = &wrongWrite;
}
