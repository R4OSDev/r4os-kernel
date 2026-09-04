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
const smp = @import("kernel/smp.zig");

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
    beginBootStep(.framebuffer, "Startbild");
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
    beginBootStep(.cpu, "CPU initialisieren");
    cpu_boot.init();
    fatal.setBootPhase(.timer);
    beginBootStep(.timer, "Timer initialisieren");
    timer_boot.init();
    fatal.setBootPhase(.driver);
    beginBootStep(.driver, "Basistreiber laden");
    driver_boot.init();
    fatal.setBootPhase(.input);
    beginBootStep(.input, "Eingabe initialisieren");
    input_boot.initKeyboard();
    input_boot.initMouse();
    input_boot.completePs2();
    fatal.setBootPhase(.entry);
    beginBootStep(.intro, "Startinfo laden");
    requireBootStep(boot_intro.init(), .entry, "Boot intro failed");
    fatal.setBootPhase(.memory);
    beginBootStep(.memory, "Speicher initialisieren");
    requireBootStep(memory_boot.initCore(), .memory, "Memory boot failed");
    memory_boot.dumpBlockSummary();
    fatal.setBootPhase(.storage);
    beginBootStep(.storage_foundation, "Laufwerke vorbereiten");
    requireBootStep(storage_boot.initFoundation(), .storage, "Storage foundation failed");
    fatal.setBootPhase(.module);
    beginBootStep(.module, "Module laden");
    module_boot.init();
    fatal.setBootPhase(.platform);
    beginBootStep(.platform, "Plattform erfassen");
    requireBootStep(platform_boot.initDeviceMappings(), .platform, "Platform boot failed");
    fatal.setBootPhase(.usb);
    beginBootStep(.usb_preload, "USB-Protokolle laden");
    requireBootStep(usb_protocol_preload_boot.init(), .usb, "USB protocol preload failed");
    fatal.setBootPhase(.service);
    beginBootStep(.service, "Dienste vorbereiten");
    service_boot.init();
    fatal.setBootPhase(.platform);
    const pcie_status = platform_boot.pcieStatus() orelse fatal.kernelFatal(.platform, "PCIe status missing after platform boot");
    fatal.setBootPhase(.storage);
    beginBootStep(.storage_controllers, "Controller starten");
    requireBootStep(storage_boot.initControllers(pcie_status), .storage, "Storage controller boot failed");
    fatal.setBootPhase(.loader);
    beginBootStep(.loader, "System laden");
    requireBootStep(loader_boot.initFilesystemLoader(), .loader, "Loader boot failed");
    requireBootStep(system_update_recovery_boot.recoverBeforeRuntime(), .loader, "System update recovery failed");
    requireBootStep(upload_claim_boot.recoverBeforeRuntime(), .loader, "Upload publish claim recovery failed");
    fatal.setBootPhase(.irq);
    beginBootStep(.irq, "Interrupts starten");
    requireBootStep(platform_irq_boot.init(), .irq, "Platform IRQ boot failed");
    fatal.setBootPhase(.audio);
    beginBootStep(.audio, "Audio vorbereiten");
    requireBootStep(audio_boot.init(), .audio, "Audio boot failed");
    fatal.setBootPhase(.network);
    beginBootStep(.network, "Netzwerk vorbereiten");
    requireBootStep(network_boot.init(), .network, "Network boot failed");
    fatal.setBootPhase(.usb);
    beginBootStep(.usb_hid, "USB-Eingabe starten");
    requireBootStep(usb_hid_boot.init(), .usb, "USB-HID boot failed");
    fatal.setBootPhase(.task_runtime);
    beginBootStep(.task_runtime, "Task-Runtime starten");
    requireBootStep(runtime_boot.initTaskRuntime(), .task_runtime, "Task runtime boot failed");
    fatal.setBootPhase(.driver_policy);
    beginBootStep(.driver_policy, "Treiberplan erstellen");
    requireBootStep(driver_policy_boot.init(), .driver_policy, "Driver policy boot failed");
    fatal.setBootPhase(.runtime);
    beginBootStep(.runtime, "Runtime vorbereiten");
    bootscreen.setStatus("SMP pruefen");
    const quick_acceptance = smp.acceptanceProbeEnabled();
    // Keep the scheduler/preemption witness isolated from device IRQ work.
    // The NVMe proof follows it and a final marker tells the host that both
    // bounded probes have emitted their complete diagnostics.
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
    // 0.56.2: Hintergrund-RX-Task - NACH initTaskRuntime (sonst von
    // task.init() gewischt) und NACH driver_policy_boot (NIC geladen).
    bootscreen.setStatus("Netzwerk-Task starten");
    _ = net_core.startRxTask();
    // 0.59.13: DHCP acquisition follows the real R4D link in its own task.
    // It must start after the NIC modules and scheduler, just like net-rx.
    bootscreen.setStatus("DHCP starten");
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
    bootscreen.setStatus("USB-Polling starten");
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
    bootscreen.setStatus("Task-Registry pruefen");
    _ = task_registry_selftest.runIfEnabled();
    // 0.56.15: Absichtlicher Kernel-Stack-Overflow (nur -Dstack-guard-test):
    // der Thread rennt in seine Guard-Page, erwartet wird ein sauberer
    // Page-Fault-Crash-Report mit "STACK GUARD HIT task=st-overflow".
    if (comptime config.enable_stack_guard_test) {
        _ = sched_task.createKernelThread("st-overflow", stackGuardTestMain);
    }
    bootscreen.setStatus("Runtime starten");
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
