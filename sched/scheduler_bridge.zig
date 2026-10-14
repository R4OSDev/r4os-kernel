const native_tasks = @import("task.zig");
// Stateless serial presentation for the native scheduler; no task owner cuts.
const task = @import("task.zig");
const timer = @import("../kernel/timer.zig");
const k = @import("../kernel/log.zig");
const scheduler = @import("scheduler.zig");
export fn r4os_zig_scheduler_log(event: u8, target: ?*task.Task, a: u64, b: u64) callconv(.c) void {
    switch (event) {
        0 => {
            k.puts("[TIMER] deadline_queue=ordered idle=");
            k.puts(if (a != 0) "one-shot" else "periodic-fallback");
            k.puts(" backend=");
            k.puts(timer.backendName());
            k.puts("\r\n");
        },
        1 => {
            k.puts("[SMP] productive cpu=");
            k.putDec(a);
            k.puts(" task=");
            k.puts(native_tasks.get(target.?, .name));
            k.puts(" class=r4x\r\n");
        },
        2 => {
            const t = target.?;
            k.puts("[SMP] switch boundary violation cpu=");
            k.putDec(a);
            k.puts(" task=");
            k.putDec(native_tasks.get(t, .id));
            k.puts("/");
            k.puts(native_tasks.get(t, .name));
            k.puts(" depth=");
            k.putDec(b);
            k.puts(" state=");
            k.puts(task.stateName(native_tasks.get(t, .state)));
            k.puts("\r\n");
        },
        3 => {
            const t = target.?;
            k.puts("TASK EXIT INVARIANT: owned synchronization remains id=");
            k.putDec(native_tasks.get(t, .id));
            k.puts(" locks=");
            k.putDec(native_tasks.get(t, .held_lock_count));
            k.puts(" unwind=");
            k.putDec(native_tasks.get(t, .unwind_guard_count));
            k.puts(" objects=");
            var wrote = false;
            for (native_tasks.get(t, .held_locks)) |held| {
                if (!held.active) continue;
                if (wrote) k.puts(",");
                k.puts(held.name);
                k.puts("=");
                k.putHex(held.object_id, 16);
                k.puts("@");
                k.putDec(held.rank);
                wrote = true;
            }
            if (!wrote) k.puts("none");
            k.puts("\r\n");
        },
        4 => {
            k.puts("TASK EXIT INVARIANT: transition rejected id=");
            k.putDec(native_tasks.get(target.?, .id));
            k.puts("\r\n");
        },
        5 => k.puts("IRQ FPU GUARD task identity changed inside handler\r\n"),
        6 => k.puts("SCHEDPRIO FAIL busy-spawn\r\n"),
        7 => {
            const st = scheduler.stats();
            k.puts(if (a != 0) "SCHEDPRIO OK" else "SCHEDPRIO FAIL");
            k.puts(" high_latency_max=");
            k.putDec(b);
            k.puts(" bar=3 picks_high=");
            k.putDec(st.priority_picks_high);
            k.puts(" picks_normal=");
            k.putDec(st.priority_picks_normal);
            k.puts(" rr_picks=");
            k.putDec(st.priority_rr_picks);
            k.puts(" selects=");
            k.putDec(st.priority_selects);
            k.puts("\r\n");
        },
        else => unreachable,
    }
}
