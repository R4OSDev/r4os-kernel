const std = @import("std");
const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var api: *const r4os.r4dev.DriverApi = undefined;
var state: p.State = .{};
var backend: r4os.r4dev.AudioOutputIdleBackend = undefined;
var secondary: r4os.r4dev.AudioOutputIdleBackend = undefined;
var threads: r4os.r4dev.DriverThreadContext = undefined;
var thread: u64 = 0;
comptime {
    asm (r4os.r4dev.driverEntriesAsm("p55_init", "p55_shutdown"));
}
fn dispatch(op: u32, data: ?*anyopaque, len: u32) i32 {
    const ctx = r4os.r4dev.DriverContext.init(api);
    var input: a.ProtocolBuffer = .{ .data = data, .len = len, .capacity = len };
    var out: a.ProtocolBuffer = .{};
    return ctx.protocolDispatch(p.role, op, &input, &out);
}
export fn p55_init(table: *const r4os.r4dev.DriverApi) callconv(.c) i32 {
    api = table;
    const ctx = r4os.r4dev.DriverContext.init(api);
    backend = .{ .base = .{ .base = .{ .version = a.audio_backend_idle_version, .size = @sizeOf(r4os.r4dev.AudioOutputIdleBackend), .write_pcm = write, .stop = stop, .status = status, .min_rate = 8000, .max_rate = 192000, .max_channels = 2 }, .outputs = .{ .query = query, .select = select, .active = active } }, .idle = idle };
    if (ctx.registerAudioOutputIdle("P55D", &backend) != 0 or ctx.registerAudioOutputEpoch("P55D", epoch) != 0) return -10;
    secondary = backend;
    secondary.base.outputs = .{ .query = secondaryQuery, .select = secondarySelect, .active = secondaryActive };
    if (ctx.registerAudioOutputIdle("P55B", &secondary) != 0 or ctx.registerAudioOutputEpoch("P55B", secondaryEpoch) != 0) return -11;
    threads = ctx.threads() orelse return -12;
    if (threads.start(pacer, 0, a.driver_thread_flag_parallel, &thread) != 0) return -13;
    return 0;
}
export fn p55_shutdown() callconv(.c) i32 {
    @atomicStore(u32, &state.terminal_stop, 1, .release);
    if (thread != 0) {
        _ = threads.stop(thread);
        var result: i32 = 0;
        if (threads.join(thread, api.timer_frequency() * 2, &result) != 0) return -20;
        if (threads.release(thread) != 0) return -21;
        thread = 0;
    }
    _ = dispatch(p.unregister, null, 0);
    return 0;
}
fn write(_: ?*anyopaque, _: [*]const u8, len: u32, _: u32, _: u16, _: u16) callconv(.c) i32 {
    _ = p.count(&state.writes);
    _ = @atomicRmw(u64, &state.bytes, .Add, len, .acq_rel);
    return 0;
}
fn stop(_: ?*anyopaque) callconv(.c) i32 {
    _ = p.count(&state.stops);
    return 0;
}
fn idle(_: ?*anyopaque) callconv(.c) i32 {
    _ = p.count(&state.idles);
    return 0;
}
fn status(_: ?*anyopaque, out: *a.AudioBackendStatus) callconv(.c) i32 {
    var registration: p.Registration = .{ .address = @intFromPtr(&state), .bytes = @sizeOf(p.State) };
    _ = dispatch(p.register, &registration, @sizeOf(p.Registration));
    out.* = .{ .active = 1, .writes = p.load(&state.writes), .refills = p.load(&state.writes), .buffer_bytes = 1920 };
    return 0;
}
fn query(_: u64, index: u32, out: *a.AudioOutputInfo) callconv(.c) i32 {
    if (!p.acquire(&state)) return a.service_api_result_busy;
    defer p.release(&state);
    const number = p.count(&state.queries);
    if (p.load(&state.reentrant_at_query) == number) {
        const ctx = r4os.r4dev.DriverContext.init(api);
        const result = ctx.unregisterAudioBackend("P55D");
        @atomicStore(i32, &state.reentrant_result, result, .release);
        _ = p.count(&state.reentrant_attempts);
    }
    const fail = p.load(&state.fail_at_query);
    if (fail != 0 and number >= fail) return a.service_api_result_busy;
    if (p.load(&state.change_at_query) == number) _ = p.count(&state.generation);
    out.* = .{};
    if (index >= p.load(&state.output_count)) return 0;
    _ = std.fmt.bufPrint(&out.id, "P55-{d}", .{index}) catch unreachable;
    _ = std.fmt.bufPrint(&out.name, "generation:{d}/output:{d}", .{ p.load(&state.generation), index }) catch unreachable;
    out.kind = a.audio_output_kind_speaker;
    out.availability = a.audio_output_available;
    out.preferred_rate = 48000;
    out.channels = 2;
    out.format = 1;
    return 1;
}
fn select(_: u64, index: u32) callconv(.c) i32 {
    if (!p.acquire(&state)) return a.service_api_result_busy;
    defer p.release(&state);
    if (index >= p.load(&state.output_count)) return -1;
    _ = p.count(&state.selections);
    @atomicStore(i32, &state.active_index, @intCast(index), .release);
    _ = p.count(&state.generation);
    return 0;
}
fn active(_: u64) callconv(.c) i32 {
    _ = p.count(&state.actives);
    return if (p.load(&state.output_count) == 0) -1 else p.load(&state.active_index);
}

