const gdt = @import("gdt.zig");
const interrupts = @import("interrupts.zig");
const ioapic = @import("ioapic.zig");
const lapic = @import("lapic.zig");
const percpu = @import("percpu.zig");
const pic = @import("pic.zig");
const irq_router = @import("../../kernel/irq_router.zig");
const scheduler = @import("../../sched/scheduler.zig");
const timer = @import("../../kernel/timer.zig");
const tss = @import("tss.zig");
const fpu = @import("fpu.zig");
const tlb = @import("tlb_shootdown.zig");
const keyboard = @import("../../driver/input/keyboard.zig");
const mouse = @import("../../driver/input/mouse.zig");
const ps2_controller = @import("../../driver/input/i8042.zig");
const k = @import("../../kernel/log.zig");
const crash = @import("../../kernel/crash.zig");
const crash_screen = @import("../../kernel/crash_screen.zig");
const mem_blocks = @import("../../memory/blocks.zig");
const r4x = @import("../../program/r4x.zig");

const IDT_ENTRIES = 256;
const INTERRUPT_GATE: u8 = 0x8E;
pub const RESCHEDULE_VECTOR: u8 = 0xF0;
pub const STOP_VECTOR: u8 = 0xF1;
pub const TLB_VECTOR: u8 = tlb.VECTOR;

pub const InterruptFrame = extern struct {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    vector: u64,
    error_code: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
};

// Rust owns IDT storage, assembly entry functions and IST frame relocation.
extern fn r4os_rust_idt_init() callconv(.c) void;
extern fn r4os_rust_idt_load() callconv(.c) void;
extern fn r4os_rust_interrupted_rsp(*const InterruptFrame) callconv(.c) u64;
comptime {
    if (@sizeOf(InterruptFrame) != 160 or @offsetOf(InterruptFrame, "vector") != 120 or
        @offsetOf(InterruptFrame, "error_code") != 128 or @offsetOf(InterruptFrame, "rip") != 136 or
        @offsetOf(InterruptFrame, "rflags") != 152) @compileError("Rust/assembly interrupt frame differs");
    if (fpu.state_storage_bytes != 4096 or fpu.state_storage_align != 64 or TLB_VECTOR != 0xF2 or
        lapic.SPURIOUS_VECTOR != 0xFF or pic.MASTER_OFFSET != 32) @compileError("Rust exception foundation differs");
}
pub fn init() void {
    r4os_rust_idt_init();
    k.puts("  IDT loaded [OK]\r\n");
}
pub fn loadCurrent() void {
    r4os_rust_idt_load();
}

// IRQ runtime remains with its scheduler/device owners until 0.83.9/11.
export fn r4os_zig_irq_dispatch(frame: *const InterruptFrame) callconv(.c) void {
    const irq_flags = interrupts.saveAndDisableRuntime();
    defer interrupts.restore(irq_flags);
    const vector = frame.vector;
    if (vector < pic.MASTER_OFFSET or vector >= pic.MASTER_OFFSET + 32) return;

    const irq: u8 = @intCast(vector - pic.MASTER_OFFSET);
    var request_preempt = false;
    const preemptible_instruction_pointer = r4x.isPreemptibleInstructionPointer(frame.rip);
    if (percpu.currentIndex() != 0 and irq == timer.PIT_IRQ) {
        request_preempt = scheduler.onSecondaryTick(timer.tickCount(), preemptible_instruction_pointer);
        lapic.endOfInterrupt();
        if (request_preempt) scheduler.preemptFromIrq();
        return;
    }
    var wake_request_checkpoint = scheduler.structureStats().wakeup_reschedule_requests;
    switch (irq) {
        timer.PIT_IRQ => {
            request_preempt = scheduler.onTick(timer.onIrq(), preemptible_instruction_pointer);
            // Timer wakeups were considered by onTick. Only a new wake from a
            // subsequently dispatched device handler needs another decision.
            wake_request_checkpoint = scheduler.structureStats().wakeup_reschedule_requests;
        },
        keyboard.IRQ, mouse.IRQ => ps2_controller.onIrq(irq),
        else => {},
    }
    irq_router.dispatch(irq);
    if (!ioapic.isRoutingActive() or irq < 16) {
        pic.endOfInterrupt(irq);
    }
    lapic.endOfInterrupt();
    if (!request_preempt and
        (irq != timer.PIT_IRQ or
            scheduler.structureStats().wakeup_reschedule_requests != wake_request_checkpoint))
    {
        request_preempt = scheduler.preemptPendingWake(preemptible_instruction_pointer);
    }
    if (request_preempt) scheduler.preemptFromIrq();
}

