const bootlog = @import("../../kernel/bootlog.zig");
const cpu = @import("../../platform/cpu.zig");

pub const state_storage_bytes: usize = 4096;
pub const state_storage_align: usize = 64;

const CR0_MP: u64 = 1 << 1;
const CR0_EM: u64 = 1 << 2;
const CR0_TS: u64 = 1 << 3;
const CR0_NE: u64 = 1 << 5;

const CR4_OSFXSR: u64 = 1 << 9;
const CR4_OSXMMEXCPT: u64 = 1 << 10;
const CR4_OSXSAVE: u64 = 1 << 18;

const XCR0_X87: u64 = 1 << 0;
const XCR0_SSE: u64 = 1 << 1;
const XCR0_AVX: u64 = 1 << 2;
const XCR0_R4OS_SSE_MASK: u64 = XCR0_X87 | XCR0_SSE;
const XCR0_R4OS_AVX_MASK: u64 = XCR0_R4OS_SSE_MASK | XCR0_AVX;

pub const backend_none: u32 = 0;
pub const backend_fxsave: u32 = 1;
pub const backend_xsave: u32 = 2;

pub const simd_abi_none: u32 = 0;
pub const simd_abi_sse2: u32 = 1;
pub const simd_abi_avx: u32 = 2;
pub const simd_abi_avx2: u32 = 3;

const Backend = enum(u32) {
    none = backend_none,
    fxsave = backend_fxsave,
    xsave = backend_xsave,
};