fn epoch(_: u64, out: *a.AudioOutputEpoch) callconv(.c) i32 {
    if (!p.acquire(&state)) return a.service_api_result_busy;
    defer p.release(&state);
    _ = p.count(&state.epochs);
    out.* = .{ .generation = p.load(&state.generation), .count = p.load(&state.output_count), .active_index = if (p.load(&state.output_count) == 0) -1 else p.load(&state.active_index) };
    switch (p.load(&state.bad_epoch)) {
        1 => out.count = 33, 2 => out.generation = 0, 3 => out.active_index = @intCast(out.count),
        4 => out.version = 0, 5 => out.size = 23, else => {},
    }
    return 1;
}
fn secondaryEpoch(_: u64, out: *a.AudioOutputEpoch) callconv(.c) i32 {
    if (!p.acquire(&state)) return a.service_api_result_busy;
    defer p.release(&state);
    _ = p.count(&state.secondary_epochs);
    const n = p.load(&state.secondary_count);
    out.* = .{ .generation = p.load(&state.secondary_generation), .count = n, .active_index = if (n == 0) -1 else 0 };
    return 1;
}
fn secondaryQuery(_: u64, index: u32, out: *a.AudioOutputInfo) callconv(.c) i32 {
    if (!p.acquire(&state)) return a.service_api_result_busy;
    defer p.release(&state);
    _ = p.count(&state.secondary_queries);
    if (index == 0 and @atomicRmw(u32, &state.cross_change, .Xchg, 0, .acq_rel) != 0) _ = p.count(&state.generation);
    out.* = .{};
    if (index >= p.load(&state.secondary_count)) return 0;
    _ = std.fmt.bufPrint(&out.id, "P55B-{d}", .{index}) catch unreachable;
    _ = std.fmt.bufPrint(&out.name, "Secondary:{d}", .{index}) catch unreachable;
    out.kind = a.audio_output_kind_line_out; out.availability = a.audio_output_available;
    out.preferred_rate = 48000; out.channels = 2; out.format = 1;
    return 1;
}
fn secondarySelect(_: u64, index: u32) callconv(.c) i32 {
    if (!p.acquire(&state)) return a.service_api_result_busy;
    defer p.release(&state);
    return if (index < p.load(&state.secondary_count)) 0 else -1;
}
fn secondaryActive(_: u64) callconv(.c) i32 { return if (p.load(&state.secondary_count) == 0) -1 else 0; }
fn registryWork(_: usize) callconv(.c) i32 {
    const ctx = r4os.r4dev.DriverContext.init(api);
    const removed = ctx.unregisterAudioBackend("P55D");
    if (removed != 0) return removed;
    const added = ctx.registerAudioOutputIdle("P55D", &backend);
    if (added != 0) return added;
    return ctx.registerAudioOutputEpoch("P55D", epoch);
}
fn pacer(_: usize) callconv(.c) i32 {
    const ctx = r4os.r4dev.DriverContext.init(api);
    while (p.load(&state.terminal_stop) == 0) {
        const request = p.load(&state.rebind_request);
        if (request != 0 and request != p.load(&state.rebind_completed)) {
            var handle: u32 = 0;
            if (ctx.workSubmitOwned(registryWork, 0, &handle) != 0) return -30;
            var status_value: a.DriverCompletionStatus = .{};
            while (true) {
                if (ctx.completionStatus(handle, &status_value) != 0) return -31;
                if (status_value.state == a.driver_work_state_completed or status_value.state == a.driver_work_state_cancelled) break;
                _ = threads.sleepTicks(1);
            }
            if (ctx.completionRelease(handle) != 0) return -32;
            if (status_value.result == a.driver_work_owner_busy) { _ = threads.sleepTicks(1); continue; }
            @atomicStore(i32, &state.rebind_result, status_value.result, .release);
            @atomicStore(u32, &state.rebind_completed, request, .release);
        }
        _ = threads.sleepTicks(2);
    }
    @atomicStore(u32, &state.thread_done, 1, .release);
    return 0;
}
