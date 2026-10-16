// R4OS Kernel - Einstieg.

const std = @import("std");
const limine = @import("bootloader/limine.zig");
const boot_info = @import("bootloader/boot_info.zig");
const interrupts = @import("arch/x86_64/interrupts.zig");
const config = @import("config");
const crash = @import("kernel/crash.zig");
const crash_screen = @import("kernel/crash_screen.zig");
const fatal = @import("kernel/fatal.zig");
const log = @import("kernel/log.zig");
const boot_intro = @import("kernel/boot_intro.zig");
const com_debug_boot = @import("kernel/com_debug_boot.zig");
const cpu_boot = @import("kernel/cpu_boot.zig");
const timer_boot = @import("kernel/timer_boot.zig");
const driver_boot = @import("kernel/driver_boot.zig");
const driver_api = @import("kernel/driver_api.zig");
const input_boot = @import("kernel/input_boot.zig");
const memory_boot = @import("kernel/memory_boot.zig");
const storage_boot = @import("kernel/storage_boot.zig");
const module_boot = @import("kernel/module_boot.zig");
const usb_protocol_preload_boot = @import("kernel/usb_protocol_preload_boot.zig");
const service_boot = @import("kernel/service_boot.zig");
const platform_boot = @import("kernel/platform_boot.zig");
const loader_boot = @import("kernel/loader_boot.zig");
const platform_irq_boot = @import("kernel/platform_irq_boot.zig");
const audio_boot = @import("kernel/audio_boot.zig");
const network_boot = @import("kernel/network_boot.zig");
const usb_hid_boot = @import("kernel/usb_hid_boot.zig");
const driver_policy_boot = @import("kernel/driver_policy_boot.zig");
const runtime_boot = @import("kernel/runtime_boot.zig");
const boot_status = @import("kernel/boot_status.zig");
const net_core = @import("net/core.zig");
const sched_sync = @import("sched/sync.zig");
const sched_task = @import("sched/task.zig");
const sched_scheduler = @import("sched/scheduler.zig");
const task_registry_selftest = @import("sched/task_registry_selftest.zig");
const block_storage = @import("storage/block.zig");
const bootscreen = @import("kernel/bootscreen.zig");
const boot_display = @import("display/boot_display.zig");
const kernel_version = @import("kernel/version.zig");
const smp = @import("kernel/smp.zig");
const rust_bridge = @import("kernel/rust_bridge.zig");

pub const panic = std.debug.FullPanic(handleZigPanic);

// Private boot cuts for the Rust entry owner. No native Zig value crosses
// this boundary; each future migration wave replaces its own cut in Rust.
export fn r4os_zig_boot_action(action: u32) callconv(.c) bool {
    switch (action) {
        0 => kernel_version.keepMetadata(),
        1 => {
            crash.init();
            fatal.init();
            fatal.setBootPhase(.entry);
            com_debug_boot.init();
        },
        2 => {
            _ = boot_info.init();
            _ = boot_display.init();
            boot_status.beginBootLogRedirect();
        },
        3 => {
            rust_bridge.init();
            if (config.enable_kernel_fatal_test) fatal.kernelFatal(.entry, "Kernel fatal crash test");
            if (config.enable_zig_panic_test) @panic("Zig panic crash test");
            if (config.enable_crash_screen_test) {
                // The existing single renderer fixture also retains an exact
                // diagnostic generation across presentation handoff and seals
                // the final incident before its fatal-screen replay.
                const diag = @import("kernel/diag_screen.zig");
                const first = diag.beginResolvableIncident();
                diag.line("Resolved diagnostic predecessor.");
                diag.resetForExternalDisplay();
                if (!first.valid() or !diag.resolveIncident(first))
                    fatal.kernelFatal(.entry, "Diagnostic handoff token mismatch");
                const retained = diag.beginResolvableIncident();
                diag.line("Retained diagnostic incident after handoff.");
                diag.beginIncident();
                if (!retained.valid() or diag.resolveIncident(first) or diag.resolveIncident(retained))
                    fatal.kernelFatal(.entry, "Diagnostic incident seal mismatch");
                const report = crash.fromCpuException(.{
                    .frame = .{
                        .registers = .{ .rax = 1, .rbx = 2, .rcx = 3, .rdx = 4, .rbp = 0xffff_ffff_8000_1000 },
                        .vector = 14,
                        .error_code = 0b10111,
                        .rip = 0xffff_ffff_8000_2000,
                        .rsp = 0xffff_ffff_8000_3000,
                        .cs = 8,
                        .rflags = 0x202,
                    },
                    .cr2 = 0xffff_ffff_dead_beef,
                    .boot_phase = .entry,
                    .ticks = 0,
                    .memory = crash.untrackedMemory(),
                    .message = "Crash screen renderer test",
                });
                _ = crash_screen.render(&report);
                log.serialWriteRaw("[CRASH-RENDER] completed\r\n");
                interrupts.haltForever();
            }
        },
        4 => cpu_boot.init(),
        5 => timer_boot.init(),
        6 => driver_boot.init(),
        7 => {
            input_boot.initKeyboard();
            input_boot.initMouse();
            input_boot.completePs2();
        },
        8 => return boot_intro.init(),
        9 => return memory_boot.initCore(),
        10 => memory_boot.dumpBlockSummary(),
        11 => return storage_boot.initFoundation(),
        12 => module_boot.init(),
        13 => return platform_boot.initDeviceMappings(),
        14 => return usb_protocol_preload_boot.init(),
        15 => service_boot.init(),
        16 => {
            fatal.setBootPhase(.platform);
            _ = platform_boot.pcieStatus() orelse fatal.kernelFatal(.platform, "PCIe status missing after platform boot");
        },
        17 => return storage_boot.initControllers(platform_boot.pcieStatus().?),
        20 => return loader_boot.initFilesystemLoader(),
        21 => return platform_irq_boot.init(),
        22 => return audio_boot.init(),
        23 => return network_boot.init(),
        24 => return usb_hid_boot.init(),
        25 => return runtime_boot.initTaskRuntime(),
        26 => return driver_policy_boot.init(),
        27 => {
            const quick_acceptance = smp.acceptanceProbeEnabled();
            _ = smp.runAcceptanceProbeIfEnabled(memory_boot.usableBytes());
            if (quick_acceptance) {
                _ = runNvmeInterruptAcceptanceProbe();
                if (net_core.runTcpPerformanceContractProbe()) {
                    log.puts("[TCPBURSTPROBE] result=OK write_bytes=4068 segments=3 catalog=48 delayed_ack_ms=40\r\n");
                } else {
                    log.puts("[TCPBURSTPROBE] result=FAILED\r\n");
                    fatal.kernelFatal(.runtime, "TCP burst probe failed");
                }
                log.puts("[QUICKPROBE] result=DONE\r\n");
            }
        },
        28 => {
            _ = net_core.startRxTask();
        },
        29 => {
            _ = net_core.startDhcpTask();
        },
        30 => {
            if (comptime config.enable_boot_selftests) {
                return sched_sync.selfTest();
            }
            return true;
        },
        31 => {
            if (comptime config.enable_boot_selftests or config.enable_block_dispatch_selftest) {
                return block_storage.parallelDispatchSelfTest();
            }
            return true;
        },
        32 => {
            _ = usb_hid_boot.startPollTask();
        },
        33 => {
            if (comptime config.enable_boot_selftests) {
                return sched_scheduler.prioritySelfTest();
            }
            return true;
        },
        34 => boot_status.statusLine(if (comptime config.enable_boot_selftests)
            "BOOTSELFTEST OK heap=1 page_tables=1 sync=1 priority=1\r\n"
        else
            "BOOTSELFTEST OFF heap=0 page_tables=0 sync=0 priority=0\r\n"),
        35 => {
            _ = task_registry_selftest.runIfEnabled();
        },
        36 => {
            if (comptime config.enable_stack_guard_test) {
                _ = sched_task.createKernelThread("st-overflow", stackGuardTestMain);
            }
        },
        else => fatal.kernelFatal(.entry, "Invalid private boot cut"),
    }
    return true;
}