pub const Status = struct {
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

const CpuidRegs = struct {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
};

extern fn r4os_read_cr0() callconv(.c) u64;
extern fn r4os_write_cr0(value: u64) callconv(.c) void;
extern fn r4os_read_cr4() callconv(.c) u64;
extern fn r4os_write_cr4(value: u64) callconv(.c) void;
extern fn r4os_xsetbv(index: u32, value: u64) callconv(.c) void;
extern fn r4os_fninit() callconv(.c) void;
extern fn r4os_fxsave(dest: [*]u8) callconv(.c) void;
extern fn r4os_fxrstor(src: [*]const u8) callconv(.c) void;
extern fn r4os_xsave(dest: [*]u8, mask: u64) callconv(.c) void;
extern fn r4os_xrstor(src: [*]const u8, mask: u64) callconv(.c) void;

var initialized: bool = false;
var active_backend: Backend = .none;
var active_state_bytes: u32 = 0;
var active_xcr0_mask: u64 = 0;
var detected_avx_supported: bool = false;
var detected_avx2_supported: bool = false;
var active_avx_enabled: bool = false;
var active_avx2_enabled: bool = false;
var active_simd_abi: u32 = simd_abi_none;
var active_cr0: u64 = 0;
var active_cr4: u64 = 0;
var save_counter: u64 = 0;
var restore_counter: u64 = 0;
var task_init_counter: u64 = 0;
var task_state_byte_total: u64 = 0;
var initial_state: [state_storage_bytes]u8 align(state_storage_align) = .{0} ** state_storage_bytes;

pub fn init() bool {
    if (initialized) return active_backend != .none;

    const s = cpu.status();
    if (!s.features.fxsr or !s.features.sse or !s.features.sse2) {
        initialized = true;
        cpu.setModuleSimdAllowed(false);
        logStatus("missing fxsr/sse/sse2");
        return false;
    }

    var cr0 = r4os_read_cr0();
    cr0 &= ~(CR0_EM | CR0_TS);
    cr0 |= CR0_MP | CR0_NE;
    r4os_write_cr0(cr0);

    var cr4 = r4os_read_cr4();
    cr4 |= CR4_OSFXSR | CR4_OSXMMEXCPT;

    var target_xcr0_mask: u64 = XCR0_R4OS_SSE_MASK;
    detected_avx_supported = s.features.avx and xsaveComponentSupported(XCR0_AVX);
    detected_avx2_supported = detected_avx_supported and s.features.avx2;
    if (detected_avx_supported) target_xcr0_mask = XCR0_R4OS_AVX_MASK;

    if (s.features.xsave) {
        cr4 |= CR4_OSXSAVE;
        r4os_write_cr4(cr4);
        r4os_xsetbv(0, target_xcr0_mask);
        const xcr0 = xgetbv(0);
        if ((xcr0 & target_xcr0_mask) == target_xcr0_mask) {
            const state_bytes = detectXsaveBytes(target_xcr0_mask) orelse {
                // A guessed XSAVE size is unsafe: XSAVE would write past the
                // task buffer and XRSTOR could consume truncated state.  Do
                // not publish a partially configured backend.  cpu_boot
                // treats this as a hard CPU/task-state initialization error.
                r4os_write_cr4(cr4 & ~CR4_OSXSAVE);
                active_cr0 = r4os_read_cr0();
                active_cr4 = r4os_read_cr4();
                initialized = true;
                cpu.setModuleSimdAllowed(false);
                logStatus("invalid xsave state size");
                return false;
            };
            active_backend = .xsave;
            active_xcr0_mask = target_xcr0_mask;
            active_state_bytes = state_bytes;
            active_avx_enabled = (target_xcr0_mask & XCR0_AVX) != 0;
            active_avx2_enabled = active_avx_enabled and detected_avx2_supported;
            cpu.noteOsXsaveEnabled(xcr0);
        }
    }

    if (active_backend == .none) {
        r4os_write_cr4(cr4 & ~CR4_OSXSAVE);
        active_backend = .fxsave;
        active_xcr0_mask = 0;
        active_state_bytes = 512;
        active_avx_enabled = false;
        active_avx2_enabled = false;
    }

    active_simd_abi = if (active_avx2_enabled)
        simd_abi_avx2
    else if (active_avx_enabled)
        simd_abi_avx
    else
        simd_abi_sse2;

    active_cr0 = r4os_read_cr0();
    active_cr4 = r4os_read_cr4();
    r4os_fninit();
    const state_bytes: usize = active_state_bytes;
    saveRaw(initial_state[0..state_bytes]);
    restoreRaw(initial_state[0..state_bytes]);
    initialized = true;
    cpu.setModuleSimdAllowed(true);
    logStatus("ready");
    return true;
}

pub fn initTaskState(state: []u8) bool {
    const bytes = activeStateBytes();
    if (!validTaskStateBuffer(state.ptr, state.len, bytes)) return false;
    @memcpy(state[0..bytes], initial_state[0..bytes]);
    task_init_counter +%= 1;
    task_state_byte_total +%= active_state_bytes;
    return true;
}

pub fn saveTaskState(state: []u8) void {
    const bytes = activeStateBytes();
    if (!validTaskStateBuffer(state.ptr, state.len, bytes)) return;
    saveRaw(state[0..bytes]);
    save_counter +%= 1;
}

pub fn restoreTaskState(state: []const u8) void {
    const bytes = activeStateBytes();
    if (!validTaskStateBuffer(state.ptr, state.len, bytes)) return;
    restoreRaw(state[0..bytes]);
    restore_counter +%= 1;
}

// R4D interrupt handlers are normal freestanding modules and may contain
// compiler-generated SSE/AVX instructions.  They do not own a schedulable
// task state, so an IRQ must run them from the architectural initial state
// after the interrupted task has been saved.  Otherwise an asynchronous
// handler can overwrite live XMM/YMM lanes in the interrupted R4X program.
pub fn restoreInitialState() bool {
    const bytes = activeStateBytes();
    if (bytes == 0) return false;
    restoreRaw(initial_state[0..bytes]);
    restore_counter +%= 1;
    return true;
}

pub fn activeStateBytes() usize {
    if (!initialized or active_backend == .none) return 0;
    return active_state_bytes;
}

pub fn status() Status {
    return .{
        .initialized = initialized,
        .enabled = active_backend != .none,
        .backend = @intFromEnum(active_backend),
        .state_bytes = active_state_bytes,
        .state_storage_bytes = state_storage_bytes,
        .xcr0_mask = active_xcr0_mask,
        .avx_supported = detected_avx_supported,
        .avx_enabled = active_avx_enabled,
        .avx2_supported = detected_avx2_supported,
        .avx2_enabled = active_avx2_enabled,
        .simd_abi = active_simd_abi,
        .cr0 = active_cr0,
        .cr4 = active_cr4,
        .save_count = save_counter,
        .restore_count = restore_counter,
        .task_init_count = task_init_counter,
        .task_state_bytes = task_state_byte_total,
    };
}

pub fn backendName(code: u32) []const u8 {
    return switch (code) {
        backend_fxsave => "fxsave",
        backend_xsave => "xsave",
        else => "none",
    };
}

fn saveRaw(state: []u8) void {
    switch (active_backend) {
        .fxsave => r4os_fxsave(state.ptr),
        .xsave => r4os_xsave(state.ptr, active_xcr0_mask),
        .none => {},
    }
}

fn restoreRaw(state: []const u8) void {
    switch (active_backend) {
        .fxsave => r4os_fxrstor(state.ptr),
        .xsave => r4os_xrstor(state.ptr, active_xcr0_mask),
        .none => {},
    }
}

fn detectXsaveBytes(mask: u64) ?u32 {
    const ci = cpu.status();
    if (ci.max_basic_leaf < 0x0D) return null;
    const leaf = cpuid(0x0D, 0);
    var required: u64 = 576; // Legacy x87/SSE area plus XSAVE header.
    if ((mask & XCR0_AVX) != 0) {
        const avx_component = cpuid(0x0D, 2);
        if (avx_component.eax == 0) return null;
        const avx_end = @as(u64, avx_component.ebx) + @as(u64, avx_component.eax);
        if (avx_end > required) required = avx_end;
    }
    if (@as(u64, leaf.ebx) < required or leaf.ebx > state_storage_bytes) return null;
    return leaf.ebx;
}

fn validTaskStateBuffer(ptr: [*]const u8, len: usize, bytes: usize) bool {
    if (!initialized or active_backend == .none or bytes == 0 or len < bytes) return false;
    return (@intFromPtr(ptr) & (state_storage_align - 1)) == 0;
}

fn xsaveComponentSupported(mask: u64) bool {
    const ci = cpu.status();
    if (!ci.features.xsave or ci.max_basic_leaf < 0x0D) return false;
    const leaf = cpuid(0x0D, 0);
    const supported = @as(u64, leaf.eax) | (@as(u64, leaf.edx) << 32);
    return (supported & mask) == mask;
}

fn logStatus(note: []const u8) void {
    const st = status();
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

fn cpuid(leaf: u32, subleaf: u32) CpuidRegs {
    var eax: u32 = leaf;
    var ebx: u32 = 0;
    var ecx: u32 = subleaf;
    var edx: u32 = 0;
    asm volatile ("cpuid"
        : [eax] "={eax}" (eax),
          [ebx] "={ebx}" (ebx),
          [ecx] "={ecx}" (ecx),
          [edx] "={edx}" (edx),
        : [leaf_in] "{eax}" (leaf),
          [subleaf_in] "{ecx}" (subleaf),
    );
    return .{ .eax = eax, .ebx = ebx, .ecx = ecx, .edx = edx };
}

fn xgetbv(index: u32) u64 {
    var eax: u32 = 0;
    var edx: u32 = 0;
    asm volatile ("xgetbv"
        : [eax] "={eax}" (eax),
          [edx] "={edx}" (edx),
        : [index] "{ecx}" (index),
    );
    return (@as(u64, edx) << 32) | eax;
}
