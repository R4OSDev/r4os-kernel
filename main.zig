// R4OS Kernel - Einstieg.

const std = @import("std");
const limine = @import("bootloader/limine.zig");
const boot_info = @import("bootloader/boot_info.zig");
const interrupts = @import("arch/x86_64/interrupts.zig");
const config = @import("config");
const crash = @import("kernel/crash.zig");
const crash_screen = @import("kernel/crash_screen.zig");
const fatal = @import("kernel/fatal.zig");
const boot_intro = @import("kernel/boot_intro.zig");
const com_debug_boot = @import("kernel/com_debug_boot.zig");
const cpu_boot = @import("kernel/cpu_boot.zig");
const timer_boot = @import("kernel/timer_boot.zig");
const driver_boot = @import("kernel/driver_boot.zig");
const input_boot = @import("kernel/input_boot.zig");
const memory_boot = @import("kernel/memory_boot.zig");
const storage_boot = @import("kernel/storage_boot.zig");
const module_boot = @import("kernel/module_boot.zig");
const usb_protocol_preload_boot = @import("kernel/usb_protocol_preload_boot.zig");
const service_boot = @import("kernel/service_boot.zig");
const platform_boot = @import("kernel/platform_boot.zig");
const loader_boot = @import("kernel/loader_boot.zig");
const system_update_recovery_boot = @import("kernel/system_update_recovery_boot.zig");
const upload_claim_boot = @import("kernel/upload_claim_boot.zig");
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

pub const panic = std.debug.FullPanic(handleZigPanic);

