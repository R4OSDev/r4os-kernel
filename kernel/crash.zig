pub const MAGIC: u64 = 0x52344f5343525348;
comptime {
    if (@import("builtin").is_test) _ = @import("rust_owner_test");
}
pub const VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 128;

pub const FLAG_CPU: u16 = 1 << 0;
pub const FLAG_PAGE_FAULT: u16 = 1 << 1;
pub const FLAG_MEMORY: u16 = 1 << 2;
pub const FLAG_CONTEXT: u16 = 1 << 3;
pub const FLAG_MESSAGE: u16 = 1 << 4;

pub const Cause = enum(u8) {
    cpu_exception = 1,
    page_fault = 2,
    double_fault = 3,
    machine_check = 4,
    kernel_fatal = 5,
    zig_panic = 6,
    manual_crash_test = 7,
};

pub const BootPhase = enum(u8) {
    unknown = 0,
    entry = 1,
    cpu = 2,
    timer = 3,
    driver = 4,
    input = 5,
    memory = 6,
    storage = 7,
    module = 8,
    platform = 9,
    loader = 10,
    irq = 11,
    service = 12,
    runtime = 13,
    audio = 14,
    network = 15,
    usb = 16,
    driver_policy = 17,
    shell = 18,
    task_runtime = 19,
};

pub const CrashEntry = enum(u8) {
    primary = 0,
    reentrant = 1,
};

pub const ExceptionKind = enum(u8) {
    divide_error = 0,
    debug = 1,
    nmi = 2,
    breakpoint = 3,
    overflow = 4,
    bound_range_exceeded = 5,
    invalid_opcode = 6,
    device_not_available = 7,
    double_fault = 8,
    coprocessor_segment_overrun = 9,
    invalid_tss = 10,
    segment_not_present = 11,
    stack_segment_fault = 12,
    general_protection_fault = 13,
    page_fault = 14,
    x87_floating_point = 16,
    alignment_check = 17,
    machine_check = 18,
    simd_floating_point = 19,
    virtualization = 20,
    control_protection = 21,
    hypervisor_injection = 28,
    vmm_communication = 29,
    security = 30,
    unknown = 255,
};

pub const MemoryOwner = enum(u8) {
    kernel = 0,
    driver = 1,
    protocol = 2,
    r4x_instance = 3,
    task = 4,
    device = 5,
    bootloader = 6,
    system = 7,
    unknown = 254,
    unavailable = 255,
};

pub const MemoryKind = enum(u8) {
    boot = 0,
    kernel = 1,
    kernel_heap = 2,
    page_table = 3,
    virtual_range = 4,
    program_image = 5,
    app_heap = 6,
    app_stack = 7,
    dma = 8,
    mmio = 9,
    framebuffer = 10,
    reserved = 11,
    free = 12,
    unknown = 13,
    unavailable = 255,
};

pub const MemoryStatus = enum(u8) {
    free = 0,
    reserved = 1,
    committed = 2,
    guard = 3,
    mapped = 4,
    released = 5,
    @"error" = 6,
    unknown = 254,
    unavailable = 255,
};

pub const MemoryTraceState = enum(u8) {
    unavailable = 0,
    untracked = 1,
    tracked = 2,
};

pub const ContextState = enum(u8) {
    unavailable = 0,
    available = 1,
};

pub const CpuRegisters = extern struct {
    r15: u64 = 0,
    r14: u64 = 0,
    r13: u64 = 0,
    r12: u64 = 0,
    r11: u64 = 0,
    r10: u64 = 0,
    r9: u64 = 0,
    r8: u64 = 0,
    rsi: u64 = 0,
    rdi: u64 = 0,
    rbp: u64 = 0,
    rdx: u64 = 0,
    rcx: u64 = 0,
    rbx: u64 = 0,
    rax: u64 = 0,
};

pub const CpuFrameSnapshot = extern struct {
    registers: CpuRegisters = .{},
    vector: u64 = 0,
    error_code: u64 = 0,
    rip: u64 = 0,
    rsp: u64 = 0,
    cs: u64 = 0,
    rflags: u64 = 0,
};