export fn r4os_zig_boot_step(phase: u8, progress: u8, text: [*]const u8, len: usize) callconv(.c) void {
    fatal.setBootPhase(@enumFromInt(phase));
    beginBootStep(@enumFromInt(progress), text[0..len]);
}
export fn r4os_zig_boot_status(text: [*]const u8, len: usize) callconv(.c) void {
    bootscreen.setStatus(text[0..len]);
}
export fn r4os_zig_boot_require(ok: bool, phase: u8, text: [*]const u8, len: usize) callconv(.c) void {
    requireBootStep(ok, @enumFromInt(phase), text[0..len]);
}
export fn r4os_zig_runtime_start() callconv(.c) noreturn {
    return runtime_boot.start();
}

fn runNvmeInterruptAcceptanceProbe() bool {
    if (!block_storage.nvmeInterruptAcceptanceProbe()) return false;
    const required: u32 = (1 << 0) | (1 << 8) | (1 << 9) | (1 << 10) | (1 << 11) | (1 << 12);
    const forbidden: u32 = (1 << 13) | (1 << 14) | (1 << 15);
    var index: usize = 0;
    while (index < block_storage.maxDevices()) : (index += 1) {
        const device = block_storage.get(index) orelse continue;
        if (device.bus != .nvme) continue;
        var status = driver_api.StorageBackendStatus{
            .state = 0,
            .last_error = 0,
            .last_lba = 0,
            .last_sectors = 0,
            .recoveries = 0,
            .recovery_failures = 0,
        };
        if (driver_api.queryStorageBackendStatus(index, &status) and
            (status.state & required) == required and (status.state & forbidden) == 0)
        {
            log.puts("[NVMEIRQ] result=OK mode=");
            log.puts(if ((status.state & (1 << 16)) != 0) "msix" else "msi");
            log.puts(" irq=nonzero work=nonzero completions=nonzero poll_fallback=0\r\n");
            return true;
        }
        log.puts("[NVMEIRQ] result=FAILED state=0x");
        log.putHex(status.state, 8);
        log.puts("\r\n");
        return false;
    }
    log.puts("[NVMEIRQ] result=FAILED reason=device-missing\r\n");
    return false;
}

fn beginBootStep(phase: bootscreen.Phase, status: []const u8) void {
    bootscreen.setPhase(phase);
    bootscreen.setStatus(status);
}

fn stackGuardTestMain() callconv(.c) void {
    _ = stackGuardRecurse(1);
}

fn stackGuardRecurse(depth: u64) u64 {
    var pad: [512]u8 = undefined;
    pad[0] = @truncate(depth);
    std.mem.doNotOptimizeAway(&pad);
    const next = stackGuardRecurse(depth + 1);
    return next +% pad[0];
}

fn requireBootStep(ok: bool, phase: crash.BootPhase, fallback_message: []const u8) void {
    if (!ok) fatal.haltPendingOrMessage(phase, fallback_message);
    fatal.setBootPhase(phase);
}

fn handleZigPanic(message: []const u8, ret_addr: ?usize) noreturn {
    if (ret_addr) |address| {
        log.puts("[CRASH] panic-ret=0x");
        log.putHex(address, 16);
        log.puts("\r\n");
    }
    fatal.zigPanic(message);
}