export fn kmain() callconv(.c) noreturn {
    kernel_version.keepMetadata();
    limine.keepRequests();
    crash.init();
    fatal.init();
    fatal.setBootPhase(.entry);
    com_debug_boot.init();
    _ = boot_info.init();
    _ = boot_display.init();
    boot_status.beginBootLogRedirect();
    bootscreen.setPhase(.framebuffer);
    if (config.enable_kernel_fatal_test) fatal.kernelFatal(.entry, "Kernel fatal crash test");
    if (config.enable_zig_panic_test) @panic("Zig panic crash test");
    if (config.enable_crash_screen_test) {
        const report = crash.fromCpuException(.{
            .frame = .{
                .registers = .{
                    .rax = 0x0000_0000_0000_0001,
                    .rbx = 0x0000_0000_0000_0002,
                    .rcx = 0x0000_0000_0000_0003,
                    .rdx = 0x0000_0000_0000_0004,
                    .rbp = 0xffff_ffff_8000_1000,
                },
                .vector = 14,
                .error_code = 0b10111,
                .rip = 0xffff_ffff_8000_2000,
                .rsp = 0xffff_ffff_8000_3000,
                .cs = 0x8,
                .rflags = 0x202,
            },
            .cr2 = 0xffff_ffff_dead_beef,
            .boot_phase = .entry,
            .ticks = 0,
            .memory = crash.untrackedMemory(),
            .message = "Crash screen renderer test",
        });
        _ = crash_screen.render(&report);
        interrupts.haltForever();
    }
    fatal.setBootPhase(.cpu);
    bootscreen.setPhase(.cpu);
    cpu_boot.init();
    fatal.setBootPhase(.timer);
    bootscreen.setPhase(.timer);
    timer_boot.init();
    fatal.setBootPhase(.driver);
    bootscreen.setPhase(.driver);
    driver_boot.init();
    fatal.setBootPhase(.input);
    bootscreen.setPhase(.input);
    input_boot.initKeyboard();
    input_boot.initMouse();
    input_boot.completePs2();
    fatal.setBootPhase(.entry);
    bootscreen.setPhase(.intro);
    requireBootStep(boot_intro.init(), .entry, "Boot intro failed");
    fatal.setBootPhase(.memory);
    bootscreen.setPhase(.memory);
    requireBootStep(memory_boot.initCore(), .memory, "Memory boot failed");
    memory_boot.dumpBlockSummary();
    fatal.setBootPhase(.storage);
    bootscreen.setPhase(.storage_foundation);
    requireBootStep(storage_boot.initFoundation(), .storage, "Storage foundation failed");
    fatal.setBootPhase(.module);
    bootscreen.setPhase(.module);
    module_boot.init();
    fatal.setBootPhase(.platform);
    bootscreen.setPhase(.platform);
    requireBootStep(platform_boot.initDeviceMappings(), .platform, "Platform boot failed");
    fatal.setBootPhase(.usb);
    bootscreen.setPhase(.usb_preload);
    requireBootStep(usb_protocol_preload_boot.init(), .usb, "USB protocol preload failed");
    fatal.setBootPhase(.service);
    bootscreen.setPhase(.service);
    service_boot.init();
    fatal.setBootPhase(.platform);
    const pcie_status = platform_boot.pcieStatus() orelse fatal.kernelFatal(.platform, "PCIe status missing after platform boot");
    fatal.setBootPhase(.storage);
    bootscreen.setPhase(.storage_controllers);
    requireBootStep(storage_boot.initControllers(pcie_status), .storage, "Storage controller boot failed");
    fatal.setBootPhase(.loader);
    bootscreen.setPhase(.loader);
    requireBootStep(loader_boot.initFilesystemLoader(), .loader, "Loader boot failed");
    requireBootStep(system_update_recovery_boot.recoverBeforeRuntime(), .loader, "System update recovery failed");
    requireBootStep(upload_claim_boot.recoverBeforeRuntime(), .loader, "Upload publish claim recovery failed");
    fatal.setBootPhase(.irq);
    bootscreen.setPhase(.irq);
    requireBootStep(platform_irq_boot.init(), .irq, "Platform IRQ boot failed");
    fatal.setBootPhase(.audio);
    bootscreen.setPhase(.audio);
    requireBootStep(audio_boot.init(), .audio, "Audio boot failed");
    fatal.setBootPhase(.network);
    bootscreen.setPhase(.network);
    requireBootStep(network_boot.init(), .network, "Network boot failed");
    fatal.setBootPhase(.usb);
    bootscreen.setPhase(.usb_hid);
    requireBootStep(usb_hid_boot.init(), .usb, "USB-HID boot failed");
    fatal.setBootPhase(.task_runtime);
    bootscreen.setPhase(.task_runtime);
    requireBootStep(runtime_boot.initTaskRuntime(), .task_runtime, "Task runtime boot failed");
    fatal.setBootPhase(.driver_policy);
    bootscreen.setPhase(.driver_policy);
    requireBootStep(driver_policy_boot.init(), .driver_policy, "Driver policy boot failed");
    fatal.setBootPhase(.runtime);
    bootscreen.setPhase(.runtime);
    // 0.56.2: Hintergrund-RX-Task - NACH initTaskRuntime (sonst von
    // task.init() gewischt) und NACH driver_policy_boot (NIC geladen).
    _ = net_core.startRxTask();
    // 0.59.13: DHCP acquisition follows the real R4D link in its own task.
    // It must start after the NIC modules and scheduler, just like net-rx.
    _ = net_core.startDhcpTask();
    // Invasive correctness workloads are excluded from every normal kernel
    // artifact. The explicit -Dboot-selftests diagnostic kernel preserves
    // their real boot-time coverage without charging product readiness.
    if (comptime config.enable_boot_selftests) {
        if (!sched_sync.selfTest()) fatal.kernelFatal(.runtime, "Boot sync selftest failed");
    }
    if (comptime config.enable_boot_selftests or config.enable_block_dispatch_selftest) {
        if (!block_storage.parallelDispatchSelfTest()) fatal.kernelFatal(.runtime, "Block dispatch selftest failed");
    }
    // 0.56.17: Autonomer Input-Poll-Task - NACH initTaskRuntime (sonst von
    // task.init() gewischt); pollt USB-HID im 10-ms-Takt ohne Konsumenten.
    _ = usb_hid_boot.startPollTask();
    if (comptime config.enable_boot_selftests) {
        if (!sched_scheduler.prioritySelfTest()) fatal.kernelFatal(.runtime, "Boot scheduler priority selftest failed");
        boot_status.statusLine("BOOTSELFTEST OK heap=1 page_tables=1 sync=1 priority=1\r\n");
    } else {
        boot_status.statusLine("BOOTSELFTEST OFF heap=0 page_tables=0 sync=0 priority=0\r\n");
    }
    // 0.59.10: Nur mit OPTION TASKREGISTRY selftest=yes. Die echte QEMU-
    // Abnahme belastet die dynamische Task-/Wait-/Stack-/Reserve-Linie;
    // normale Produktionsboots fuehren hier keinen Zusatztest aus.
    _ = task_registry_selftest.runIfEnabled();
    // 0.56.15: Absichtlicher Kernel-Stack-Overflow (nur -Dstack-guard-test):
    // der Thread rennt in seine Guard-Page, erwartet wird ein sauberer
    // Page-Fault-Crash-Report mit "STACK GUARD HIT task=st-overflow".
    if (comptime config.enable_stack_guard_test) {
        _ = sched_task.createKernelThread("st-overflow", stackGuardTestMain);
    }
    return runtime_boot.start();
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
    _ = ret_addr;
    fatal.zigPanic(message);
}
