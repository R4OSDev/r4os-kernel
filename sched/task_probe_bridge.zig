// Temporary stateless private owner snapshots for the original native task probes.
// No task, queue, reserve, timeout or test orchestration is implemented here.
const cfg = @import("../kernel/boot_config.zig");
const virt = @import("../memory/virt.zig");
const ipc = @import("../kernel/service_ipc.zig");
const work = @import("../kernel/driver_work.zig");
const block = @import("../storage/block.zig");
const net = @import("../net/core.zig");
const input = @import("../driver/usb/hid.zig");
const Span = extern struct { pointer: ?[*]const u8 = null, len: usize = 0 };
const Vm = extern struct { ranges: u64, reserved: u64, committed: u64, resident: u64 };
const Owner = extern struct { started: u64, id: u32 };
const Owners = extern struct { ipc: Owner, work: Owner, block: Owner, net: Owner, input: Owner, input_bound: bool };
const Network = extern struct { started: bool, id: u32, generation: u64, polls: u64, iterations: u64 };
export fn r4os_zig_task_probe_option(out: *Span) callconv(.c) void {
    const value = cfg.optionValue(cfg.get(), "TASKREGISTRY", "selftest") orelse {
        out.* = .{};
        return;
    };
    out.* = .{ .pointer = value.ptr, .len = value.len };
}
export fn r4os_zig_task_probe_vm(out: *Vm) callconv(.c) void {
    const value = virt.stats();
    out.* = .{ .ranges = value.active_ranges, .reserved = value.reserved_bytes, .committed = value.committed_bytes, .resident = value.resident_bytes };
}
export fn r4os_zig_task_probe_owners(out: *Owners) callconv(.c) void {
    const a = ipc.performanceSummary();
    const b = work.summary();
    const c = block.runtimeWorkerSummary();
    const d = net.rxTaskSummary();
    const e = input.pollTaskSummary();
    const f = input.status();
    out.* = .{ .ipc = .{ .started = a.worker_started, .id = a.worker_task_id }, .work = .{ .started = b.worker_started, .id = b.worker_task_id }, .block = .{ .started = c.worker_started, .id = c.worker_task_id }, .net = .{ .started = @intFromBool(d.started), .id = d.task_id }, .input = .{ .started = @intFromBool(e.started), .id = e.task_id }, .input_bound = f.keyboard_bound or f.mouse_bound };
}
export fn r4os_zig_task_probe_network(out: *Network) callconv(.c) void {
    const value = net.rxTaskSummary();
    out.* = .{ .started = value.started, .id = value.task_id, .generation = value.task_generation, .polls = value.polls, .iterations = value.iterations };
}
const WorkHandler = *const fn (usize) callconv(.c) i32;
export fn r4os_zig_task_probe_work_submit(lane: usize, handler: WorkHandler, deadline: u64, budget: u32, serial_key: u64, deadline_lane: bool, out: *u32) callconv(.c) i32 {
    if (!deadline_lane) return work.submit(0, handler, lane, 0, out);
    return work.submitRequest(0, &.{ .handler = handler, .context = lane, .serial_key = serial_key, .deadline_tick = deadline, .budget_ticks = budget }, out);
}
export fn r4os_zig_task_probe_work(operation: u8, handle: u32, timeout: u64, out: ?*i32) callconv(.c) i32 {
    return switch (operation) {
        0 => work.completionWait(handle, timeout, out.?),
        1 => work.cancel(handle),
        2 => work.completionRelease(handle),
        else => unreachable,
    };
}
