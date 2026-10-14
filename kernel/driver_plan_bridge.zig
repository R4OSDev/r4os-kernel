// Stateless cuts to current Config, R4D loader, input/audio/display owners.
const boot_config = @import("boot_config.zig");
const audio_boot = @import("audio_boot.zig");
const bootlog = @import("bootlog.zig");
const bootscreen = @import("bootscreen.zig");
const keyboard = @import("../driver/input/keyboard.zig");
const mouse = @import("../driver/input/mouse.zig");
const registry = @import("../driver/registry.zig");
const r4d = @import("../program/r4d.zig");
const Span = extern struct { pointer: [*]const u8, len: usize };
export fn r4os_zig_driver_plan_config(opaque_config: *const anyopaque, operation: u8, index: usize, out: ?*Span) callconv(.c) u64 {
    const config: *const boot_config.Config = @ptrCast(@alignCast(opaque_config));
    return switch (operation) {
        0 => config.driver_count,
        1 => config.disabled_count,
        2 => @intFromBool(config.auto_acpi),
        3 => @intFromBool(config.auto_pci),
        4, 5 => blk: {
            const name = if (operation == 4) boot_config.driverName(index) else boot_config.disabledName(index);
            out.?.* = .{ .pointer = name.ptr, .len = name.len };
            break :blk 0;
        },
        else => unreachable,
    };
}
export fn r4os_zig_driver_plan_disabled(opaque_config: *const anyopaque, name: [*]const u8, len: usize) callconv(.c) bool {
    const config: *const boot_config.Config = @ptrCast(@alignCast(opaque_config));
    return boot_config.isDisabled(config, name[0..len]);
}
export fn r4os_zig_driver_plan_load(name: [*]const u8, len: usize) callconv(.c) u8 {
    return @intFromEnum(r4d.loadRuntimeNameResult(name[0..len]));
}
export fn r4os_zig_driver_plan_effect(operation: u8, pointer: [*]const u8, len: usize, value: u32) callconv(.c) void {
    const name = pointer[0..len];
    switch (operation) {
        0 => bootlog.puts("[PLAN] build\r\n"),
        1 => bootlog.puts("[PLAN] AUTO=ACPI enabled\r\n"),
        2 => bootlog.puts("[PLAN] AUTO=PCI enabled\r\n"),
        3 => {
            bootlog.puts("[PLAN][WARN] skip ");
            bootlog.puts(name);
            bootlog.puts(" plan capacity reached\r\n");
        },
        4 => {
            bootlog.puts("[PLAN] skip ");
            bootlog.puts(name);
            bootlog.puts(" disabled by CONFIG.R4S\r\n");
        },
        5 => {
            bootlog.puts("[PLAN] skip ");
            bootlog.puts(name);
            bootlog.puts(" already planned\r\n");
        },
        6 => {
            bootlog.puts("[PLAN] drivers selected=");
            bootlog.putDec(value);
            bootlog.puts("\r\n");
        },
        7 => {
            bootscreen.setDriver(name);
            bootlog.puts("[PLAN] load ");
            bootlog.puts(name);
            bootlog.puts("\r\n");
        },
        8 => {
            bootlog.puts("[PLAN] ");
            bootlog.puts(name);
            bootlog.puts(" already ");
            bootlog.puts(registry.sourceName(@enumFromInt(value & 255)));
            bootlog.puts("/");
            bootlog.puts(registry.stateName(@enumFromInt(value >> 8)));
            bootlog.puts("\r\n");
        },
        9 => {
            const result: r4d.RuntimeLoadResult = @enumFromInt(value);
            switch (result) {
                .name_invalid, .file_missing, .invalid_file, .load_failed => bootscreen.setDriverError(name),
                else => {},
            }
            bootlog.puts("[PLAN][WARN] load failed ");
            bootlog.puts(name);
            bootlog.puts(" result=");
            bootlog.puts(r4d.runtimeLoadResultName(result));
            bootlog.puts("\r\n");
        },
        10, 11 => {
            const result: audio_boot.R4dPlanResult = @enumFromInt(value);
            if (operation == 10) audio_boot.reportAc97R4dPlan(result) else audio_boot.reportHdaR4dPlan(result);
        },
        12 => bootlog.puts("[PLAN] disable PS2 core input\r\n"),
        13 => {
            registry.setState(value, .shutdown);
            keyboard.disable();
            mouse.disable();
            registry.markUnloaded(value);
        },
        14 => bootlog.puts("[PLAN][WARN] PS2 registry entry missing\r\n"),
        15 => bootlog.puts("[PLAN][WARN] DISABLE=ATAPIO ignored after boot-storage init\r\n"),
        else => unreachable,
    }
}
comptime {
    if (boot_config.MAX_DRIVERS != 12) @compileError("Native driver plan must keep exact Config capacity");
}