pub const CpuExceptionInfo = extern struct {
    vector: u16 = 0,
    kind: ExceptionKind = .unknown,
    error_code: u64 = 0,
    rip: u64 = 0,
    rsp: u64 = 0,
    rbp: u64 = 0,
    cs: u64 = 0,
    rflags: u64 = 0,
    registers: CpuRegisters = .{},
};

pub const PageFaultInfo = extern struct {
    fault_address: u64 = 0,
    raw_error_code: u64 = 0,
    present: u8 = 0,
    write: u8 = 0,
    user: u8 = 0,
    reserved_bit: u8 = 0,
    instruction_fetch: u8 = 0,
    protection_key: u8 = 0,
    shadow_stack: u8 = 0,
    sgx: u8 = 0,
};

pub const FaultMemoryInfo = extern struct {
    state: MemoryTraceState = .unavailable,
    block_id: u32 = 0,
    owner: MemoryOwner = .unavailable,
    owner_id: u64 = 0,
    kind: MemoryKind = .unavailable,
    status: MemoryStatus = .unavailable,
    phys_base: u64 = 0,
    phys_len: u64 = 0,
    virt_base: u64 = 0,
    virt_len: u64 = 0,
};

pub const ExecutionContext = extern struct {
    state: ContextState = .unavailable,
    r4x_instance_id: u32 = 0,
    task_id: u32 = 0,
    owner_id: u64 = 0,
    image_base: u64 = 0,
    image_size: u64 = 0,
    instruction_offset: u64 = 0,
    tag: [16]u8 = .{0} ** 16,
};

pub const FixedText = extern struct {
    len: u16 = 0,
    bytes: [MAX_MESSAGE_BYTES]u8 = .{0} ** MAX_MESSAGE_BYTES,

    pub fn set(self: *FixedText, text: []const u8) void {
        r4os_rust_crash_text(self, text.ptr, text.len);
    }

    pub fn slice(self: *const FixedText) []const u8 {
        return self.bytes[0..self.len];
    }
};

pub const CrashReport = extern struct {
    magic: u64 = MAGIC,
    version: u16 = VERSION,
    flags: u16 = 0,
    cause: Cause = .manual_crash_test,
    boot_phase: BootPhase = .unknown,
    reserved0: u16 = 0,
    stop_code: u32 = 0,
    ticks: u64 = 0,
    cpu: CpuExceptionInfo = .{},
    page_fault: PageFaultInfo = .{},
    memory: FaultMemoryInfo = .{},
    context: ExecutionContext = .{},
    message: FixedText = .{},
};

pub const BuildInput = struct {
    cause: Cause = .manual_crash_test,
    boot_phase: BootPhase = .unknown,
    ticks: u64 = 0,
    cpu: ?CpuFrameSnapshot = null,
    cr2: u64 = 0,
    memory: FaultMemoryInfo = unavailableMemory(),
    context: ExecutionContext = unavailableContext(),
    message: []const u8 = "",
};

pub const CpuExceptionInput = struct {
    frame: CpuFrameSnapshot,
    cr2: u64 = 0,
    boot_phase: BootPhase = .unknown,
    ticks: u64 = 0,
    memory: FaultMemoryInfo = unavailableMemory(),
    context: ExecutionContext = unavailableContext(),
    message: []const u8 = "",
};

