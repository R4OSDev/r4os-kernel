// Private fixed C view. CPU discovery and mutable FPU/SIMD notes live in Rust.
const bootlog = @import("../kernel/bootlog.zig");
const FeatureSet = extern struct {
    tsc: bool = false,
    msr: bool = false,
    apic: bool = false,
    mtrr: bool = false,
    pat: bool = false,
    fxsr: bool = false,
    sse: bool = false,
    sse2: bool = false,
    sse3: bool = false,
    ssse3: bool = false,
    sse41: bool = false,
    sse42: bool = false,
    x2apic: bool = false,
    tsc_deadline: bool = false,
    xsave: bool = false,
    osxsave: bool = false,
    avx: bool = false,
    avx2: bool = false,
    fsgsbase: bool = false,
    erms: bool = false,
    invpcid: bool = false,
    clflushopt: bool = false,
    syscall: bool = false,
    nx: bool = false,
    page1gb: bool = false,
    rdtscp: bool = false,
    long_mode: bool = false,
    invariant_tsc: bool = false,
};

pub const Status = extern struct {
    detected: bool = false,
    vendor: [12]u8 = .{0} ** 12,
    vendor_len: u64 = 0,
    brand: [48]u8 = .{0} ** 48,
    brand_len: u64 = 0,
    max_basic_leaf: u32 = 0,
    max_extended_leaf: u32 = 0,
    stepping: u8 = 0,
    base_model: u8 = 0,
    base_family: u8 = 0,
    display_model: u16 = 0,
    display_family: u16 = 0,
    processor_type: u8 = 0,
    apic_id: u8 = 0,
    logical_processors: u8 = 0,
    clflush_line_bytes: u16 = 0,
    physical_address_bits: u8 = 0,
    virtual_address_bits: u8 = 0,
    l2_cache_kb: u32 = 0,
    tsc_denominator: u32 = 0,
    tsc_numerator: u32 = 0,
    crystal_hz: u32 = 0,
    base_mhz: u16 = 0,
    max_mhz: u16 = 0,
    bus_mhz: u16 = 0,
    xcr0: u64 = 0,
    pat_msr: u64 = 0,
    mtrr_cap: u64 = 0,
    features: FeatureSet = .{},
};

extern fn r4os_rust_cpu_detect(output: *Status) void;
extern fn r4os_rust_cpu_status(output: *Status) void;
extern fn r4os_rust_cpu_control(operation: u8, value: u64) u64;
pub fn detect() Status {
    var value: Status = undefined;
    r4os_rust_cpu_detect(&value);
    return value;
}
pub fn status() Status {
    var value: Status = undefined;
    r4os_rust_cpu_status(&value);
    return value;
}
pub fn patAvailable() bool {
    return r4os_rust_cpu_control(0, 0) != 0;
}
pub fn mtrrAvailable() bool {
    return r4os_rust_cpu_control(1, 0) != 0;
}
pub fn writeCombiningBasisAvailable() bool {
    return r4os_rust_cpu_control(2, 0) != 0;
}
pub fn patEntry5IsWriteCombining() bool {
    return r4os_rust_cpu_control(3, 0) != 0;
}
pub fn moduleSimdAllowed() bool {
    return r4os_rust_cpu_control(4, 0) != 0;
}
pub fn setModuleSimdAllowed(allowed: bool) void {
    _ = r4os_rust_cpu_control(5, @intFromBool(allowed));
}
pub fn noteOsXsaveEnabled(value: u64) void {
    _ = r4os_rust_cpu_control(6, value);
}
// Original CPUID.1 boot-target projection; full local APIC identity is separate.
pub fn bootApicId() u32 {
    return @intCast(r4os_rust_cpu_control(7, 0));
}
comptime {
    if (@sizeOf(FeatureSet) != 28 or @sizeOf(Status) != 184 or @offsetOf(Status, "features") != 152 or @offsetOf(Status, "vendor_len") != 16)
        @compileError("Rust CPU C layout differs");
}
export fn r4os_zig_cpu_log(value: *const Status, simd: bool) callconv(.c) void {
    if (comptime @import("builtin").is_test) return;
    logStatus(value.*, simd);
}
fn logStatus(current: Status, simd: bool) void {
    bootlog.puts("[CPU] vendor=");
    bootlog.puts(current.vendor[0..current.vendor_len]);
    bootlog.puts(" family=");
    bootlog.putDec(current.display_family);
    bootlog.puts(" model=");
    bootlog.putDec(current.display_model);
    bootlog.puts(" stepping=");
    bootlog.putDec(current.stepping);
    bootlog.puts(" apic=");
    bootlog.putDec(current.apic_id);
    bootlog.puts(" features=");
    bootlog.puts(if (current.features.long_mode) "lm" else "no-lm");
    bootlog.puts(",");
    bootlog.puts(if (current.features.pat) "pat" else "no-pat");
    bootlog.puts(",");
    bootlog.puts(if (current.features.mtrr) "mtrr" else "no-mtrr");
    bootlog.puts(",");
    bootlog.puts(if (current.features.invariant_tsc) "invtsc" else "tsc-variable");
    bootlog.puts(",");
    bootlog.puts(if (current.features.avx) "avx" else "no-avx");
    bootlog.puts(",");
    bootlog.puts(if (current.features.avx2) "avx2" else "no-avx2");
    bootlog.puts(" modules_simd=");
    bootlog.puts(if (simd) "allowed" else "blocked");
    bootlog.puts("\r\n");
}
