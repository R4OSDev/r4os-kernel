const std = @import("std");
const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var line: [512]u8 = undefined;
const Probe = struct {
    sys: r4os.r4sys.Context,
    audio: r4os.r4audio.Context,
    dev: r4os.r4dev.Context,
    fn report(self: *const Probe, comptime format: []const u8, values: anytype) void {
        self.sys.println(std.fmt.bufPrint(&line, format, values) catch "SID52 report overflow");
    }
    fn controls(self: *const Probe, value: p.Controls) bool {
        var copy = value;
        var input: a.ProtocolBuffer = .{ .data = &copy, .len = @sizeOf(p.Controls), .capacity = @sizeOf(p.Controls) };
        var out: a.ProtocolBuffer = .{};
        return self.dev.protocolDispatch(p.role, p.configure, &input, &out) == 0;
    }
    fn snapshot(self: *const Probe) ?p.State {
        var value: p.State = .{};
        var input: a.ProtocolBuffer = .{};
        var out: a.ProtocolBuffer = .{ .data = &value, .capacity = @sizeOf(p.State) };
        if (self.dev.protocolDispatch(p.role, p.snapshot, &input, &out) != 0 or out.len != @sizeOf(p.State) or value.magic != p.magic) return null;
        return value;
    }
    fn closeRaw(self: *const Probe, id: u32) bool {
        const deadline = self.sys.ticks() +| self.sys.ticksFromMilliseconds(3000);
        while (self.sys.ticks() < deadline) {
            const rc = self.audio.audioClose(id);
            if (rc == 0) return true;
            if (rc != a.service_api_result_busy) return false;
            self.sys.sleepTicks(1);
        }
        return false;
    }
    fn closeService(self: *const Probe, id: u32) bool {
        const deadline = self.sys.ticks() +| self.sys.ticksFromMilliseconds(3000);
        while (self.sys.ticks() < deadline) {
            const rc = self.sys.audioServiceClose(id);
            if (rc == 0) return true;
            if (rc != a.service_api_result_busy) return false;
            self.sys.sleepTicks(1);
        }
        return false;
    }
    fn play(self: *const Probe) i32 {
        const deadline = self.sys.ticks() +| self.sys.ticksFromMilliseconds(1000);
        while (true) {
            const rc = self.audio.sidPlayFrame(1, 0x1020, 50);
            if (rc != a.service_api_result_busy or self.sys.ticks() >= deadline) return rc;
            self.sys.sleepTicks(1);
        }
    }
    fn shared(self: *const Probe) bool {
        if (!self.controls(.{})) return false;
        const opened = self.audio.audioOpenStream(48000, 2, .s16le);
        if (opened <= 0) return false;
        const raw: u32 = @intCast(opened);
        defer _ = self.closeRaw(raw);
        const service_open = self.sys.audioServiceOpenStream(48000, 2, .s16le);
        if (service_open <= 0) return false;
        const service: u32 = @intCast(service_open);
        defer _ = self.closeService(service);
        var signal: [1920]u8 = .{0x23} ** 1920;
        const written = self.audio.audioWrite(raw, &signal);
        const service_written = self.sys.audioServiceWrite(service, &signal);
        var status: a.AudioServiceStatus = .{};
        if (written != signal.len or service_written != signal.len or self.sys.audioServiceStatus(&status) != 0 or status.materialized_sessions == 0) return false;
        const handle = self.audio.sidAcquire();
        if (handle != 1) return false;
        defer _ = self.audio.sidRelease(1);
        if (self.play() != 0) return false;
        const before = self.snapshot() orelse return false;
        const stopped = self.audio.sidStop(1);
        const after = self.snapshot() orelse return false;
        const perf = self.dev.performanceSummary() orelse return false;
        self.report("SID52 SHARED stop={d} core_open={d} materialized={d} raw_bytes={d} service_bytes={d} own_calls={d} own_acks={d} idle_calls={d} stop_calls={d} reset_before={d} reset_after={d}", .{ stopped, perf.audio_open_streams, status.materialized_sessions, written, service_written, after.sid_stop_calls - before.sid_stop_calls, after.sid_stop_acks - before.sid_stop_acks, after.idle_calls - before.idle_calls, after.stop_calls - before.stop_calls, before.reset_generation, after.reset_generation });
        const continued = self.audio.audioWrite(raw, &signal);
        return stopped == 0 and perf.audio_open_streams >= 2 and continued == signal.len and after.sid_stop_acks == before.sid_stop_acks + 1 and after.idle_calls == before.idle_calls and after.stop_calls == before.stop_calls and after.reset_generation == before.reset_generation;
    }
    fn pending(self: *const Probe) bool {
        if (!self.controls(.{ .write_result = a.service_api_result_busy })) return false;
        if (self.audio.sidAcquire() != 1) return false;
        defer { _ = self.controls(.{}); _ = self.audio.sidStop(1); _ = self.audio.sidRelease(1); }
        const played = self.audio.sidPlayFrame(1, 0x1020, 50);
        const before = self.snapshot() orelse return false;
        if (!self.controls(.{ .write_result = a.service_api_result_busy, .sid_stop_result = -55 })) return false;
        const stopped = self.audio.sidStop(1);
        const failed = self.snapshot() orelse return false;
        if (!self.controls(.{})) return false;
        const retried = self.audio.sidPlayFrame(1, 0x1020, 50);
        const after = self.snapshot() orelse return false;
        self.report("SID52 PENDING play={d} stop={d} retry={d} renders_before={d} renders_after={d} failed_idle={d} failed_stop={d} accepted_bytes={d}", .{ played, stopped, retried, before.sid_renders, after.sid_renders, failed.idle_calls - before.idle_calls, failed.stop_calls - before.stop_calls, after.bytes - before.bytes });
        return played == a.service_api_result_busy and stopped == -55 and retried == 0 and after.sid_renders == before.sid_renders and after.sid_plays == before.sid_plays and failed.idle_calls == before.idle_calls and failed.stop_calls == before.stop_calls and after.bytes == before.bytes + 3840;
    }
    fn midiParallel(self: *const Probe) bool {
        if (!self.controls(.{})) return false;
        if (self.audio.midiOpenSynth("OPL3") != 1) return false;
        defer _ = self.audio.midiClose(1);
        if (self.audio.sidAcquire() != 1) return false;
        defer { _ = self.audio.sidStop(1); _ = self.audio.sidRelease(1); }
        const rendered = self.audio.midiRender(1, 480);
        const sid_rendered = self.play();
        const before = self.snapshot() orelse return false;
        const stopped = self.audio.sidStop(1);
        const after = self.snapshot() orelse return false;
        const continued = self.audio.midiRender(1, 480);
        const end = self.snapshot() orelse return false;
        const perf = self.dev.performanceSummary() orelse return false;
        self.report("SID52 MIDI core_open={d} midi_before={d} sid_render={d} sid_stop={d} midi_after={d} idle_calls={d} stop_calls={d} midi_stops={d} continued_renders={d}", .{ perf.audio_open_streams, rendered, sid_rendered, stopped, continued, after.idle_calls - before.idle_calls, after.stop_calls - before.stop_calls, after.midi_stops - before.midi_stops, end.midi_renders - after.midi_renders });
        return perf.audio_open_streams == 0 and rendered == 0 and sid_rendered == 0 and stopped == 0 and continued == 0 and after.idle_calls == before.idle_calls and after.stop_calls == before.stop_calls and after.midi_stops == before.midi_stops and end.midi_renders == after.midi_renders + 1;
    }
    fn outputChange(self: *const Probe) bool {
        if (!self.controls(.{}) or self.audio.sidAcquire() != 1) return false;
        defer { _ = self.controls(.{}); _ = self.audio.sidStop(1); _ = self.audio.sidRelease(1); }
        if (self.play() != 0 or !self.controls(.{ .idle_result = a.service_api_result_busy })) return false;
        const before = self.snapshot() orelse return false;
        const first = self.audio.sidStop(1);
        const waiting = self.snapshot() orelse return false;
        const next = 1 - before.active_output;
        const name = if (next == 0) @as([]const u8, "PROOF52_A") else @as([]const u8, "PROOF52_B");
        var id: [64]u8 = .{0} ** 64; @memcpy(id[0..name.len], name);
        const selected = self.audio.audioSelectOutput(&id);
        const changed = self.snapshot() orelse return false;
        const finished = self.audio.sidStop(1);
        const after = self.snapshot() orelse return false;
        self.report("SID52 OUTPUT first={d} select={d} final={d} old={d} new={d} generation_before={d} generation_after={d} select_stop_acks={d} idle_after_switch={d} private_calls={d}", .{ first, selected, finished, before.active_output, after.active_output, before.output_generation, after.output_generation, changed.stop_acks - waiting.stop_acks, after.idle_calls - changed.idle_calls, after.sid_stop_calls - before.sid_stop_calls });
        return first == a.service_api_result_busy and selected == 0 and finished == 0 and after.active_output == next and after.output_generation == before.output_generation + 1 and changed.stop_acks == waiting.stop_acks + 1 and after.idle_calls == changed.idle_calls and after.sid_stop_calls == before.sid_stop_calls + 1;
    }
    fn sourceChange(self: *const Probe) bool {
        if (!self.controls(.{}) or self.audio.sidAcquire() != 1) return false;
        defer { _ = self.controls(.{}); _ = self.audio.sidStop(1); _ = self.audio.sidRelease(1); }
        if (self.play() != 0 or !self.controls(.{ .idle_result = a.service_api_result_busy })) return false;
        const before = self.snapshot() orelse return false;
        const first = self.audio.sidStop(1);
        if (!self.controls(.{ .idle_result = a.service_api_result_busy, .replace_requested = 1 })) return false;
        _ = self.dev.performanceSummary() orelse return false;
        const replacement = self.snapshot() orelse return false;
        const second = self.audio.sidStop(1);
        if (!self.controls(.{})) return false;
        const finished = self.audio.sidStop(1);
        const after = self.snapshot() orelse return false;
        const released = self.audio.sidRelease(1);
        if (!self.controls(.{ .replace_requested = 1 })) return false;
        _ = self.dev.performanceSummary() orelse return false;
        const renewed = self.snapshot() orelse return false;
        const acquired = self.audio.sidAcquire();
        const played = self.play();
        const stopped = self.audio.sidStop(1);
        const closed = self.audio.sidRelease(1);
        const final = self.snapshot() orelse return false;
        self.report("SID52 SOURCE first={d} second={d} final={d} live_registrations={d} live_result={d} private_calls={d} private_acks={d} idle_acks={d} release={d} renewed_result={d} renewed_registrations={d} acquire={d} play={d} stop={d} close={d} final_private_acks={d} final_idle_acks={d}", .{ first, second, finished, replacement.synth_registrations - before.synth_registrations, replacement.replace_result, after.sid_stop_calls - before.sid_stop_calls, after.sid_stop_acks - before.sid_stop_acks, after.idle_acks - before.idle_acks, released, renewed.replace_result, renewed.synth_registrations - before.synth_registrations, acquired, played, stopped, closed, final.sid_stop_acks - before.sid_stop_acks, final.idle_acks - before.idle_acks });
        return first == a.service_api_result_busy and second == a.service_api_result_busy and finished == 0 and replacement.replace_result == a.service_api_result_busy and replacement.synth_registrations == before.synth_registrations and after.sid_stop_calls == before.sid_stop_calls + 1 and after.sid_stop_acks == before.sid_stop_acks + 1 and after.idle_acks == before.idle_acks + 1 and released == 0 and renewed.replace_result == 0 and renewed.synth_registrations == before.synth_registrations + 1 and acquired == 1 and played == 0 and stopped == 0 and closed == 0 and final.sid_stop_acks == before.sid_stop_acks + 2 and final.idle_acks == before.idle_acks + 2;
    }
    fn last(self: *const Probe, failure: bool, resume_source: bool) bool {
        if (!self.controls(.{})) return false;
        if (self.audio.sidAcquire() != 1) return false;
        defer { _ = self.controls(.{}); _ = self.audio.sidStop(1); _ = self.audio.sidRelease(1); }
        if (self.play() != 0) return false;
        const before = self.snapshot() orelse return false;
        const expected: i32 = if (failure) -66 else a.service_api_result_busy;
        if (!self.controls(.{ .idle_result = expected })) return false;
        const first = self.audio.sidStop(1);
        const invalid = if (resume_source) self.audio.sidWriteRegister(1, 25, 0x11) else @as(i32, -2);
        const second = self.audio.sidStop(1);
        const waiting = self.snapshot() orelse return false;
        var resumed: i32 = 0;
        if (resume_source) {
            if (self.audio.sidWriteRegister(1, 4, 0x11) != 0) return false;
            resumed = self.audio.sidStop(1);
        }
        if (!self.controls(.{})) return false;
        const finished = self.audio.sidStop(1);
        const done = self.snapshot() orelse return false;
        const replayed = self.audio.sidStop(1);
        const replay = self.snapshot() orelse return false;
        const own: u64 = if (resume_source) 2 else 1;
        self.report("SID52 LAST error={d} resume={d} first={d} second={d} resumed={d} final={d} replay={d} private_waiting={d} private_done={d} idle_acks={d} replay_idle_delta={d} invalid={d}", .{ @intFromBool(failure), @intFromBool(resume_source), first, second, resumed, finished, replayed, waiting.sid_stop_calls - before.sid_stop_calls, done.sid_stop_calls - before.sid_stop_calls, done.idle_acks - before.idle_acks, replay.idle_calls - done.idle_calls, invalid });
        return first == expected and second == expected and waiting.sid_stop_calls == before.sid_stop_calls + 1 and waiting.idle_acks == before.idle_acks and invalid == -2 and (!resume_source or resumed == expected) and finished == 0 and replayed == 0 and done.sid_stop_calls == before.sid_stop_calls + own and done.sid_stop_acks == before.sid_stop_acks + own and done.idle_acks == before.idle_acks + 1 and replay.idle_calls == done.idle_calls;
    }
};
pub fn r4_app_main(app: *r4os.App) i32 {
    const probe = Probe{ .sys = app.system(), .audio = app.audioLowLevel() orelse return 2, .dev = app.devicesLowLevel() orelse return 3 };
    var input: a.ProtocolBuffer = .{};
    var output: a.ProtocolBuffer = .{};
    if (probe.dev.protocolDispatch(p.role, p.ping, &input, &output) != 0) return 4;
    _ = probe.dev.performanceSummary() orelse return 5;
    _ = probe.snapshot() orelse { probe.sys.println("SID52 fixture observer unavailable"); return 6; };
    const args = app.args(); const mode: u8 = if (args.len == 0) 'S' else std.ascii.toUpper(args[0]);
    const ok = switch (mode) { 'S' => probe.shared(), 'P' => probe.pending(), 'I' => probe.last(false, false), 'E' => probe.last(true, false), 'N' => probe.last(false, true), 'O' => probe.outputChange(), 'R' => probe.sourceChange(), 'M' => probe.midiParallel(), else => false };
    probe.report("SID52 result={s} mode={c}; real Kernel callback/stream ownership, controlled provider; no DMA/speaker/isolated latency proof", .{ if (ok) @as([]const u8, "PASS") else @as([]const u8, "FAIL"), mode });
    return if (ok) 0 else 1;
}
