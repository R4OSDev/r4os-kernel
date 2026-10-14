// Private C transport for the native Rust SMP owner; stateless presentation and explicit diagnostic admission.
//
// APs are started sequentially through private low-memory INIT/SIPI
// trampolines.  They publish `parked` only after CPU-local GDT/TSS/GS, IDT,
// FPU and scheduler state are ready.  The BSP never waits without a deadline,
// and failed APs are excluded from the schedulable mask permanently.

const std = @import("std");
const acpi = @import("../platform/acpi.zig");
const boot_config = @import("boot_config.zig");
const percpu = @import("../arch/x86_64/percpu.zig");
const policy = @import("smp_policy.zig");
const k = @import("log.zig");

pub const Status = extern struct {
    initialized: bool = false,
    activated: bool = false,
    discovered: u32 = 1,
    started: u32 = 0,
    online: u32 = 1,
    failed: u32 = 0,
    duplicates: u32 = 0,
    disabled: u32 = 0,
    startup_timeouts: u32 = 0,
    stop_timeouts: u32 = 0,
};

extern fn r4os_rust_smp_control(operation: u8, index: u32) u64;
extern fn r4os_rust_smp_start(candidates: [*]const policy.Candidate, len: usize) bool;
extern fn r4os_rust_smp_status(output: *Status) void;
comptime {
    if (@sizeOf(Status) != 36 or @offsetOf(Status, "discovered") != 4)
        @compileError("Native SMP C layout differs");
}
pub fn initBsp() void {
    _ = r4os_rust_smp_control(0, 0);
}
pub fn reserveLowMemory() void {
    _ = r4os_rust_smp_control(1, 0);
}
pub fn startApplicationProcessors(info: acpi.Info) bool {
    var candidates: [acpi.MAX_MADT_CPUS]policy.Candidate = .{policy.Candidate{}} ** acpi.MAX_MADT_CPUS;
    const count: usize = @intCast(@min(info.madt_lapic_count, acpi.MAX_MADT_CPUS));
    for (0..count) |index| {
        const entry = info.madt_cpu_entries[index];
        candidates[index] = .{ .apic_id = entry.apic_id, .enabled = entry.enabled(), .online_capable = entry.onlineCapable() };
    }
    return r4os_rust_smp_start(&candidates, count);
}
pub fn activate() void {
    _ = r4os_rust_smp_control(2, 0);
}
pub fn stopOthers() void {
    _ = r4os_rust_smp_control(3, 0);
}
pub fn sendReschedule(index: u32) void {
    _ = r4os_rust_smp_control(4, index);
}
pub fn irqTarget(ordinal: u32) u32 {
    return @intCast(r4os_rust_smp_control(5, ordinal));
}
pub fn capacity() struct { available: u32, configured: u32 } {
    return .{ .available = percpu.schedulableCount(), .configured = @intCast(r4os_rust_smp_control(6, 0)) };
}
pub fn status() Status {
    var result: Status = undefined;
    r4os_rust_smp_status(&result);
    return result;
}
export fn r4os_zig_smp_log(event: u8, index: u32, value: u64, text: [*]const u8, len: usize, st: *const Status) callconv(.c) void {
    switch (event) {
        0 => {
            k.puts("[SMP] low-memory trampolines reserved=");
            k.putDec(value);
        },
        1, 2 => {
            k.puts("[SMP] ap=");
            k.putDec(index);
            k.puts(" apic=");
            k.putDec(percpu.apicId(index) orelse 0);
            k.puts(" failed=");
            k.puts(text[0..len]);
            if (event == 2) {
                k.puts(" progress=");
                k.putDec(value);
            }
        },
        3 => {
            k.puts("[SMP] stage=");
            k.puts(text[0..len]);
            k.puts(" discovered=");
            k.putDec(st.discovered);
            k.puts(" started=");
            k.putDec(st.started);
            k.puts(" online=");
            k.putDec(st.online);
            k.puts(" failed=");
            k.putDec(st.failed);
            k.puts(" fallback=");
            k.puts(if (st.online == 1) "1cpu" else "no");
        },
        4 => {
            k.puts("[SMP] trampoline invalid size=");
            k.putDec(value);
        },
        5 => k.puts("[SMP] trampoline low-memory reservation missing"),
        6 => k.puts("[SMP] trampoline identity mapping failed"),
        else => unreachable,
    }
    k.puts("\r\n");
}

comptime {
    _ = @import("smp_probe_bridge.zig");
}
pub fn acceptanceProbeEnabled() bool {
    const value = boot_config.optionValue(boot_config.get(), "SMP", "selftest") orelse return false;
    return std.ascii.eqlIgnoreCase(value, "yes");
}

extern fn r4os_rust_smp_probe(usable: u64) bool;
pub fn runAcceptanceProbeIfEnabled(usable: u64) bool {
    return if (acceptanceProbeEnabled()) r4os_rust_smp_probe(usable) else true;
}
