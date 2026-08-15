const boot_config = @import("boot_config.zig");
const boot_status = @import("boot_status.zig");
const bootscreen = @import("bootscreen.zig");
const bootlog = @import("bootlog.zig");
const drive = @import("../fs/drive.zig");
const fatal = @import("fatal.zig");
const k = @import("log.zig");
const loader_perf = @import("loader_perf.zig");
const r4x = @import("../program/r4x.zig");
const usb_msc = @import("../driver/usb/msc.zig");
const scheduler = @import("../sched/scheduler.zig");
const interrupts = @import("../arch/x86_64/interrupts.zig");

const TERMINAL_FALLBACK_PATH = "/R4OS/SOFTWARE/TERMINAL/TERMINAL.R4X";
const SERVICE_MANAGER_BOOT_PATH = "/R4OS/SOFTWARE/TERMINAL/SERVMAN.R4X";
const SERVICE_MANAGER_BOOT_ARGS = "BOOT";
const RECOVERY_FALLBACK_PATHS = [_][]const u8{
    "/R4OS/SOFTWARE/TERMINAL/RECOVERY.R4X",
};

pub fn start(config: *const boot_config.Config, usable_bytes: u64) noreturn {
    r4x.initializeRuntime(usable_bytes);
    bootlog.puts("[LAUNCH] Legacy shell runtime bridge disabled; external shell required\r\n");

    const configured_path = boot_config.shellPath(config);
    const configured_args = boot_config.shellArgs(config);

    if (drive.get('C')) |boot_drive| {
        const working_drive = drive.current() orelse boot_drive;
        startServiceManagerBoot(boot_drive, working_drive);

        switch (tryStartConfigured(boot_drive, working_drive, configured_path, configured_args)) {
            .ran => waitForShellAfterBoot(),
            .not_found, .failed => {},
        }

        if (!samePath(configured_path, TERMINAL_FALLBACK_PATH)) {
            switch (tryStartTerminalFallback(boot_drive, working_drive)) {
                .ran => waitForShellAfterBoot(),
                .not_found => {
                    k.puts("External Terminal fallback not found: ");
                    k.puts(TERMINAL_FALLBACK_PATH);
                    k.puts("\r\n");
                    bootlog.puts("[LAUNCH][WARN] external terminal fallback not found\r\n");
                },
                .failed => {
                    k.puts("External Terminal fallback failed: ");
                    k.puts(TERMINAL_FALLBACK_PATH);
                    k.puts("\r\n");
                    bootlog.puts("[LAUNCH][WARN] external terminal fallback failed\r\n");
                },
            }
        } else {
            bootlog.puts("[LAUNCH] terminal fallback skipped: configured shell is Terminal\r\n");
        }

        startRecoveryFallbacks(boot_drive, working_drive, configured_path);
    } else {
        k.puts("Boot drive C: not mounted\r\n");
        bootlog.puts("[LAUNCH][WARN] C: not mounted; external shell fallback unavailable\r\n");
    }

    noExternalShellCrash(configured_path);
}

fn startServiceManagerBoot(boot_drive: *drive.Drive, working_drive: *drive.Drive) void {
    bootlog.puts("[LAUNCH] service manager boot=");
    bootlog.puts(SERVICE_MANAGER_BOOT_PATH);
    bootlog.puts(" args=");
    bootlog.puts(SERVICE_MANAGER_BOOT_ARGS);
    bootlog.puts("\r\n");

    const perf_start = loader_perf.beginServiceBoot();
    const result = r4x.runPath(boot_drive, SERVICE_MANAGER_BOOT_PATH, SERVICE_MANAGER_BOOT_ARGS, working_drive);
    switch (result) {
        .ran => {
            loader_perf.finishServiceBoot(perf_start, loader_perf.service_boot_ran);
            bootlog.puts("[LAUNCH] service manager boot completed\r\n");
        },
        .not_found => {
            loader_perf.finishServiceBoot(perf_start, loader_perf.service_boot_not_found);
            k.puts("Service manager not found: ");
            k.puts(SERVICE_MANAGER_BOOT_PATH);
            k.puts("\r\n");
            bootlog.puts("[LAUNCH][WARN] service manager boot not found\r\n");
        },
        .failed => {
            loader_perf.finishServiceBoot(perf_start, loader_perf.service_boot_failed);
            k.puts("Service manager failed: ");
            k.puts(SERVICE_MANAGER_BOOT_PATH);
            k.puts("\r\n");
            bootlog.puts("[LAUNCH][WARN] service manager boot failed\r\n");
        },
    }
}

fn tryStartConfigured(boot_drive: *drive.Drive, working_drive: *drive.Drive, path: []const u8, args: []const u8) r4x.RunResult {
    bootlog.puts("[LAUNCH] configured shell=");
    bootlog.puts(path);
    if (args.len > 0) {
        bootlog.puts(" args=");
        bootlog.puts(args);
    }
    bootlog.puts("\r\n");

    return switch (r4x.runShellPathWithHost(boot_drive, path, args, working_drive, shellHostForPath(path))) {
        .ran => .ran,
        .not_found => resultNotFound("Configured shell", path),
        .failed => resultFailed("Configured shell", path),
    };
}

fn tryStartTerminalFallback(boot_drive: *drive.Drive, working_drive: *drive.Drive) r4x.RunResult {
    bootlog.puts("[LAUNCH] external terminal fallback=");
    bootlog.puts(TERMINAL_FALLBACK_PATH);
    bootlog.puts("\r\n");

    return switch (r4x.runShellPathWithHost(boot_drive, TERMINAL_FALLBACK_PATH, "", working_drive, .terminal_mode)) {
        .ran => .ran,
        .not_found => .not_found,
        .failed => .failed,
    };
}

