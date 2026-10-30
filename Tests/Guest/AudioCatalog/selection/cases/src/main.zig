const std = @import("std");
const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var line: [768]u8 = undefined;
fn timeout(ms: u64) r4os.time_contract.Timeout {
    return r4os.time_contract.timeoutFinite(.{ .nanoseconds = ms * 1_000_000 });
}
fn code(result: r4os.app_audio.Result) i32 {
    return switch (result) {
        .ok => 0,
        .timed_out => a.service_api_result_timeout,
        .failure => |v| v,
    };
}
fn id(n: u8) [64]u8 {
    var out: [64]u8 = @splat(0);
    @memcpy(out[0..4], "P56-");
    out[4] = '0' + n;
    return out;
}
const Probe = struct {
    sys: r4os.r4sys.Context,
    dev: r4os.r4dev.Context,
    fn report(self: *const Probe, comptime format: []const u8, args: anytype) void {
        self.sys.println(std.fmt.bufPrint(&line, format, args) catch "P56X overflow");
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
        if (self.dev.protocolDispatch(p.role, p.snapshot, &data, &out) != 0 or out.len != @sizeOf(p.State)) return null;
        return result;
    }
    fn release(self: *const Probe) void {
        var data: a.ProtocolBuffer = .{};
        var out: a.ProtocolBuffer = .{};
        _ = self.dev.protocolDispatch(p.role, p.release_select, &data, &out);
    }
    fn entered(self: *const Probe) ?p.State {
        const end = self.sys.ticks() +| self.sys.ticksFromMilliseconds(2000);
        while (self.sys.ticks() < end) {
            const s = self.snapshot() orelse return null;
            if (s.select_entered != 0 and s.select_finished == 0) return s;
            self.sys.sleepTicks(1);
        }
        return null;
    }
    fn selected(self: *const Probe, audio: *const r4os.Audio, target: u8, page: *a.AudioServiceOutputState) bool {
        const end = self.sys.ticks() +| self.sys.ticksFromMilliseconds(2000);
        const target_id = id(target);
        var request: a.AudioServiceOutputRequest = .{};
        while (self.sys.ticks() < end) {
            if (code(audio.outputs(&request, timeout(100), page)) != 0) {
                // Read-only retries while a newly started endpoint registers.
                // The absolute deadline above is never extended.
                self.sys.sleepTicks(1);
                continue;
            }
            if (page.reason != a.audio_output_reason_catalog_busy and std.mem.eql(u8, &page.desired_id, &target_id) and std.mem.eql(u8, &page.active_id, &target_id)) return true;
            self.sys.sleepTicks(1);
        }
        return false;
    }
    fn baseline(_: *const Probe, audio: *const r4os.Audio) bool {
        var request: a.AudioServiceOutputRequest = .{};
        var page: a.AudioServiceOutputState = .{};
        if (code(audio.outputs(&request, timeout(2000), &page)) != 0) return false;
        const zero = id(0);
        if (std.mem.eql(u8, &page.desired_id, &zero) and std.mem.eql(u8, &page.active_id, &zero)) return true;
        request.id = zero;
        return code(audio.selectOutput(&request, timeout(4000), &page)) == 0 and std.mem.eql(u8, &page.desired_id, &zero);
    }
};
var client_audio: r4os.Audio = undefined;
var client_ms: u64 = 80;
var client_result: i32 = -99;
var client_done: bool = false;
var client_page: a.AudioServiceOutputState = .{};
fn client(_: u64) callconv(.c) i32 {
    var request: a.AudioServiceOutputRequest = .{ .id = id(1) };
    client_page = .{};
    client_result = code(client_audio.selectOutput(&request, timeout(client_ms), &client_page));
    @atomicStore(bool, &client_done, true, .release);
    return client_result;
}
var pcm_stream: r4os.AudioStream = undefined;
var pcm_sys: r4os.r4sys.Context = undefined;
var pcm_stop: bool = false;
var pcm_accepted: u64 = 0;
var pcm_writes: u64 = 0;
var pcm_busy: u64 = 0;
fn pcm(_: u64) callconv(.c) i32 {
    const bytes: [1920]u8 = @splat(0x20);
    while (!@atomicLoad(bool, &pcm_stop, .acquire)) {
        switch (pcm_stream.writeOnce(&bytes, timeout(100))) {
            .written => |n| {
                _ = @atomicRmw(u64, &pcm_accepted, .Add, n, .acq_rel);
                _ = @atomicRmw(u64, &pcm_writes, .Add, 1, .acq_rel);
            },
            .busy => |n| {
                _ = @atomicRmw(u64, &pcm_accepted, .Add, n, .acq_rel);
                _ = @atomicRmw(u64, &pcm_busy, .Add, 1, .acq_rel);
            },
            .timed_out => return a.service_api_result_timeout,
            .failure => |failure| return failure.raw,
        }
        pcm_sys.sleepTicks(@max(pcm_sys.ticksFromMilliseconds(5), 1));
    }
    return code(pcm_stream.close(timeout(3000)));
}
fn automatic(probe: *const Probe, audio: *const r4os.Audio) bool {
    if (!probe.baseline(audio)) return false;
    var request: a.AudioServiceOutputRequest = .{ .id = id(1) };
    var page: a.AudioServiceOutputState = .{};
    if (code(audio.selectOutput(&request, timeout(4000), &page)) != 0) return false;
    if (!probe.control(.{ .output_count = 2, .select_hold_ms = 2500 })) return false;
    probe.sys.sleepTicks(probe.sys.ticksFromMilliseconds(650));
    var status: a.AudioServiceStatus = .{};
    const rc = code(audio.status(timeout(100), &status));
    const started = probe.entered() orelse return false;
    var master: a.AudioServiceMasterState = .{};
    const master_rc = code(audio.masterState(timeout(100), &master));
    request = .{};
    const outputs_rc = code(audio.outputs(&request, timeout(100), &page));
    const held = probe.snapshot() orelse return false;
    const independent = rc == 0 and master_rc == 0 and outputs_rc == 0 and held.select_finished == 0 and held.selections == 1 and
        status.flags & a.audio_service_flag_backend_present == 0 and page.reason == a.audio_output_reason_catalog_busy and
        held.queries == started.queries and held.epochs == started.epochs;
    probe.release();
    const finished = probe.selected(audio, 1, &page);
    probe.report("P56X AUTO status={d} master={d} outputs={d} held={d} selectCalls={d} queryDelta={d} epochDelta={d} acknowledged={d} pass={d}", .{ rc, master_rc, outputs_rc, @intFromBool(held.select_finished == 0), held.selections, held.queries - started.queries, held.epochs - started.epochs, @intFromBool(finished), @intFromBool(independent and finished) });
    return independent and finished;
}
fn failureCase(probe: *const Probe, audio: *const r4os.Audio, result: i32) bool {
    if (!probe.baseline(audio) or !probe.control(.{ .output_count = 2, .select_result = result })) return false;
    var request: a.AudioServiceOutputRequest = .{ .id = id(1) };
    var page: a.AudioServiceOutputState = .{};
    const rc = code(audio.selectOutput(&request, timeout(2000), &page));
    request = .{};
    const read = code(audio.outputs(&request, timeout(100), &page));
    const seen = probe.snapshot() orelse return false;
    const zero = id(0);
    const ok = rc == result and read == 0 and seen.selections == 1 and seen.select_finished != 0 and seen.active_index == 0 and
        std.mem.eql(u8, &page.desired_id, &zero) and std.mem.eql(u8, &page.active_id, &zero) and page.reason == a.audio_output_reason_activation_failed;
    probe.report("P56X PROVIDER_FAILURE expected={d} actual={d} read={d} active={d} desiredRetained={d} selectCalls={d} reason={d} pass={d}", .{ result, rc, read, seen.active_index, @intFromBool(std.mem.eql(u8, &page.desired_id, &zero)), seen.selections, page.reason, @intFromBool(ok) });
    return ok;
}
fn lostReply(probe: *const Probe, audio: *const r4os.Audio) bool {
    if (!probe.baseline(audio) or !probe.control(.{ .output_count = 2, .select_hold_ms = 2500 })) return false;
    client_audio = audio.*;
    client_ms = 80;
    @atomicStore(bool, &client_done, false, .release);
    var thread: a.ProgramJoinHandle = .{};
    if (probe.sys.threadCreateHandle(client, 0, 0, 0, &thread) != a.thread_ok) return false;
    var joined = false;
    defer if (!joined) {
        probe.release();
        var exit: i32 = 0;
        _ = probe.sys.threadHandleJoin(&thread, a.io_wait_forever, &exit);
    };
    _ = probe.entered() orelse return false;
    var exit: i32 = -99;
    const join = probe.sys.threadHandleJoin(&thread, probe.sys.ticksFromMilliseconds(500), &exit);
    joined = join == a.thread_ok;
    const held = probe.snapshot() orelse return false;
    var status: a.AudioServiceStatus = .{};
    const status_rc = code(audio.status(timeout(100), &status));
    var request: a.AudioServiceOutputRequest = .{ .id = id(0) };
    var page: a.AudioServiceOutputState = .{};
    const other = code(audio.selectOutput(&request, timeout(100), &page));
    const pending = join == a.thread_ok and exit == a.service_api_result_timeout and client_result == a.service_api_result_timeout and held.select_finished == 0 and held.selections == 1 and status_rc == 0 and other == a.service_api_result_busy;
    probe.release();
    const acknowledged = probe.selected(audio, 1, &page);
    const final = probe.snapshot() orelse return false;
    const alive = code(audio.status(timeout(100), &status)) == 0;
    const ok = pending and acknowledged and alive and final.selections == 1;
    probe.report("P56X LOST_REPLY callerJoin={d} callerExit={d} callerResult={d} providerStillHeld={d} status={d} otherIntent={d} afterACK={d} serviceAlive={d} selectCalls={d} pass={d}", .{ join, exit, client_result, @intFromBool(held.select_finished == 0), status_rc, other, @intFromBool(acknowledged), @intFromBool(alive), final.selections, @intFromBool(ok) });
    return ok;
}
fn childDeath(probe: *const Probe, audio: *const r4os.Audio) bool {
    if (!probe.baseline(audio) or !probe.control(.{ .output_count = 2, .select_hold_ms = 2500 })) return false;
    var child: a.ProgramProcessHandle = .{};
    if (probe.sys.programSpawnHandle("C:\\TEMP\\P56X.R4X", "C", .console, &child) != a.program_handle_ok) return false;
    var reaped = false;
    defer if (!reaped) {
        _ = probe.sys.programHandleKill(&child);
        var end: a.ProgramProcessCompletion = .{};
        _ = probe.sys.programHandleWait(&child, a.io_wait_forever, &end);
        _ = probe.sys.programHandleReap(&child, &end);
    };
    _ = probe.entered() orelse return false;
    const kill = probe.sys.programHandleKill(&child);
    var completion: a.ProgramProcessCompletion = .{};
    const wait = probe.sys.programHandleWait(&child, probe.sys.ticksFromMilliseconds(2000), &completion);
    var reap = if (wait == a.program_handle_ok) probe.sys.programHandleReap(&child, &completion) else wait;
    const reap_deadline = probe.sys.ticks() +| probe.sys.ticksFromMilliseconds(1000);
    while (reap == a.program_handle_error_would_block and probe.sys.ticks() < reap_deadline) {
        probe.sys.sleepTicks(1);
        reap = probe.sys.programHandleReap(&child, &completion);
    }
    reaped = reap == a.program_handle_ok;
    const held = probe.snapshot() orelse return false;
    var status: a.AudioServiceStatus = .{};
    const alive = code(audio.status(timeout(100), &status)) == 0;
    probe.release();
    var page: a.AudioServiceOutputState = .{};
    const selected = probe.selected(audio, 1, &page);
    const final = probe.snapshot() orelse return false;
    const ok = kill == a.program_handle_ok and wait == a.program_handle_ok and reap == a.program_handle_ok and held.select_finished == 0 and held.selections == 1 and alive and selected and final.selections == 1;
    probe.report("P56X CLIENT_DEATH id={d} generation={d} kill={d} wait={d} reap={d} providerStillHeld={d} serviceAlive={d} afterACK={d} selectCalls={d} pass={d}", .{ child.instance_id, child.generation, kill, wait, reap, @intFromBool(held.select_finished == 0), @intFromBool(alive), @intFromBool(selected), final.selections, @intFromBool(ok) });
    return ok;
}
fn restart(probe: *const Probe, audio: *const r4os.Audio) bool {
    if (!probe.baseline(audio)) return false;
    // Finish earlier persistence first. The pending selection must create its
    // own persistence worker during normal service shutdown after the ACK.
    const persist_deadline = probe.sys.ticks() +| probe.sys.ticksFromMilliseconds(3000);
    var master: a.AudioServiceMasterState = .{};
    while (true) {
        if (code(audio.masterState(timeout(100), &master)) != 0) return false;
        if (master.flags & a.audio_master_state_flag_persist_pending == 0) break;
        if (probe.sys.ticks() >= persist_deadline) return false;
        probe.sys.sleepTicks(1);
    }
    if (!probe.control(.{ .output_count = 2, .select_hold_ms = 2500 })) return false;
    var old: a.ServiceInfo = .{};
    if (probe.sys.serviceStatus("AUDSVC", &old) != 0) return false;
    client_audio = audio.*;
    client_ms = 5000;
    @atomicStore(bool, &client_done, false, .release);
    var thread: a.ProgramJoinHandle = .{};
    if (probe.sys.threadCreateHandle(client, 0, 0, 0, &thread) != a.thread_ok) return false;
    var joined = false;
    defer if (!joined) {
        probe.release();
        var exit: i32 = 0;
        _ = probe.sys.threadHandleJoin(&thread, a.io_wait_forever, &exit);
    };
    _ = probe.entered() orelse return false;
    var stopped: a.ServiceInfo = .{};
    const stop = probe.sys.serviceStop("AUDSVC", &stopped, probe.sys.ticksFromMilliseconds(50));
    const held = probe.snapshot() orelse return false;
    const pending = stop == a.service_api_result_timeout and stopped.state == a.service_state_stopping and held.select_finished == 0;
    probe.release();
    var exit: i32 = -99;
    const join = probe.sys.threadHandleJoin(&thread, probe.sys.ticksFromMilliseconds(3000), &exit);
    joined = join == a.thread_ok;
    const deadline = probe.sys.ticks() +| probe.sys.ticksFromMilliseconds(5000);
    while (probe.sys.ticks() < deadline) {
        if (probe.sys.serviceStatus("AUDSVC", &stopped) != 0) return false;
        if (stopped.state == a.service_state_stopped) break;
        probe.sys.sleepTicks(1);
    }
    const final = probe.snapshot() orelse return false;
    if (!pending or !joined or stopped.state != a.service_state_stopped or stopped.exit_code != 0 or final.select_finished == 0) return false;
    var fresh: a.ServiceInfo = .{};
    const start = probe.sys.serviceStart("AUDSVC", &fresh);
    var page: a.AudioServiceOutputState = .{};
    const restored = start == 0 and probe.selected(audio, 1, &page);
    const ok = restored and fresh.instance_id != old.instance_id and final.selections == 1;
    probe.report("P56X SERVICE_RESTART stop={d} pendingState={d} providerStillHeld={d} clientJoin={d} clientExit={d} stopped={d} serviceExit={d} start={d} oldInstance={d} newInstance={d} persistedDesired={d} selectCalls={d} pass={d}", .{ stop, @as(u32, a.service_state_stopping), @intFromBool(held.select_finished == 0), join, exit, stopped.state, stopped.exit_code, start, old.instance_id, fresh.instance_id, @intFromBool(restored), final.selections, @intFromBool(ok) });
    return ok;
}
pub fn r4_app_main(app: *r4os.App) i32 {
    const probe = Probe{ .sys = app.system(), .dev = app.devicesLowLevel() orelse return 2 };
    const audio = app.audio() orelse return 3;
    if (app.args().len != 0 and app.args()[0] == 'C') {
        client_audio = audio;
        client_ms = 5000;
        return client(0);
    }
    _ = probe.dev.performanceSummary() orelse return 4;
    if (app.args().len != 0 and app.args()[0] == 'W') return withdrawal(&probe, &audio);
    if (!probe.baseline(&audio)) return 5;
    pcm_stream = switch (audio.openStream(48000, 2, .s16le, 65536, timeout(2000))) {
        .stream => |s| s,
        else => return 6,
    };
    const first: [64]u8 = @splat(0x20);
    if (pcm_stream.writeOnce(&first, timeout(2000)) != .written) return 7;
    pcm_sys = probe.sys;
    @atomicStore(bool, &pcm_stop, false, .release);
    var producer: a.ProgramJoinHandle = .{};
    if (probe.sys.threadCreateHandle(pcm, 0, 0, 0, &producer) != a.thread_ok) return 8;
    var producer_joined = false;
    defer if (!producer_joined) {
        @atomicStore(bool, &pcm_stop, true, .release);
        var exit: i32 = 0;
        _ = probe.sys.threadHandleJoin(&producer, a.io_wait_forever, &exit);
    };
    var ok = automatic(&probe, &audio);
    ok = failureCase(&probe, &audio, -1) and ok;
    ok = failureCase(&probe, &audio, a.service_api_result_busy) and ok;
    ok = lostReply(&probe, &audio) and ok;
    @atomicStore(bool, &pcm_stop, true, .release);
    var exit: i32 = -99;
    const join = probe.sys.threadHandleJoin(&producer, probe.sys.ticksFromMilliseconds(4000), &exit);
    producer_joined = join == a.thread_ok;
    const accepted = @atomicLoad(u64, &pcm_accepted, .acquire);
    const writes = @atomicLoad(u64, &pcm_writes, .acquire);
    const busy = @atomicLoad(u64, &pcm_busy, .acquire);
    const pcm_ok = join == a.thread_ok and exit == 0 and accepted > 0 and writes > 0;
    probe.report("P56X PARALLEL_PCM join={d} exit={d} acceptedBytes={d} writes={d} busy={d} pass={d}; software acceptance only", .{ join, exit, accepted, writes, busy, @intFromBool(pcm_ok) });
    ok = pcm_ok and ok;
    ok = childDeath(&probe, &audio) and ok;
    ok = restart(&probe, &audio) and ok;
    probe.report("P56X ALL {s}", .{if (ok) @as([]const u8, "PASS") else @as([]const u8, "FAIL")});
    return if (ok) 0 else 1;
}

