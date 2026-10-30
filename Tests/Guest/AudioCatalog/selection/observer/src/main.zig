const std = @import("std");
const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var line: [768]u8 = undefined;
var worker_audio: r4os.Audio = undefined;
var worker_request: a.AudioServiceOutputRequest = .{};
var worker_page: a.AudioServiceOutputState = .{};
var worker_result: i32 = -99;
var worker_done: bool = false;
fn timeout(ms: u64) r4os.time_contract.Timeout {
    return r4os.time_contract.timeoutFinite(.{ .nanoseconds = ms * 1_000_000 });
}
fn code(result: r4os.app_audio.Result) i32 {
    return switch (result) { .ok => 0, .timed_out => a.service_api_result_timeout, .failure => |raw| raw };
}
fn writeCode(result: r4os.app_audio.WriteResult) i32 {
    return switch (result) {
        .written => |n| @intCast(n), .busy => |n| if (n == 0) a.service_api_result_busy else -91,
        .timed_out => |n| if (n == 0) a.service_api_result_timeout else -92,
        .failure => |f| f.raw,
    };
}
fn selection(_: u64) callconv(.c) i32 {
    worker_result = code(worker_audio.selectOutput(&worker_request, timeout(5000), &worker_page));
    @atomicStore(bool, &worker_done, true, .release);
    return worker_result;
}
const Probe = struct {
    sys: r4os.r4sys.Context,
    dev: r4os.r4dev.Context,
    fn report(self: *const Probe, comptime format: []const u8, args: anytype) void {
        self.sys.println(std.fmt.bufPrint(&line, format, args) catch "P56 report overflow");
    }
    fn control(self: *const Probe, value: p.Controls) bool {
        var copy = value;
        var data: a.ProtocolBuffer = .{ .data = &copy, .len = @sizeOf(p.Controls), .capacity = @sizeOf(p.Controls) };
        var out: a.ProtocolBuffer = .{};
        return self.dev.protocolDispatch(p.role, p.configure, &data, &out) == 0;
    }
    fn snapshot(self: *const Probe) ?p.State {
        var result: p.State = .{};
        var data: a.ProtocolBuffer = .{};
        var out: a.ProtocolBuffer = .{ .data = &result, .capacity = @sizeOf(p.State) };
        if (self.dev.protocolDispatch(p.role, p.snapshot, &data, &out) != 0 or out.len != @sizeOf(p.State) or result.magic != p.magic) return null;
        return result;
    }
    fn release(self: *const Probe) void {
        var data: a.ProtocolBuffer = .{}; var out: a.ProtocolBuffer = .{};
        _ = self.dev.protocolDispatch(p.role, p.release_select, &data, &out);
    }
};
pub fn r4_app_main(app: *r4os.App) i32 {
    const probe = Probe{ .sys = app.system(), .dev = app.devicesLowLevel() orelse return 2 };
    const audio = app.audio() orelse return 3;
    _ = probe.dev.performanceSummary() orelse return 4;
    if (!probe.control(.{ .output_count = 2 })) return 5;
    probe.sys.sleepTicks(@max(probe.sys.ticksFromMilliseconds(650), 1));
    var page: a.AudioServiceOutputState = .{};
    var request: a.AudioServiceOutputRequest = .{};
    if (code(audio.outputs(&request, timeout(2000), &page)) != 0 or page.total != 2) return 6;
    var existing = switch (audio.openStream(48000, 2, .s16le, 65536, timeout(2000))) { .stream => |s| s, else => return 7 };
    defer _ = existing.close(timeout(2000));
    const data: [64]u8 = @splat(0x20);
    if (writeCode(existing.writeOnce(&data, timeout(2000))) != data.len) return 8;
    var lazy = switch (audio.openStream(48000, 2, .s16le, 65536, timeout(2000))) { .stream => |s| s, else => return 9 };
    defer _ = lazy.close(timeout(2000));
    if (!probe.control(.{ .output_count = 2, .select_hold_ms = 2500 })) return 10;
    probe.sys.sleepTicks(@max(probe.sys.ticksFromMilliseconds(650), 1));
    if (code(audio.outputs(&request, timeout(2000), &page)) != 0) return 11;
    worker_audio = audio; worker_request = .{};
    @memcpy(worker_request.id[0..5], "P56-1");
    worker_result = -99; @atomicStore(bool, &worker_done, false, .release);
    var handle: a.ProgramJoinHandle = .{};
    if (probe.sys.threadCreateHandle(selection, 0, 128 * 1024, 0, &handle) != a.thread_ok) return 12;
    var joined = false;
    defer if (!joined) {
        probe.release(); var exit: i32 = -99;
        _ = probe.sys.threadHandleJoin(&handle, a.io_wait_forever, &exit);
    };
    const deadline = probe.sys.ticks() +| probe.sys.ticksFromMilliseconds(2000);
    var seen = probe.snapshot() orelse return 13;
    while (seen.select_entered == 0 and probe.sys.ticks() < deadline) {
        probe.sys.sleepTicks(1); seen = probe.snapshot() orelse return 14;
    }
    if (seen.select_entered == 0 or seen.select_finished != 0) return 15;
    const started = probe.sys.ticks();
    var status: a.AudioServiceStatus = .{};
    const status_rc = code(audio.status(timeout(100), &status));
    var master: a.AudioServiceMasterState = .{};
    const master_rc = code(audio.masterState(timeout(100), &master));
    const outputs_rc = code(audio.outputs(&request, timeout(100), &page));
    const volume_rc = code(existing.setVolume(32768, timeout(100)));
    const lazy_rc = writeCode(lazy.writeOnce(&data, timeout(100)));
    const existing_rc = writeCode(existing.writeOnce(&data, timeout(100)));
    var other: a.AudioServiceOutputRequest = .{};
    @memcpy(other.id[0..5], "P56-0");
    var other_page: a.AudioServiceOutputState = .{};
    const second_rc = code(audio.selectOutput(&other, timeout(100), &other_page));
    const elapsed = probe.sys.ticks() -| started;
    seen = probe.snapshot() orelse return 16;
    const premature = @atomicLoad(bool, &worker_done, .acquire);
    const before = app.args().len != 0 and app.args()[0] == 'B';
    var ok = !premature and seen.select_finished == 0 and seen.selections == 1;
    if (before) {
        for ([_]i32{status_rc, master_rc, outputs_rc, volume_rc, lazy_rc, existing_rc, second_rc}) |rc| ok = ok and rc == a.service_api_result_timeout;
    } else {
        ok = ok and status_rc == 0 and master_rc == 0 and outputs_rc == 0 and volume_rc == 0 and
            lazy_rc == a.service_api_result_busy and (existing_rc == data.len or existing_rc == a.service_api_result_busy) and second_rc == a.service_api_result_busy and
            status.flags & a.audio_service_flag_backend_present == 0 and page.reason == a.audio_output_reason_catalog_busy;
    }
    probe.report("P56 DISPATCH mode={s} entered={d} finished={d} status={d} master={d} outputs={d} volume={d} lazyWrite={d} existingWrite={d} secondIntent={d} elapsedTicks={d} premature={d} selectCalls={d} pass={d}", .{ if (before) @as([]const u8,"BEFORE") else @as([]const u8,"AFTER"),seen.select_entered, seen.select_finished,status_rc,master_rc,outputs_rc,volume_rc,lazy_rc,existing_rc,second_rc,elapsed,@intFromBool(premature),seen.selections,@intFromBool(ok) });
    probe.release();
    var exit: i32 = -99;
    const join = probe.sys.threadHandleJoin(&handle, probe.sys.ticksFromMilliseconds(6000), &exit);
    joined = join == a.thread_ok;
    seen = probe.snapshot() orelse return 17;
    const acknowledged = join == a.thread_ok and exit == 0 and worker_result == 0 and seen.select_finished != 0 and seen.active_index == 1 and std.mem.eql(u8,&worker_page.desired_id,&worker_request.id);
    probe.report("P56 ACK join={d} exit={d} result={d} finished={d} active={d} desiredMatch={d} pass={d}", .{join,exit,worker_result,seen.select_finished,seen.active_index,@intFromBool(std.mem.eql(u8,&worker_page.desired_id,&worker_request.id)),@intFromBool(acknowledged)});
    ok = ok and acknowledged;
    probe.report("P56 DISPATCH_RESULT {s}; real normal Core/R4D/AUDSVC, no hardware/speed/hearing claim", .{if(ok) @as([]const u8,"PASS") else @as([]const u8,"FAIL")});
    return if (ok) 0 else 1;
}
