pub const MAGIC: u64 = 0x52344f5343525348;
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
        self.* = .{};
        const n = min(text.len, MAX_MESSAGE_BYTES);
        var i: usize = 0;
        while (i < n) : (i += 1) self.bytes[i] = text[i];
        self.len = @intCast(n);
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

var crash_depth: u8 = 0;

pub fn init() void {
    crash_depth = 0;
}

pub fn enterCrashPath() CrashEntry {
    if (crash_depth == 0) {
        crash_depth = 1;
        return .primary;
    }
    if (crash_depth != 255) crash_depth += 1;
    return .reentrant;
}

pub fn resetCrashPathForTest() void {
    crash_depth = 0;
}

pub fn crashDepthForTest() u8 {
    return crash_depth;
}

pub fn build(input: BuildInput) CrashReport {
    var report: CrashReport = .{
        .cause = input.cause,
        .boot_phase = input.boot_phase,
        .ticks = input.ticks,
        .stop_code = stopCodeForCause(input.cause),
    };

    if (input.cpu) |frame| {
        report.flags |= FLAG_CPU;
        report.cpu = cpuExceptionFromFrame(frame);
        report.stop_code = stopCodeForCpuVector(frame.vector);
        if (frame.vector == 14 or input.cause == .page_fault) {
            report.flags |= FLAG_PAGE_FAULT;
            report.page_fault = decodePageFault(frame.error_code, input.cr2);
            if (frame.vector != 14) report.stop_code = stopCodeForCause(.page_fault);
        }
    }

    if (input.memory.state != .unavailable) {
        report.flags |= FLAG_MEMORY;
        report.memory = input.memory;
    }

    if (input.context.state == .available) {
        report.flags |= FLAG_CONTEXT;
        report.context = input.context;
    }

    if (input.message.len != 0) {
        report.flags |= FLAG_MESSAGE;
        report.message.set(input.message);
    }

    return report;
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
    return .{
        .vector = saturateU16(frame.vector),
        .kind = exceptionKindForVector(frame.vector),
        .error_code = frame.error_code,
        .rip = frame.rip,
        .rsp = frame.rsp,
        .rbp = frame.registers.rbp,
        .cs = frame.cs,
        .rflags = frame.rflags,
        .registers = frame.registers,
    };
}

pub fn decodePageFault(error_code: u64, fault_address: u64) PageFaultInfo {
    return .{
        .fault_address = fault_address,
        .raw_error_code = error_code,
        .present = bit(error_code, 0),
        .write = bit(error_code, 1),
        .user = bit(error_code, 2),
        .reserved_bit = bit(error_code, 3),
        .instruction_fetch = bit(error_code, 4),
        .protection_key = bit(error_code, 5),
        .shadow_stack = bit(error_code, 6),
        .sgx = bit(error_code, 15),
    };
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
    return switch (vector) {
        8 => .double_fault,
        14 => .page_fault,
        18 => .machine_check,
        else => .cpu_exception,
    };
}

pub fn exceptionKindForVector(vector: u64) ExceptionKind {
    return switch (vector) {
        0 => .divide_error,
        1 => .debug,
        2 => .nmi,
        3 => .breakpoint,
        4 => .overflow,
        5 => .bound_range_exceeded,
        6 => .invalid_opcode,
        7 => .device_not_available,
        8 => .double_fault,
        9 => .coprocessor_segment_overrun,
        10 => .invalid_tss,
        11 => .segment_not_present,
        12 => .stack_segment_fault,
        13 => .general_protection_fault,
        14 => .page_fault,
        16 => .x87_floating_point,
        17 => .alignment_check,
        18 => .machine_check,
        19 => .simd_floating_point,
        20 => .virtualization,
        21 => .control_protection,
        28 => .hypervisor_injection,
        29 => .vmm_communication,
        30 => .security,
        else => .unknown,
    };
}

pub fn causeName(cause: Cause) []const u8 {
    return switch (cause) {
        .cpu_exception => "CPU Exception",
        .page_fault => "Page Fault",
        .double_fault => "Double Fault",
        .machine_check => "Machine Check",
        .kernel_fatal => "Kernel Fatal",
        .zig_panic => "Zig Panic",
        .manual_crash_test => "Manual Crash Test",
    };
}