// Only fixed C values cross to the one Rust crash-record/depth owner.
const RawBuildInput = extern struct {
    cause: Cause,
    phase: BootPhase,
    ticks: u64,
    cpu: ?*const CpuFrameSnapshot,
    cr2: u64,
    memory: *const FaultMemoryInfo,
    context: *const ExecutionContext,
    message: [*]const u8,
    message_len: usize,
};
const TextView = extern struct { data: [*]const u8, len: usize };
comptime {
    if (@sizeOf(CrashReport) != 496 or @offsetOf(CrashReport, "cpu") != 32 or
        @offsetOf(CrashReport, "message") != 360 or @sizeOf(RawBuildInput) != 64 or
        @offsetOf(RawBuildInput, "cpu") != 16) @compileError("Rust crash-record layout differs");
}
extern fn r4os_rust_crash_reset() callconv(.c) void;
extern fn r4os_rust_crash_depth() callconv(.c) u8;
extern fn r4os_rust_crash_enter() callconv(.c) CrashEntry;
extern fn r4os_rust_crash_build(*CrashReport, *const RawBuildInput) callconv(.c) void;
extern fn r4os_rust_crash_cpu(*CpuExceptionInfo, *const CpuFrameSnapshot) callconv(.c) void;
extern fn r4os_rust_crash_pf(*PageFaultInfo, u64, u64) callconv(.c) void;
extern fn r4os_rust_crash_text(*FixedText, [*]const u8, usize) callconv(.c) void;
extern fn r4os_rust_crash_name(*TextView, u32, u8) callconv(.c) void;
extern fn r4os_rust_crash_code(Cause) callconv(.c) u32;
extern fn r4os_rust_crash_vector_code(u64) callconv(.c) u32;
extern fn r4os_rust_crash_vector_cause(u64) callconv(.c) Cause;
extern fn r4os_rust_crash_vector_kind(u64) callconv(.c) ExceptionKind;
pub fn init() void {
    r4os_rust_crash_reset();
}
pub fn enterCrashPath() CrashEntry {
    return r4os_rust_crash_enter();
}
pub fn resetCrashPathForTest() void {
    r4os_rust_crash_reset();
}
pub fn crashDepthForTest() u8 {
    return r4os_rust_crash_depth();
}
pub fn build(input: BuildInput) CrashReport {
    var frame = input.cpu orelse CpuFrameSnapshot{};
    const raw = RawBuildInput{ .cause = input.cause, .phase = input.boot_phase, .ticks = input.ticks, .cpu = if (input.cpu != null) &frame else null, .cr2 = input.cr2, .memory = &input.memory, .context = &input.context, .message = input.message.ptr, .message_len = input.message.len };
    var output: CrashReport = undefined;
    r4os_rust_crash_build(&output, &raw);
    return output;
}

pub fn fromCpuException(input: CpuExceptionInput) CrashReport {
    return build(.{
        .cause = causeForCpuVector(input.frame.vector),
        .boot_phase = input.boot_phase,
        .ticks = input.ticks,
        .cpu = input.frame,
        .cr2 = input.cr2,
        .memory = input.memory,
        .context = input.context,
        .message = input.message,
    });
}

pub fn fromKernelFatal(boot_phase: BootPhase, ticks: u64, message: []const u8) CrashReport {
    return build(.{
        .cause = .kernel_fatal,
        .boot_phase = boot_phase,
        .ticks = ticks,
        .message = message,
    });
}

pub fn fromZigPanic(boot_phase: BootPhase, ticks: u64, message: []const u8) CrashReport {
    return build(.{
        .cause = .zig_panic,
        .boot_phase = boot_phase,
        .ticks = ticks,
        .message = message,
    });
}

pub fn manualCrashTest(boot_phase: BootPhase, ticks: u64, message: []const u8) CrashReport {
    return build(.{
        .cause = .manual_crash_test,
        .boot_phase = boot_phase,
        .ticks = ticks,
        .message = message,
    });
}

pub fn cpuExceptionFromFrame(frame: CpuFrameSnapshot) CpuExceptionInfo {
    var result: CpuExceptionInfo = undefined;
    r4os_rust_crash_cpu(&result, &frame);
    return result;
}

pub fn decodePageFault(error_code: u64, fault_address: u64) PageFaultInfo {
    var result: PageFaultInfo = undefined;
    r4os_rust_crash_pf(&result, error_code, fault_address);
    return result;
}

pub fn unavailableMemory() FaultMemoryInfo {
    return .{};
}

