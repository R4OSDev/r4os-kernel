// Early CPU and interrupt foundation for kernel startup.
//
// This layer initializes the x86_64 foundation that must be ready before most
// later kernel subsystems: GDT, IDT, and the initial PIC state.

const gdt = @import("../arch/x86_64/gdt.zig");
const idt = @import("../arch/x86_64/idt.zig");
const fpu = @import("../arch/x86_64/fpu.zig");
const cpu = @import("../platform/cpu.zig");
const monotonic = @import("../platform/monotonic.zig");
const fatal = @import("fatal.zig");
const pic = @import("../arch/x86_64/pic.zig");
const smp = @import("smp.zig");

// Native BSP/AP setup no longer calls the Zig IDT facade. Keep its remaining
// exception/IRQ owner callbacks linked until those foreign owners migrate.
comptime {
    _ = idt;
}

// Transitional facade; Rust owns this boot cut and its readiness state.
extern fn r4os_rust_cpu_boot() callconv(.c) void;
extern fn r4os_rust_cpu_ready() callconv(.c) bool;
pub fn init() void {
    r4os_rust_cpu_boot();
}
pub fn isInitialized() bool {
    return r4os_rust_cpu_ready();
}
// These owners migrate in clock/CPU/SMP waves 0.83.9–10.
export fn r4os_zig_cpu_foundation() callconv(.c) void {
    pic.init();
    _ = cpu.detect();
    smp.initBsp();
    monotonic.configureCpuClock();
    if (!fpu.init()) fatal.kernelFatal(.cpu, "FPU/SSE task-state init failed");
}