pub fn bootPhaseName(phase: BootPhase) []const u8 {
    return switch (phase) {
        .unknown => "unknown",
        .entry => "entry",
        .cpu => "cpu",
        .timer => "timer",
        .driver => "driver",
        .input => "input",
        .memory => "memory",
        .storage => "storage",
        .module => "module",
        .platform => "platform",
        .loader => "loader",
        .irq => "irq",
        .service => "service",
        .runtime => "runtime",
        .audio => "audio",
        .network => "network",
        .usb => "usb",
        .driver_policy => "driver_policy",
        .shell => "shell",
        .task_runtime => "task_runtime",
    };
}

pub fn exceptionName(kind: ExceptionKind) []const u8 {
    return switch (kind) {
        .divide_error => "Divide Error",
        .debug => "Debug",
        .nmi => "Non-maskable Interrupt",
        .breakpoint => "Breakpoint",
        .overflow => "Overflow",
        .bound_range_exceeded => "Bound Range Exceeded",
        .invalid_opcode => "Invalid Opcode",
        .device_not_available => "Device Not Available",
        .double_fault => "Double Fault",
        .coprocessor_segment_overrun => "Coprocessor Segment Overrun",
        .invalid_tss => "Invalid TSS",
        .segment_not_present => "Segment Not Present",
        .stack_segment_fault => "Stack-Segment Fault",
        .general_protection_fault => "General Protection Fault",
        .page_fault => "Page Fault",
        .x87_floating_point => "x87 Floating-Point Exception",
        .alignment_check => "Alignment Check",
        .machine_check => "Machine Check",
        .simd_floating_point => "SIMD Floating-Point Exception",
        .virtualization => "Virtualization Exception",
        .control_protection => "Control Protection Exception",
        .hypervisor_injection => "Hypervisor Injection Exception",
        .vmm_communication => "VMM Communication Exception",
        .security => "Security Exception",
        .unknown => "Unknown",
    };
}

pub fn memoryOwnerName(owner: MemoryOwner) []const u8 {
    return switch (owner) {
        .kernel => "kernel",
        .driver => "driver",
        .protocol => "protocol",
        .r4x_instance => "r4x_instance",
        .task => "task",
        .device => "device",
        .bootloader => "bootloader",
        .system => "system",
        .unknown => "unknown",
        .unavailable => "unavailable",
    };
}

pub fn memoryKindName(kind: MemoryKind) []const u8 {
    return switch (kind) {
        .boot => "boot",
        .kernel => "kernel",
        .kernel_heap => "kernel_heap",
        .page_table => "page_table",
        .virtual_range => "virtual_range",
        .program_image => "program_image",
        .app_heap => "app_heap",
        .app_stack => "app_stack",
        .dma => "dma",
        .mmio => "mmio",
        .framebuffer => "framebuffer",
        .reserved => "reserved",
        .free => "free",
        .unknown => "unknown",
        .unavailable => "unavailable",
    };
}

pub fn memoryStatusName(status: MemoryStatus) []const u8 {
    return switch (status) {
        .free => "free",
        .reserved => "reserved",
        .committed => "committed",
        .guard => "guard",
        .mapped => "mapped",
        .released => "released",
        .@"error" => "error",
        .unknown => "unknown",
        .unavailable => "unavailable",
    };
}

pub fn stopCodeForCause(cause: Cause) u32 {
    return switch (cause) {
        .cpu_exception => 0x0001_0000,
        .page_fault => 0x0001_000e,
        .double_fault => 0x0001_0008,
        .machine_check => 0x0001_0012,
        .kernel_fatal => 0x0002_0001,
        .zig_panic => 0x0002_0002,
        .manual_crash_test => 0x000f_0001,
    };
}

pub fn stopCodeForCpuVector(vector: u64) u32 {
    const low: u32 = @intCast(vector & 0xffff);
    return switch (vector) {
        8 => stopCodeForCause(.double_fault),
        14 => stopCodeForCause(.page_fault),
        18 => stopCodeForCause(.machine_check),
        else => 0x0001_0000 | low,
    };
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
