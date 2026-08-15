// Cooperative deadman watchdog (0.60.20): turns scheduler-progressing
// lock/storage stalls into an on-screen diagnosis. A task that stays blocked for more than
// DEADLOCK_TICKS while holding at least one real Mutex is a wedged
// transaction. UnwindGuards are intentionally excluded: they are lifetime
// / hard-kill deferrals and legitimately span storage or network waits.
// When the deadman finds a candidate, it draws one bounded task snapshot,
// so a frozen machine without serial access still tells us WHO waits on
// WHAT without switching the normal runtime console route.
//
// The deadman itself does no file system or storage I/O; it only scans the
// task registry and paints to the framebuffer console. It cannot preempt a
// kernel callback that hard-spins with no scheduling point. Driver-local
// xHCI Tick/TSC deadlines own that failure class and paint directly.

const block = @import("../storage/block.zig");
const diag_screen = @import("diag_screen.zig");
const k = @import("log.zig");
const scheduler = @import("../sched/scheduler.zig");
const sched_task = @import("../sched/task.zig");
const timer = @import("timer.zig");

const SCAN_INTERVAL_TICKS: u64 = 5 * @as(u64, timer.DEFAULT_HZ);
const DEADLOCK_TICKS: u64 = 30 * @as(u64, timer.DEFAULT_HZ);
const REDUMP_INTERVAL_TICKS: u64 = 120 * @as(u64, timer.DEFAULT_HZ);

var started = false;
var last_dump_tick: u64 = 0;

pub fn start() bool {
    if (started) return true;
    // The deadman is part of the progress floor: allocator pressure must not
    // prevent it from starting and public hard-kill APIs must not remove the
    // only framebuffer-visible observer of a later storage/lock stall.
    if (sched_task.createKernelThreadCritical("deadman", deadmanMain) != null) started = true;
    return started;
}

fn deadmanMain() callconv(.c) void {
    while (true) {
        scheduler.sleepTicksWithReason(SCAN_INTERVAL_TICKS, "deadman-idle");
        const now = timer.tickCount();
        const task_snapshot = sched_task.captureDeadmanSnapshot(now, DEADLOCK_TICKS);
        const wedged_lock = task_snapshot.wedged_mutex_holder;
        const stalled_exec = block.hasStalledExecution(now, DEADLOCK_TICKS);
        if (!wedged_lock and !stalled_exec) {
            last_dump_tick = 0;
            continue;
        }
        if (last_dump_tick != 0 and now -% last_dump_tick < REDUMP_INTERVAL_TICKS) continue;
        last_dump_tick = now;

        // Framebuffer-direct dump: visible even when the desktop owns the
        // screen and the console sink is detached. A new incident clears any
        // boot-time band; saturation preserves these first/root-cause rows.
        const dump_incident = diag_screen.beginResolvableIncident();
        diag_screen.write("[DEADMAN] wedged mutex_holder=");
        diag_screen.write(if (wedged_lock) "yes" else "no");
        diag_screen.write(" stalled_storage_exec=");
        diag_screen.write(if (stalled_exec) "yes" else "no");
        diag_screen.endLine();
        if (stalled_exec) block.dumpStalledToDiag(now, DEADLOCK_TICKS);
        sched_task.dumpDeadmanSnapshotToDiag(&task_snapshot);
        diag_screen.line("[DEADMAN] end of dump");
        // No token survives the report or the following 120-second sleep.
        // Pixels and retained evidence remain, but killing/restarting the
        // cooperative watchdog cannot suppress every later root cause.
        _ = diag_screen.resolveIncident(dump_incident);

        // Keep the normal non-visible runtime hook installed: k.puts reaches
        // COM1 and bootlog/LOGSVC without reactivating the framebuffer console.
        k.puts("\r\n[DEADMAN] wedged: mutex_holder=");
        k.puts(if (wedged_lock) "yes" else "no");
        k.puts(" stalled_storage_exec=");
        k.puts(if (stalled_exec) "yes" else "no");
        k.puts("\r\n");
        if (stalled_exec) block.dumpStalledExecutions(now, DEADLOCK_TICKS);
        sched_task.dumpDeadmanSnapshotToLog(&task_snapshot);
        k.puts("[DEADMAN] end of dump\r\n");
    }
}
