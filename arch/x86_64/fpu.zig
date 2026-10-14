// Private C facade: Rust owns CPUID/XCR0 selection and every FPU state/count.
const bootlog = @import("../../kernel/bootlog.zig");
pub const state_storage_bytes: usize = 4096;
pub const state_storage_align: usize = 64;
pub const backend_none: u32 = 0;
pub const backend_fxsave: u32 = 1;
pub const backend_xsave: u32 = 2;
pub const simd_abi_none: u32 = 0;
pub const simd_abi_sse2: u32 = 1;
pub const simd_abi_avx: u32 = 2;
pub const simd_abi_avx2: u32 = 3;
pub const Status = extern struct {
    initialized: bool = false,
    enabled: bool = false,
    backend: u32 = backend_none,
    state_bytes: u32 = 0,
    state_storage_bytes: u32 = state_storage_bytes,
    xcr0_mask: u64 = 0,
    avx_supported: bool = false,
    avx_enabled: bool = false,
    avx2_supported: bool = false,
    avx2_enabled: bool = false,
    simd_abi: u32 = simd_abi_none,
    cr0: u64 = 0,
    cr4: u64 = 0,
    save_count: u64 = 0,
    restore_count: u64 = 0,
    task_init_count: u64 = 0,
    task_state_bytes: u64 = 0,
};

extern fn r4os_rust_fpu_control(operation: u8) u64;
extern fn r4os_rust_fpu_buffer(operation: u8, buffer: [*]u8, len: usize) bool;
extern fn r4os_rust_fpu_status(output: *Status) void;
comptime {
    if (@sizeOf(Status) != 80 or @offsetOf(Status, "xcr0_mask") != 16 or
        @offsetOf(Status, "cr0") != 32 or @offsetOf(Status, "task_state_bytes") != 72)
        @compileError("Native FPU C layout differs");
}
pub fn init() bool {
    return r4os_rust_fpu_control(0) != 0;
}
pub fn initCurrentCpu() bool {
    return r4os_rust_fpu_control(1) != 0;
}
pub fn restoreInitialState() bool {
    return r4os_rust_fpu_control(2) != 0;
}
pub fn activeStateBytes() usize {
    return @intCast(r4os_rust_fpu_control(3));
}
pub fn initTaskState(state: []u8) bool {
    return r4os_rust_fpu_buffer(0, state.ptr, state.len);
}
pub fn saveTaskState(state: []u8) void {
    _ = r4os_rust_fpu_buffer(1, state.ptr, state.len);
}
pub fn restoreTaskState(state: []const u8) void {
    _ = r4os_rust_fpu_buffer(2, @constCast(state.ptr), state.len);
}
pub fn status() Status {
    var value: Status = undefined;
    r4os_rust_fpu_status(&value);
    return value;
}
export fn r4os_zig_fpu_log(event: u8, st: *const Status, inherited: [*]const u64) callconv(.c) void {
    if (event == 2) {
        bootlog.puts("[FPU] initial SIMD normalized inherited-xmm0=");
        bootlog.puts(if (inherited[0] != 0 or inherited[1] != 0) "nonzero" else "zero");
        bootlog.puts(" inherited-mxcsr=0x");
        bootlog.putHex(inherited[2], 8);
        bootlog.puts(" initial-mxcsr=0x00001F80\r\n");
    }
    logStatus(st.*, switch (event) {
        0 => "missing fxsr/sse/sse2",
        1 => "invalid xsave state size",
        2 => "ready",
        else => unreachable,
    });
}
pub fn backendName(code: u32) []const u8 {
    return switch (code) {
        backend_fxsave => "fxsave",
        backend_xsave => "xsave",
        else => "none",
    };
}
fn logStatus(st: Status, note: []const u8) void {
    bootlog.puts("[FPU] ");
    bootlog.puts(note);
    bootlog.puts(" backend=");
    bootlog.puts(backendName(st.backend));
    bootlog.puts(" state=");
    bootlog.putDec(st.state_bytes);
    bootlog.puts("/");
    bootlog.putDec(st.state_storage_bytes);
    bootlog.puts(" xcr0=0x");
    bootlog.putHex(st.xcr0_mask, 16);
    bootlog.puts(" abi=");
    bootlog.puts(simdAbiName(st.simd_abi));
    bootlog.puts(" avx=");
    bootlog.puts(if (st.avx_enabled) "on" else if (st.avx_supported) "off" else "no");
    bootlog.puts(" avx2=");
    bootlog.puts(if (st.avx2_enabled) "on" else if (st.avx2_supported) "off" else "no");
    bootlog.puts(" cr0=0x");
    bootlog.putHex(st.cr0, 16);
    bootlog.puts(" cr4=0x");
    bootlog.putHex(st.cr4, 16);
    bootlog.puts("\r\n");
}

pub fn simdAbiName(code: u32) []const u8 {
    return switch (code) {
        simd_abi_sse2 => "sse2",
        simd_abi_avx => "avx",
        simd_abi_avx2 => "avx2",
        else => "none",
    };
}