pub fn untrackedMemory() FaultMemoryInfo {
    return .{
        .state = .untracked,
        .owner = .unknown,
        .kind = .unknown,
        .status = .unknown,
    };
}

pub fn faultMemoryFromBlock(block: anytype) FaultMemoryInfo {
    return .{
        .state = .tracked,
        .block_id = block.id,
        .owner = memoryOwnerFromName(@tagName(block.owner)),
        .owner_id = block.owner_id,
        .kind = memoryKindFromName(@tagName(block.kind)),
        .status = memoryStatusFromName(@tagName(block.status)),
        .phys_base = block.phys_base,
        .phys_len = block.phys_len,
        .virt_base = block.virt_base,
        .virt_len = block.virt_len,
    };
}

pub fn unavailableContext() ExecutionContext {
    return .{};
}

pub fn contextInfo(r4x_instance_id: u32, task_id: u32, owner_id: u64) ExecutionContext {
    return .{
        .state = .available,
        .r4x_instance_id = r4x_instance_id,
        .task_id = task_id,
        .owner_id = owner_id,
    };
}

pub fn programContextInfo(r4x_instance_id: u32, task_id: u32, owner_id: u64, image_base: u64, image_size: u64, instruction_offset: u64, tag: [16]u8) ExecutionContext {
    return .{
        .state = .available,
        .r4x_instance_id = r4x_instance_id,
        .task_id = task_id,
        .owner_id = owner_id,
        .image_base = image_base,
        .image_size = image_size,
        .instruction_offset = instruction_offset,
        .tag = tag,
    };
}

pub fn causeForCpuVector(vector: u64) Cause {
    return r4os_rust_crash_vector_cause(vector);
}

pub fn exceptionKindForVector(vector: u64) ExceptionKind {
    return r4os_rust_crash_vector_kind(vector);
}

pub fn causeName(cause: Cause) []const u8 {
    return rustName(0, @intFromEnum(cause));
}

pub fn bootPhaseName(phase: BootPhase) []const u8 {
    return rustName(1, @intFromEnum(phase));
}

pub fn exceptionName(kind: ExceptionKind) []const u8 {
    return rustName(2, @intFromEnum(kind));
}

pub fn memoryOwnerName(owner: MemoryOwner) []const u8 {
    return rustName(3, @intFromEnum(owner));
}

pub fn memoryKindName(kind: MemoryKind) []const u8 {
    return rustName(4, @intFromEnum(kind));
}

pub fn memoryStatusName(status: MemoryStatus) []const u8 {
    return rustName(5, @intFromEnum(status));
}

pub fn stopCodeForCause(cause: Cause) u32 {
    return r4os_rust_crash_code(cause);
}

pub fn stopCodeForCpuVector(vector: u64) u32 {
    return r4os_rust_crash_vector_code(vector);
}

fn rustName(group: u32, value: u8) []const u8 {
    var output: TextView = undefined;
    r4os_rust_crash_name(&output, group, value);
    return output.data[0..output.len];
}

fn memoryOwnerFromName(name: []const u8) MemoryOwner {
    if (strEq(name, "kernel")) return .kernel;
    if (strEq(name, "driver")) return .driver;
    if (strEq(name, "protocol")) return .protocol;
    if (strEq(name, "r4x_instance")) return .r4x_instance;
    if (strEq(name, "task")) return .task;
    if (strEq(name, "device")) return .device;
    if (strEq(name, "bootloader")) return .bootloader;
    if (strEq(name, "system")) return .system;
    return .unknown;
}

fn memoryKindFromName(name: []const u8) MemoryKind {
    if (strEq(name, "boot")) return .boot;
    if (strEq(name, "kernel")) return .kernel;
    if (strEq(name, "kernel_heap")) return .kernel_heap;
    if (strEq(name, "page_table")) return .page_table;
    if (strEq(name, "virtual_range")) return .virtual_range;
    if (strEq(name, "program_image")) return .program_image;
    if (strEq(name, "app_heap")) return .app_heap;
    if (strEq(name, "app_stack")) return .app_stack;
    if (strEq(name, "dma")) return .dma;
    if (strEq(name, "mmio")) return .mmio;
    if (strEq(name, "framebuffer")) return .framebuffer;
    if (strEq(name, "reserved")) return .reserved;
    if (strEq(name, "free")) return .free;
    if (strEq(name, "unknown")) return .unknown;
    return .unknown;
}