export fn r4os_zig_ipi_dispatch(frame: *const InterruptFrame) callconv(.c) void {
    switch (frame.vector) {
        RESCHEDULE_VECTOR => {
            lapic.endOfInterrupt();
            scheduler.onRescheduleIpi(r4x.isPreemptibleInstructionPointer(frame.rip));
        },
        STOP_VECTOR => {
            lapic.endOfInterrupt();
            const index = percpu.currentIndex();
            _ = percpu.transition(index, .stopping, .offline);
            interrupts.haltForever();
        },
        TLB_VECTOR => {
            tlb.handleIpi();
            lapic.endOfInterrupt();
        },
        else => lapic.endOfInterrupt(),
    }
}

export fn r4os_zig_prepare_fault_stack(addr: u64, rsp: u64, bytes: usize) callconv(.c) usize {
    return r4x.preparePageFaultStack(addr, rsp, bytes) orelse 0;
}
export fn r4os_zig_fault_fpu_save(state: [*]u8) callconv(.c) void {
    fpu.saveTaskState(state[0..4096]);
}
export fn r4os_zig_fault_fpu_restore(state: [*]const u8) callconv(.c) void {
    fpu.restoreTaskState(state[0..4096]);
}
export fn r4os_zig_exception_inner(frame: *const InterruptFrame, addr: u64, private: bool) callconv(.c) void {
    exceptionDispatchInner(frame, addr, private);
}

fn exceptionDispatchInner(frame: *const InterruptFrame, page_fault_addr: u64, private_stack: bool) void {
    if (frame.vector == 14 and r4x.handlePageFault(page_fault_addr, frame.error_code, private_stack)) return;

    k.puts("[SMP] exception cpu=");
    k.putDec(percpu.currentIndex());
    if (scheduler.current()) |current_task| {
        k.puts(" task=");
        k.putDec(current_task.id);
        k.puts("/");
        k.puts(current_task.name);
        k.puts(" state_raw=");
        const state_raw: *const u8 = @ptrCast(&current_task.state);
        k.putDec(state_raw.*);
        k.puts(" running_cpu=");
        k.putDec(current_task.running_cpu);
        k.puts(" saved_rsp=0x");
        k.putHex(current_task.rsp, 16);
        k.puts(" stack=0x");
        k.putHex(current_task.stack_base, 16);
        k.puts("..0x");
        k.putHex(current_task.stack_top, 16);
    }
    const fault_rsp = interruptedRsp(frame);
    k.puts(" rip=0x");
    k.putHex(frame.rip, 16);
    k.puts(" interrupted_rsp=0x");
    k.putHex(fault_rsp, 16);
    k.puts("\r\n");

    const entry = crash.enterCrashPath();
    var report = crash.fromCpuException(.{
        .frame = crashFrameFromInterruptFrame(frame),
        .cr2 = page_fault_addr,
        .boot_phase = .unknown,
        .ticks = timer.tickCount(),
        .memory = faultMemoryForException(frame.vector, page_fault_addr),
        .context = r4x.crashContextForInstructionPointer(frame.rip),
        .message = if (entry == .reentrant)
            "Reentrant CPU exception while handling crash"
        else
            "Unhandled CPU exception",
    });

    if (entry == .reentrant) {
        crash_screen.serialMirror(&report);
    } else {
        _ = crash_screen.render(&report);
    }

    halt();
}

fn crashFrameFromInterruptFrame(frame: *const InterruptFrame) crash.CpuFrameSnapshot {
    return .{
        .registers = .{
            .r15 = frame.r15,
            .r14 = frame.r14,
            .r13 = frame.r13,
            .r12 = frame.r12,
            .r11 = frame.r11,
            .r10 = frame.r10,
            .r9 = frame.r9,
            .r8 = frame.r8,
            .rsi = frame.rsi,
            .rdi = frame.rdi,
            .rbp = frame.rbp,
            .rdx = frame.rdx,
            .rcx = frame.rcx,
            .rbx = frame.rbx,
            .rax = frame.rax,
        },
        .vector = frame.vector,
        .error_code = frame.error_code,
        .rip = frame.rip,
        .rsp = interruptedRsp(frame),
        .cs = frame.cs,
        .rflags = frame.rflags,
    };
}

fn interruptedRsp(frame: *const InterruptFrame) u64 {
    return r4os_rust_interrupted_rsp(frame);
}

fn faultMemoryForException(vector: u64, addr: u64) crash.FaultMemoryInfo {
    if (vector != 14) return crash.unavailableMemory();
    if (mem_blocks.firstContainingVirtual(addr)) |block| {
        return crash.faultMemoryFromBlock(block);
    }
    return crash.untrackedMemory();
}

fn halt() noreturn {
    interrupts.haltForever();
}
