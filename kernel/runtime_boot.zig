// Runtime, scheduler, and launcher completion for kernel startup.

const boot_status = @import("boot_status.zig");
const config = @import("config");
const deadman = @import("deadman.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");
const fatal = @import("fatal.zig");
const driver_work = @import("driver_work.zig");
const loader_boot = @import("loader_boot.zig");
const loader_perf = @import("loader_perf.zig");
const log = @import("log.zig");
const memory_boot = @import("memory_boot.zig");
const memory_reclaim = @import("../memory/reclaim.zig");
const r4p = @import("../program/r4p.zig");
const scheduler = @import("../sched/scheduler.zig");
const shell_launcher = @import("shell_launcher.zig");
const service_ipc = @import("service_ipc.zig");
const block_storage = @import("../storage/block.zig");
const task = @import("../sched/task.zig");

var task_runtime_initialized = false;

fn reclaimTaskStackCache(requested_stacks: u32) u32 {
    return task.reclaimStackCache(requested_stacks);
}

pub fn initTaskRuntime() bool {
    if (task_runtime_initialized) return true;

    if (!task.init()) {
        return false;
    }
    memory_reclaim.registerTaskStackReclaimer(reclaimTaskStackCache);
    log.puts("  Task system ");
    log.puts("[OK]\r\n");

    if (!scheduler.init()) {
        return false;
    }
    log.puts("  Scheduler ");
    log.puts("[OK]\r\n");

    if (!service_ipc.startRuntimeWorker()) {
        return false;
    }
    log.puts("  Service IPC worker ");
    log.puts("[OK]\r\n");

    if (!driver_work.init()) {
        return false;
    }
    log.puts("  Driver workqueue ");
    log.puts("[OK]\r\n");

    if (!block_storage.initRuntimeWorker()) {
        return false;
    }
    log.puts("  Block worker ");
    log.puts("[OK]\r\n");

    task_runtime_initialized = true;
    return true;
}

pub fn start() noreturn {
    const r4p_start = loader_perf.beginR4pRuntime();
    r4p.loadAll();
    loader_perf.finishR4pRuntime(r4p_start);
    r4p.dumpStatus();
    boot_status.statusLine("  Loader [OK]\r\n");

    if (!initTaskRuntime()) {
        fatal.kernelFatal(.runtime, "Task runtime init failed");
    }

    if (config.enable_page_fault_test) triggerPageFaultTest();

    // Runtime diagnostic, deliberately last in the boot: everything the
    // deadman scans (registry, timer, console sinks) is final here.
    if (deadman.start()) {
        boot_status.statusLine("  Deadman watchdog [OK]\r\n");
    } else {
        boot_status.statusLine("  Deadman watchdog [FAILED]\r\n");
        log.puts("[DEADMAN] kernel thread creation failed\r\n");
    }

    log.puts("\r\n  Launcher [START]\r\n");
    const loaded_config = loader_boot.config() orelse fatal.kernelFatal(.runtime, "Loader boot state missing");
    fatal.setBootPhase(.shell);
    shell_launcher.start(loaded_config, memory_boot.usableBytes());
}

fn triggerPageFaultTest() noreturn {
    const unmapped: u64 = 0xFFFF_FF00_0000_0000;
    log.puts("  Triggering test page fault at 0x");
    log.putHex(unmapped, 16);
    log.puts("...\r\n");
    const mem: [*]volatile u8 = @ptrFromInt(unmapped);
    mem[0] = 0xCC;
    interrupts.haltForever();
}