fn memoryStatusFromName(name: []const u8) MemoryStatus {
    if (strEq(name, "free")) return .free;
    if (strEq(name, "reserved")) return .reserved;
    if (strEq(name, "committed")) return .committed;
    if (strEq(name, "guard")) return .guard;
    if (strEq(name, "mapped")) return .mapped;
    if (strEq(name, "released")) return .released;
    if (strEq(name, "error")) return .@"error";
    return .unknown;
}

fn bit(value: u64, index: u6) u8 {
    return @intCast((value >> index) & 1);
}

fn min(a: usize, b: usize) usize {
    return if (a < b) a else b;
}

fn saturateU16(value: u64) u16 {
    return if (value > 0xffff) 0xffff else @intCast(value);
}

fn strEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (a[i] != b[i]) return false;
    }
    return true;
}

test "page fault error code is decoded into explicit flags" {
    const std = @import("std");
    const info = decodePageFault((1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 5) | (1 << 6) | (1 << 15), 0xdead_beef);
    try std.testing.expectEqual(@as(u64, 0xdead_beef), info.fault_address);
    try std.testing.expectEqual(@as(u8, 1), info.present);
    try std.testing.expectEqual(@as(u8, 1), info.write);
    try std.testing.expectEqual(@as(u8, 1), info.user);
    try std.testing.expectEqual(@as(u8, 1), info.reserved_bit);
    try std.testing.expectEqual(@as(u8, 1), info.instruction_fetch);
    try std.testing.expectEqual(@as(u8, 1), info.protection_key);
    try std.testing.expectEqual(@as(u8, 1), info.shadow_stack);
    try std.testing.expectEqual(@as(u8, 1), info.sgx);
}

test "cpu exception report captures frame and page fault details" {
    const std = @import("std");
    const frame: CpuFrameSnapshot = .{
        .registers = .{ .rax = 1, .rbx = 2, .rbp = 0x3333 },
        .vector = 14,
        .error_code = 0b10111,
        .rip = 0x1000,
        .rsp = 0x2000,
        .cs = 0x8,
        .rflags = 0x202,
    };
    const report = fromCpuException(.{
        .frame = frame,
        .cr2 = 0xcafe_babe,
        .boot_phase = .runtime,
        .ticks = 42,
        .message = "fault",
    });

    try std.testing.expectEqual(MAGIC, report.magic);
    try std.testing.expectEqual(VERSION, report.version);
    try std.testing.expectEqual(Cause.page_fault, report.cause);
    try std.testing.expectEqual(BootPhase.runtime, report.boot_phase);
    try std.testing.expectEqual(@as(u64, 42), report.ticks);
    try std.testing.expect((report.flags & FLAG_CPU) != 0);
    try std.testing.expect((report.flags & FLAG_PAGE_FAULT) != 0);
    try std.testing.expect((report.flags & FLAG_MESSAGE) != 0);
    try std.testing.expectEqual(ExceptionKind.page_fault, report.cpu.kind);
    try std.testing.expectEqual(@as(u64, 0xcafe_babe), report.page_fault.fault_address);
    try std.testing.expectEqualStrings("fault", report.message.slice());
}

