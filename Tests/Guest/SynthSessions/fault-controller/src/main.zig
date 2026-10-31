const std = @import("std");
const r = @import("r4os");
const p = @import("shared.zig");
const a = r.abi;
var line: [768]u8 = undefined;
var output: [8192]u8 = undefined;
const Probe = struct {
    sys: r.r4sys.Context,
    audio: r.r4audio.Context,
    dev: r.r4dev.Context,
    fn report(self: *const Probe, comptime fmt: []const u8, values: anytype) void {
        self.sys.println(std.fmt.bufPrint(&line, fmt, values) catch "FAULT87 report overflow");
    }
    fn controls(self: *const Probe, value: p.Controls) bool {
        var copy = value;
        var input: a.ProtocolBuffer = .{ .data = &copy, .len = @sizeOf(p.Controls), .capacity = @sizeOf(p.Controls) };
        var out: a.ProtocolBuffer = .{};
        return self.dev.protocolDispatch(p.role, p.configure, &input, &out) == 0;
    }
    fn snapshot(self: *const Probe) ?p.State {
        var state: p.State = .{};
        var input: a.ProtocolBuffer = .{};
        var out: a.ProtocolBuffer = .{ .data = &state, .capacity = @sizeOf(p.State) };
        if (self.dev.protocolDispatch(p.role, p.snapshot, &input, &out) != 0 or out.len != @sizeOf(p.State) or state.magic != p.magic) return null;
        return state;
    }
    fn count(self: *const Probe, sid: bool, minimum: u64) ?p.State {
        const until = self.sys.ticks() +| self.sys.ticksFromMilliseconds(4000);
        while (true) {
            const state = self.snapshot() orelse return null;
            if ((if (sid) state.sid_stop_calls else state.midi_stops) >= minimum) return state;
            if (self.sys.ticks() >= until) return null;
            self.sys.sleepTicks(1);
        }
    }
    fn held(self: *const Probe, h: *const a.ProgramProcessHandle, o: *const a.ProgramObservationHandle, sid: bool) bool {
        var done: a.ProgramProcessCompletion = .{};
        const wait = self.sys.programObservationWait(o, 0, &done);
        const until = self.sys.ticks() +| self.sys.ticksFromMilliseconds(500);
        var acquired: i32 = 0;
        while (true) {
            acquired = if (sid) self.audio.sidAcquire() else self.audio.midiOpenSynth("OPL3");
            if (acquired != a.service_api_result_busy or self.sys.ticks() >= until) break;
            self.sys.sleepTicks(1);
        }
        self.report("FAULT87 HELD child={d}:{d} wait={d} acquire={d}", .{ h.instance_id, h.generation, wait, acquired });
        return wait == a.program_handle_error_timeout and acquired == (if (sid) @as(i32, -2) else @as(i32, -3));
    }
    fn closePcm(self: *const Probe, id: u32) i32 {
        const until = self.sys.ticks() +| self.sys.ticksFromMilliseconds(3000);
        var rc = self.audio.audioClose(id);
        while (rc == a.service_api_result_busy and self.sys.ticks() < until) {
            self.sys.sleepTicks(1);
            rc = self.audio.audioClose(id);
        }
        return rc;
    }
    fn run(self: *const Probe, mode: u8) bool {
        const sid = mode == 'M';
        if (!self.controls(.{})) return false;
        defer _ = self.controls(.{});
        const opened = self.audio.audioOpenStream(48000, 2, .s16le);
        if (opened <= 0) return false;
        const stream: u32 = @intCast(opened);
        var pcm_live = true;
        defer if (pcm_live) {
            _ = self.closePcm(stream);
        };
        const pcm = [_]u8{0x23} ** 1920;
        if (self.audio.audioWrite(stream, &pcm) != pcm.len) return false;
        const before = self.snapshot() orelse return false;
        var control_config: p.Controls = .{ .sid_stop_result = if (sid) -55 else 0, .midi_stop_result = if (sid) 0 else -55 };
        if (!self.controls(control_config)) return false;
        var h: a.ProgramProcessHandle = .{};
        const args: [2:0]u8 = .{ mode, 0 };
        const spawn = self.sys.programSpawnWithConsoleHostHandle("C:\\TEMP\\F87A.R4X", &args, .console, .terminal_window, &h);
        if (spawn != 0) return false;
        var obs: a.ProgramObservationHandle = .{};
        if (self.sys.programObservationRetain(&h, 0, &obs) != 0) {
            _ = self.sys.programHandleKill(&h);
            return false;
        }
        var observation_live = true;
        var finished = false;
        defer {
            if (!finished) {
                _ = self.controls(.{});
                _ = self.sys.programHandleKill(&h);
                var d: a.ProgramProcessCompletion = .{};
                _ = self.sys.programHandleWait(&h, self.sys.ticksFromMilliseconds(4000), &d);
            }
            if (observation_live) _ = self.sys.programObservationRelease(&obs);
        }
        const first = self.count(sid, (if (sid) before.sid_stop_calls else before.midi_stops) + 1) orelse return false;
        if (!self.held(&h, &obs, sid)) return false;
        control_config.sid_stop_result = if (sid) a.service_api_result_busy else 0;
        control_config.midi_stop_result = if (sid) 0 else a.service_api_result_busy;
        if (!self.controls(control_config)) return false;
        _ = self.count(sid, (if (sid) first.sid_stop_calls else first.midi_stops) + 1) orelse return false;
        if (!self.held(&h, &obs, sid)) return false;
        if (!self.controls(.{})) return false;
        var done: a.ProgramProcessCompletion = .{};
        const wait = self.sys.programObservationWait(&obs, self.sys.ticksFromMilliseconds(8000), &done);
        var bytes: u32 = 0;
        const read = if (wait == 0) self.sys.programObservationRead(&obs, 0, &output, &bytes) else -99;
        const reaped = self.sys.programHandleReap(&h, &done);
        finished = wait == 0 and reaped == 0;
        self.report("FAULT87 COMPLETE child={d}:{d} wait={d} exit={d} reason={d} flags={d} bytes={d}/{d} read={d} reap={d}", .{ h.instance_id, h.generation, wait, done.exit_code, done.exit_reason, done.flags, bytes, done.output_length, read, reaped });
        if (read == 0) self.sys.write(output[0..@min(bytes, output.len)]);
        if (!finished or done.exit_code != -4097 or done.exit_reason != a.program_exit_reason_failed or done.flags & (a.program_completion_flag_output_incomplete | a.program_completion_flag_output_truncated) != 0 or read != 0 or bytes != done.output_length or done.handle.instance_id != h.instance_id or done.handle.generation != h.generation or std.mem.indexOf(u8, output[0..@min(bytes, output.len)], "IO87 submit=0") == null) return false;
        var exact = false;
        var info: a.ProgramStackFaultInfo = .{};
        for (0..4) |cpu| {
            if (self.dev.stackFault(@intCast(cpu), &info) == 1 and info.instance_id == h.instance_id and info.instance_generation == h.generation and info.exception_vector == 6 and info.task_generation != 0 and info.thread_generation != 0) {
                exact = true;
                self.report("FAULT87 EXACT owner={d}:{d} thread={d}:{d} task={d}:{d} vector={d} kind={d} rsp={x}", .{ info.instance_id, info.instance_generation, info.thread_id, info.thread_generation, info.task_id, info.task_generation, info.exception_vector, info.stack_kind, info.interrupted_rsp });
                break;
            }
        }
        const after = self.snapshot() orelse return false;
        const continued = self.audio.audioWrite(stream, &pcm);
        const reacquired = if (sid) self.audio.sidAcquire() else self.audio.midiOpenSynth("OPL3");
        const closed = if (reacquired == 1) (if (sid) self.audio.sidRelease(1) else self.audio.midiClose(1)) else -99;
        const pcm_close = self.closePcm(stream);
        if (pcm_close == 0) pcm_live = false;
        const released = self.sys.programObservationRelease(&obs);
        if (released == 0) observation_live = false;
        self.report("FAULT87 OBS release={d}", .{released});
        self.report("FAULT87 RESOURCES exact={d} private_acks={d} idle={d} stop={d} pcm_continue={d} reacquire={d} close={d} pcm_close={d}", .{ @intFromBool(exact), if (sid) after.sid_stop_acks - before.sid_stop_acks else after.midi_stop_acks - before.midi_stop_acks, after.idle_calls - before.idle_calls, after.stop_calls - before.stop_calls, continued, reacquired, closed, pcm_close });
        return exact and (if (sid) after.sid_stop_acks == before.sid_stop_acks + 1 else after.midi_stop_acks == before.midi_stop_acks + 1) and after.idle_calls == before.idle_calls and after.stop_calls == before.stop_calls and continued == pcm.len and reacquired == 1 and closed == 0 and pcm_close == 0 and released == 0;
    }
};
pub fn r4_app_main(app: *r.App) i32 {
    const args = app.args();
    if (args.len != 1 or (args[0] != 'M' and args[0] != 'T' and args[0] != 'D')) return 2;
    const probe = Probe{ .sys = app.system(), .audio = app.audioLowLevel() orelse return 3, .dev = app.devicesLowLevel() orelse return 3 };
    var input: a.ProtocolBuffer = .{};
    var out: a.ProtocolBuffer = .{};
    if (probe.dev.protocolDispatch(p.role, p.ping, &input, &out) != 0) return 4;
    _ = probe.dev.performanceSummary();
    if (args[0] == 'D') {
        if (!probe.controls(.{ .write_result = -87 })) return 5;
        const id = probe.audio.audioOpenStream(48000, 2, .s16le);
        if (id <= 0) return 6;
        probe.sys.println("FAULT87 DRIVER_NEGATIVE controlled actualR4D write callback UD2 must remain globalfatal");
        const pcm = [_]u8{0x23} ** 1920;
        _ = probe.audio.audioWrite(@intCast(id), &pcm);
        probe.sys.sleepTicks(probe.sys.ticksFromMilliseconds(2000));
        return 99;
    }
    const ok = probe.run(args[0]);
    probe.report("FAULT87 RESULT mode={c} result={s}; actual CPU exception/Core/resource ACKs; no physicalDMA claim", .{ args[0], if (ok) @as([]const u8, "PASS") else @as([]const u8, "FAIL") });
    return if (ok) 0 else 1;
}
