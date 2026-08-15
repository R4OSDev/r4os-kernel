// Early CPU and interrupt foundation for kernel startup.
//
// This layer initializes the x86_64 foundation that must be ready before most
// later kernel subsystems: GDT, IDT, and the initial PIC state.

const gdt = @import("../arch/x86_64/gdt.zig");
const idt = @import("../arch/x86_64/idt.zig");
const fpu = @import("../arch/x86_64/fpu.zig");
const cpu = @import("../platform/cpu.zig");
const fatal = @import("fatal.zig");
const pic = @import("../arch/x86_64/pic.zig");

var initialized = false;

pub fn init() void {
    if (initialized) return;

    gdt.init();
    idt.init();
    pic.init();
    _ = cpu.detect();
    if (!fpu.init()) fatal.kernelFatal(.cpu, "FPU/SSE task-state init failed");

    initialized = true;
}

pub fn isInitialized() bool {
    return initialized;
}
