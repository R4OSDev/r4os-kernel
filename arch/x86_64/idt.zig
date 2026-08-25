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

const DescriptorTablePointer = packed struct {
    limit: u16,
    base: u64,
};

const IdtEntry = packed struct {
    offset_low: u16 = 0,
    selector: u16 = 0,
    ist: u8 = 0,
    type_attr: u8 = 0,
    offset_mid: u16 = 0,
    offset_high: u32 = 0,
    zero: u32 = 0,

    fn set(self: *IdtEntry, handler: *const anyopaque, selector: u16, type_attr: u8, ist: u8) void {
        const addr: u64 = @intFromPtr(handler);
        self.offset_low = @truncate(addr);
        self.selector = selector;
        self.ist = ist & 0x7;
        self.type_attr = type_attr;
        self.offset_mid = @truncate(addr >> 16);
        self.offset_high = @truncate(addr >> 32);
        self.zero = 0;
    }
};

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

extern fn r4os_load_idt(idtr: *const DescriptorTablePointer) callconv(.c) void;

extern fn isr0() callconv(.c) void;
extern fn isr1() callconv(.c) void;
extern fn isr2() callconv(.c) void;
extern fn isr3() callconv(.c) void;
extern fn isr4() callconv(.c) void;
extern fn isr5() callconv(.c) void;
extern fn isr6() callconv(.c) void;
extern fn isr7() callconv(.c) void;
extern fn isr8() callconv(.c) void;
extern fn isr9() callconv(.c) void;
extern fn isr10() callconv(.c) void;
extern fn isr11() callconv(.c) void;
extern fn isr12() callconv(.c) void;
extern fn isr13() callconv(.c) void;
extern fn isr14() callconv(.c) void;
extern fn isr15() callconv(.c) void;
extern fn isr16() callconv(.c) void;
extern fn isr17() callconv(.c) void;
extern fn isr18() callconv(.c) void;
extern fn isr19() callconv(.c) void;
extern fn isr20() callconv(.c) void;
extern fn isr21() callconv(.c) void;
extern fn isr22() callconv(.c) void;
extern fn isr23() callconv(.c) void;
extern fn isr24() callconv(.c) void;
extern fn isr25() callconv(.c) void;
extern fn isr26() callconv(.c) void;
extern fn isr27() callconv(.c) void;
extern fn isr28() callconv(.c) void;
extern fn isr29() callconv(.c) void;
extern fn isr30() callconv(.c) void;
extern fn isr31() callconv(.c) void;
extern fn irq0() callconv(.c) void;
extern fn irq1() callconv(.c) void;
extern fn irq2() callconv(.c) void;
extern fn irq3() callconv(.c) void;
extern fn irq4() callconv(.c) void;
extern fn irq5() callconv(.c) void;
extern fn irq6() callconv(.c) void;
extern fn irq7() callconv(.c) void;
extern fn irq8() callconv(.c) void;
extern fn irq9() callconv(.c) void;
extern fn irq10() callconv(.c) void;
extern fn irq11() callconv(.c) void;
extern fn irq12() callconv(.c) void;
extern fn irq13() callconv(.c) void;
extern fn irq14() callconv(.c) void;
extern fn irq15() callconv(.c) void;
extern fn irq16() callconv(.c) void;
extern fn irq17() callconv(.c) void;
extern fn irq18() callconv(.c) void;
extern fn irq19() callconv(.c) void;
extern fn irq20() callconv(.c) void;
extern fn irq21() callconv(.c) void;
extern fn irq22() callconv(.c) void;
extern fn irq23() callconv(.c) void;
extern fn irq24() callconv(.c) void;
extern fn irq25() callconv(.c) void;
extern fn irq26() callconv(.c) void;
extern fn irq27() callconv(.c) void;
extern fn irq28() callconv(.c) void;
extern fn irq29() callconv(.c) void;
extern fn irq30() callconv(.c) void;
extern fn irq31() callconv(.c) void;
extern fn ipi_reschedule() callconv(.c) void;
extern fn ipi_stop() callconv(.c) void;

var idt: [IDT_ENTRIES]IdtEntry align(16) = .{IdtEntry{}} ** IDT_ENTRIES;

const handlers = [_]*const anyopaque{
    @ptrCast(&isr0),  @ptrCast(&isr1),  @ptrCast(&isr2),  @ptrCast(&isr3),
    @ptrCast(&isr4),  @ptrCast(&isr5),  @ptrCast(&isr6),  @ptrCast(&isr7),
    @ptrCast(&isr8),  @ptrCast(&isr9),  @ptrCast(&isr10), @ptrCast(&isr11),
    @ptrCast(&isr12), @ptrCast(&isr13), @ptrCast(&isr14), @ptrCast(&isr15),
    @ptrCast(&isr16), @ptrCast(&isr17), @ptrCast(&isr18), @ptrCast(&isr19),
    @ptrCast(&isr20), @ptrCast(&isr21), @ptrCast(&isr22), @ptrCast(&isr23),
    @ptrCast(&isr24), @ptrCast(&isr25), @ptrCast(&isr26), @ptrCast(&isr27),
    @ptrCast(&isr28), @ptrCast(&isr29), @ptrCast(&isr30), @ptrCast(&isr31),
};

