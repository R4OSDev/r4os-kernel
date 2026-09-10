const api = @import("kernel_api");

// Provider callbacks may be omitted (null). Unknown names must still fail
// type checking; no callback address needs comptime integer evaluation.
comptime {
    const provider: api.R4SysProvider = .{ .unknown_provider_callback = null };
    _ = provider;
}