fn startRecoveryFallbacks(boot_drive: *drive.Drive, working_drive: *drive.Drive, configured_path: []const u8) void {
    for (RECOVERY_FALLBACK_PATHS) |path| {
        if (samePath(configured_path, path) or samePath(TERMINAL_FALLBACK_PATH, path)) continue;
        switch (tryStartRecoveryFallback(boot_drive, working_drive, path)) {
            .ran => waitForShellAfterBoot(),
            .not_found => {
                k.puts("External recovery shell not found: ");
                k.puts(path);
                k.puts("\r\n");
                bootlog.puts("[LAUNCH][WARN] external recovery shell not found: ");
                bootlog.puts(path);
                bootlog.puts("\r\n");
            },
            .failed => {
                k.puts("External recovery shell failed: ");
                k.puts(path);
                k.puts("\r\n");
                bootlog.puts("[LAUNCH][WARN] external recovery shell failed: ");
                bootlog.puts(path);
                bootlog.puts("\r\n");
            },
        }
    }
}

fn tryStartRecoveryFallback(boot_drive: *drive.Drive, working_drive: *drive.Drive, path: []const u8) r4x.RunResult {
    bootlog.puts("[LAUNCH] external recovery shell=");
    bootlog.puts(path);
    bootlog.puts("\r\n");

    return switch (r4x.runShellPathWithHost(boot_drive, path, "", working_drive, .terminal_mode)) {
        .ran => .ran,
        .not_found => .not_found,
        .failed => .failed,
    };
}

fn resultNotFound(label: []const u8, path: []const u8) r4x.RunResult {
    k.puts(label);
    k.puts(" not found: ");
    k.puts(path);
    k.puts("\r\n");
    bootlog.puts("[LAUNCH][WARN] ");
    bootlog.puts(label);
    bootlog.puts(" not found: ");
    bootlog.puts(path);
    bootlog.puts("\r\n");
    return .not_found;
}

fn resultFailed(label: []const u8, path: []const u8) r4x.RunResult {
    k.puts(label);
    k.puts(" failed: ");
    k.puts(path);
    k.puts("\r\n");
    bootlog.puts("[LAUNCH][WARN] ");
    bootlog.puts(label);
    bootlog.puts(" failed: ");
    bootlog.puts(path);
    bootlog.puts("\r\n");
    return .failed;
}

fn noExternalShellCrash(configured_path: []const u8) noreturn {
    bootlog.puts("[LAUNCH][CRASH] no external shell available; checked config=");
    bootlog.puts(configured_path);
    bootlog.puts(" terminal=");
    bootlog.puts(TERMINAL_FALLBACK_PATH);
    bootlog.puts(" recovery=");
    for (RECOVERY_FALLBACK_PATHS) |path| {
        bootlog.puts(path);
        bootlog.puts(" ");
    }
    bootlog.puts("\r\n");

    const msc = usb_msc.status();
    if (msc.present and !msc.block_registered) {
        fatal.kernelFatal(.shell, msc.reason);
    }
    if (msc.present and (msc.read_failures != 0 or msc.transport_failures != 0)) {
        // Preserve the first actionable BOT/xHCI phase, completion code and
        // byte counts instead of replacing it with a generic final message.
        fatal.kernelFatal(.shell, msc.reason);
    }
    if (drive.get('C') == null) {
        fatal.kernelFatal(.shell, "Boot drive C: not mounted");
    }
    fatal.kernelFatal(.shell, "No external shell available");
}

fn waitForShellAfterBoot() noreturn {
    bootscreen.completeForHandoff();
    boot_status.releaseForUserSession();
    // 0.56.40: wie scheduler.exitCurrent - nie heiss spinnen, bei
    // leerer Ready-Menge IF explizit oeffnen und hlt'en (IF=0-Ring-
    // Befund, SLEEP-Haenger). hlt NUR im Idle-Fall: dieser Boot-Task
    // bleibt fuer immer in der Rotation; ein unkonditionales hlt nach
    // jedem yield bremste jede Rotationsrunde um bis zu einen Tick
    // (FSDIAG-Smoke-Watchdog-Befund).
    while (true) {
        if (scheduler.hasOtherReadyTask()) {
            scheduler.yield();
        } else {
            interrupts.enable();
            interrupts.waitForInterrupt();
        }
    }
}

fn samePath(a: []const u8, b: []const u8) bool {
    return pathLen(a) == pathLen(b) and pathEq(a, b);
}

fn shellHostForPath(path: []const u8) r4x.ConsoleHostKind {
    if (samePath(path, TERMINAL_FALLBACK_PATH)) return .terminal_mode;
    return .none;
}

fn pathEq(a: []const u8, b: []const u8) bool {
    if (pathLen(a) != pathLen(b)) return false;
    var i: usize = 0;
    while (i < a.len and i < b.len) : (i += 1) {
        if (normalizePathByte(a[i]) != normalizePathByte(b[i])) return false;
    }
    return true;
}

fn pathLen(path: []const u8) usize {
    var end = path.len;
    while (end > 0 and path[end - 1] == 0) : (end -= 1) {}
    return end;
}

fn normalizePathByte(ch: u8) u8 {
    if (ch == '\\') return '/';
    if (ch >= 'a' and ch <= 'z') return ch - ('a' - 'A');
    return ch;
}