test "cpu exception stop codes include the vector when the cause is generic" {
    const std = @import("std");
    const invalid_opcode = fromCpuException(.{
        .frame = .{
            .vector = 6,
            .rip = 0x1000,
            .rsp = 0x2000,
            .cs = 0x8,
            .rflags = 0x202,
        },
    });
    const general_protection = fromCpuException(.{
        .frame = .{
            .vector = 13,
            .error_code = 0x10,
            .rip = 0x3000,
            .rsp = 0x4000,
            .cs = 0x8,
            .rflags = 0x202,
        },
    });

    try std.testing.expectEqual(Cause.cpu_exception, invalid_opcode.cause);
    try std.testing.expectEqual(@as(u32, 0x0001_0006), invalid_opcode.stop_code);
    try std.testing.expectEqual(ExceptionKind.invalid_opcode, invalid_opcode.cpu.kind);
    try std.testing.expectEqual(Cause.cpu_exception, general_protection.cause);
    try std.testing.expectEqual(@as(u32, 0x0001_000d), general_protection.stop_code);
    try std.testing.expectEqual(ExceptionKind.general_protection_fault, general_protection.cpu.kind);
}

test "kernel fatal and zig panic reports keep phase and stop code" {
    const std = @import("std");
    const fatal = fromKernelFatal(.driver_policy, 77, "driver policy missing");
    const zig_panic = fromZigPanic(.shell, 88, "panic message");

    try std.testing.expectEqual(Cause.kernel_fatal, fatal.cause);
    try std.testing.expectEqual(BootPhase.driver_policy, fatal.boot_phase);
    try std.testing.expectEqual(@as(u32, 0x0002_0001), fatal.stop_code);
    try std.testing.expect((fatal.flags & FLAG_MESSAGE) != 0);
    try std.testing.expectEqualStrings("driver policy missing", fatal.message.slice());

    try std.testing.expectEqual(Cause.zig_panic, zig_panic.cause);
    try std.testing.expectEqual(BootPhase.shell, zig_panic.boot_phase);
    try std.testing.expectEqual(@as(u32, 0x0002_0002), zig_panic.stop_code);
    try std.testing.expect((zig_panic.flags & FLAG_MESSAGE) != 0);
    try std.testing.expectEqualStrings("panic message", zig_panic.message.slice());
}

test "fault memory can be snapshotted from a MemoryBlock-shaped value without importing memory policy" {
    const std = @import("std");
    const Owner = enum { kernel, driver, protocol, r4x_instance, task, device, bootloader, system };
    const Kind = enum { boot, kernel, kernel_heap, page_table, virtual_range, program_image, app_heap, app_stack, dma, mmio, framebuffer, reserved, free, unknown };
    const Status = enum { free, reserved, committed, guard, mapped, released, @"error" };
    const block = .{
        .id = @as(u32, 7),
        .owner = Owner.r4x_instance,
        .owner_id = @as(u64, 99),
        .kind = Kind.app_stack,
        .status = Status.guard,
        .phys_base = @as(u64, 0),
        .phys_len = @as(u64, 0),
        .virt_base = @as(u64, 0xffff_8000),
        .virt_len = @as(u64, 0x4000),
    };

    const info = faultMemoryFromBlock(block);
    try std.testing.expectEqual(MemoryTraceState.tracked, info.state);
    try std.testing.expectEqual(MemoryOwner.r4x_instance, info.owner);
    try std.testing.expectEqual(MemoryKind.app_stack, info.kind);
    try std.testing.expectEqual(MemoryStatus.guard, info.status);
    try std.testing.expectEqual(@as(u64, 99), info.owner_id);
}

test "crash path reentrancy is explicit and lock-free" {
    const std = @import("std");
    resetCrashPathForTest();
    try std.testing.expectEqual(CrashEntry.primary, enterCrashPath());
    try std.testing.expectEqual(@as(u8, 1), crashDepthForTest());
    try std.testing.expectEqual(CrashEntry.reentrant, enterCrashPath());
    try std.testing.expectEqual(@as(u8, 2), crashDepthForTest());
}

test "fixed text clips without allocation" {
    const std = @import("std");
    var text: FixedText = .{};
    text.set("abc");
    try std.testing.expectEqualStrings("abc", text.slice());

    const long = "................................................................................................................................................................";
    text.set(long);
    try std.testing.expectEqual(@as(usize, MAX_MESSAGE_BYTES), text.slice().len);
}
