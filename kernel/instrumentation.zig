//! Compile-time facade. Functional clocks/deadlines/owners never use it.
const config=@import("config");
const monotonic=@import("../platform/monotonic.zig");
pub const detailed=config.enable_metrics;
pub const work_critical_stride:u32=64;
const NativeStamp=extern struct{raw:u64,generation:u32,kind:u8};
extern fn r4os_rust_instrumentation_capture(*NativeStamp) void;
pub fn capture() monotonic.Stamp {
    if(comptime !detailed)return .{};
    var out:NativeStamp=undefined;
    r4os_rust_instrumentation_capture(&out);
    return .{.raw=out.raw,.generation=out.generation,.kind=@enumFromInt(out.kind)};
}

comptime {if(@sizeOf(NativeStamp)!=16 or @offsetOf(NativeStamp,"generation")!=8 or @offsetOf(NativeStamp,"kind")!=12)@compileError("Native optional stamp layout");}