fn withdrawal(probe: *const Probe, audio: *const r4os.Audio) i32 {
    if (!probe.baseline(audio) or !probe.control(.{ .output_count = 2, .select_hold_ms = 2500 })) return 10;
    client_audio = audio.*;
    client_ms = 5000;
    var thread: a.ProgramJoinHandle = .{};
    if (probe.sys.threadCreateHandle(client, 0, 0, 0, &thread) != a.thread_ok) return 11;
    var joined = false;
    defer if (!joined) {
        probe.release();
        var exit: i32 = 0;
        _ = probe.sys.threadHandleJoin(&thread, a.io_wait_forever, &exit);
    };
    _ = probe.entered() orelse return 12;
    // A provider control publication can withdraw its catalog while an older
    // route command is still awaiting its ACK. Neither receipt proves the
    // availability of the other; Core serialization still holds normally.
    if (!probe.control(.{ .output_count = 0 })) return 13;
    probe.release();
    var exit: i32 = -99;
    const join = probe.sys.threadHandleJoin(&thread, probe.sys.ticksFromMilliseconds(3000), &exit);
    joined = join == a.thread_ok;
    var status: a.AudioServiceStatus = .{};
    const status_rc = code(audio.status(timeout(100), &status));
    const target = id(1);
    const ok = join == a.thread_ok and exit == 0 and client_result == 0 and client_page.total == 0 and
        std.mem.eql(u8, &client_page.desired_id, &target) and client_page.active_id[0] == 0 and
        client_page.reason == a.audio_output_reason_none_available and status_rc == 0 and status.flags & a.audio_service_flag_backend_present == 0;
    probe.report("P56X WITHDRAW_ACK join={d} exit={d} result={d} total={d} activeEmpty={d} desiredRetained={d} reason={d} expectedReason={d} status={d} backendFlag={d} pass={d}", .{ join, exit, client_result, client_page.total, @intFromBool(client_page.active_id[0] == 0), @intFromBool(std.mem.eql(u8, &client_page.desired_id, &target)), client_page.reason, @as(u32, a.audio_output_reason_none_available), status_rc, status.flags & a.audio_service_flag_backend_present, @intFromBool(ok) });
    return if (ok) 0 else 1;
}