const irq_handlers = [_]*const anyopaque{
    @ptrCast(&irq0),  @ptrCast(&irq1),  @ptrCast(&irq2),  @ptrCast(&irq3),
    @ptrCast(&irq4),  @ptrCast(&irq5),  @ptrCast(&irq6),  @ptrCast(&irq7),
    @ptrCast(&irq8),  @ptrCast(&irq9),  @ptrCast(&irq10), @ptrCast(&irq11),
    @ptrCast(&irq12), @ptrCast(&irq13), @ptrCast(&irq14), @ptrCast(&irq15),
    @ptrCast(&irq16), @ptrCast(&irq17), @ptrCast(&irq18), @ptrCast(&irq19),
    @ptrCast(&irq20), @ptrCast(&irq21), @ptrCast(&irq22), @ptrCast(&irq23),
    @ptrCast(&irq24), @ptrCast(&irq25), @ptrCast(&irq26), @ptrCast(&irq27),
    @ptrCast(&irq28), @ptrCast(&irq29), @ptrCast(&irq30), @ptrCast(&irq31),
};

pub fn init() void {
    for (handlers, 0..) |handler, vector| {
        idt[vector].set(handler, gdt.codeSelector(), INTERRUPT_GATE, istForVector(vector));
    }
    for (irq_handlers, 0..) |handler, irq| {
        idt[pic.MASTER_OFFSET + irq].set(handler, gdt.codeSelector(), INTERRUPT_GATE, 0);
    }

    idt[RESCHEDULE_VECTOR].set(@ptrCast(&ipi_reschedule), gdt.codeSelector(), INTERRUPT_GATE, 0);
    idt[STOP_VECTOR].set(@ptrCast(&ipi_stop), gdt.codeSelector(), INTERRUPT_GATE, 0);

    loadCurrent();
    k.puts("  IDT loaded ");
    k.puts("[OK]\r\n");
}

pub fn loadCurrent() void {
    const idtr = DescriptorTablePointer{
        .limit = @sizeOf(@TypeOf(idt)) - 1,
        .base = @intFromPtr(&idt),
    };
    r4os_load_idt(&idtr);
}

pub export fn irqDispatch(frame: *const InterruptFrame) callconv(.c) void {
    const irq_flags = interrupts.saveAndDisable();
    defer interrupts.restore(irq_flags);
    const vector = frame.vector;
    if (vector < pic.MASTER_OFFSET or vector >= pic.MASTER_OFFSET + irq_handlers.len) return;

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

pub export fn ipiDispatch(frame: *const InterruptFrame) callconv(.c) void {
    switch (frame.vector) {
        RESCHEDULE_VECTOR => {
            lapic.endOfInterrupt();
            scheduler.onRescheduleIpi(r4x.isPreemptibleInstructionPointer(frame.rip));
        },
        STOP_VECTOR => {
            lapic.endOfInterrupt();
            const index = percpu.currentIndex();
            percpu.setSchedulable(index, false);
            _ = percpu.setState(index, .offline);
            interrupts.haltForever();
        },
        else => lapic.endOfInterrupt(),
    }
}

fn istForVector(vector: usize) u8 {
    return switch (vector) {
        2, 8, 18 => tss.DOUBLE_FAULT_IST,
        10, 11, 12, 13, 14, 17, 21, 29, 30 => tss.FAULT_IST,
        else => 0,
    };
}

pub export fn exceptionDispatch(frame: *const InterruptFrame) callconv(.c) void {
    const page_fault_addr = if (frame.vector == 14) readCr2() else 0;
    if (frame.vector == 14 and r4x.handlePageFault(page_fault_addr, frame.error_code)) return;

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

fn readCr2() u64 {
    return asm volatile ("mov %%cr2, %[ret]"
        : [ret] "=r" (-> u64),
    );
}

fn interruptedRsp(frame: *const InterruptFrame) u64 {
    const after_rflags = @intFromPtr(&frame.rip) + 24;
    const vector: usize = @intCast(frame.vector);
    if (vector < IDT_ENTRIES and istForVector(vector) != 0) {
        const saved_rsp: *const u64 = @ptrFromInt(after_rflags);
        return saved_rsp.*;
    }
    return after_rflags;
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
