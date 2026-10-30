const std = @import("std");
const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var line: [768]u8 = undefined;
var text: [4096]u8 = undefined;
var worker_sys: r4os.r4sys.Context = undefined;
var worker_audio: r4os.r4audio.Context = undefined;
var workers_go = false;
var workers_midi = false;
var results: [2]i32 = .{ -99, -99 };
fn worker(index: u64) callconv(.c) i32 {
    while (!@atomicLoad(bool, &workers_go, .acquire)) worker_sys.sleepTicks(1);
    const until = worker_sys.ticks() +| worker_sys.ticksFromMilliseconds(500);
    var rc: i32 = 0;
    while (true) {
        rc = if (workers_midi) worker_audio.midiOpenSynth("OPL3") else worker_audio.sidAcquire();
        if (rc != a.service_api_result_busy or worker_sys.ticks() >= until) break;
        worker_sys.sleepTicks(1);
    }
    @atomicStore(i32, &results[@intCast(index)], rc, .release);
    return rc;
}
const Child = struct { handle: a.ProgramProcessHandle, observation: a.ProgramObservationHandle };
const Bridge = *const fn (u32, u32, u64, u64) callconv(.c) i64;
const Probe = struct {
    sys: r4os.r4sys.Context,
    audio: r4os.r4audio.Context,
    dev: r4os.r4dev.Context,
    bridge: ?Bridge = null,
    fn internal(self: *const Probe, op: u32, id: u32, generation: u64, value: u64) i64 {
        return (self.bridge orelse return -998)(op, id, generation, value);
    }
    fn report(self: *const Probe, comptime format: []const u8, values: anytype) void {
        self.sys.println(std.fmt.bufPrint(&line, format, values) catch "P53 controlled report overflow");
    }
    fn controls(self: *const Probe, value: p.Controls) bool {
        var copy = value;
        var input: a.ProtocolBuffer = .{ .data = &copy, .len = @sizeOf(p.Controls), .capacity = @sizeOf(p.Controls) };
        var out: a.ProtocolBuffer = .{};
        return self.dev.protocolDispatch(p.role, p.configure, &input, &out) == 0;
    }
    fn snapshot(self: *const Probe) ?p.State {
        var result: p.State = .{};
        var input: a.ProtocolBuffer = .{};
        var out: a.ProtocolBuffer = .{ .data = &result, .capacity = @sizeOf(p.State) };
        if (self.dev.protocolDispatch(p.role, p.snapshot, &input, &out) != 0 or out.len != @sizeOf(p.State) or result.magic != p.magic) return null;
        return result;
    }
    fn release(self: *const Probe, sid: bool) i32 {
        const until = self.sys.ticks() +| self.sys.ticksFromMilliseconds(3000);
        while (true) {
            const rc = if (sid) self.audio.sidRelease(1) else self.audio.midiClose(1);
            if (rc != a.service_api_result_busy or self.sys.ticks() >= until) return rc;
            self.sys.sleepTicks(1);
        }
    }
    fn play(self: *const Probe) i32 {
        const until = self.sys.ticks() +| self.sys.ticksFromMilliseconds(3000);
        while (true) {
            const rc = self.audio.sidPlayFrame(1, 0x1020, 50);
            if (rc != a.service_api_result_busy or self.sys.ticks() >= until) return rc;
            self.sys.sleepTicks(1);
        }
    }
    fn spawn(self: *const Probe, sid: bool) ?Child {
        var c: Child = .{ .handle = .{}, .observation = .{} };
        const arg = [_]u8{ if (sid) 'S' else 'M', 'E', 0 };
        const spawned = self.sys.programSpawnWithConsoleHostHandle("C:\\TEMP\\P53CH.R4X", @ptrCast(&arg), .console, .terminal_window, &c.handle);
        self.report("P53 SPAWN kind={s} result={d} child={d}:{d}", .{ if (sid) @as([]const u8, "SID") else @as([]const u8, "MIDI"), spawned, c.handle.instance_id, c.handle.generation });
        if (spawned != 0) return null;
        if (self.sys.programObservationRetain(&c.handle, 0, &c.observation) != 0) {
            _ = self.sys.programHandleKill(&c.handle);
            return null;
        }
        return c;
    }
    fn finish(self: *const Probe, c: Child) bool {
        var done: a.ProgramProcessCompletion = .{};
        const waited = self.sys.programObservationWait(&c.observation, self.sys.ticksFromMilliseconds(5000), &done);
        var count: u32 = 0;
        const read = if (waited == 0) self.sys.programObservationRead(&c.observation, 0, &text, &count) else a.program_handle_error_timeout;
        const reaped = self.sys.programHandleReap(&c.handle, &done);
        const released = self.sys.programObservationRelease(&c.observation);
        self.report("P53 CONTROLLED_CHILD child={d}:{d} wait={d} exit={d} read={d} bytes={d} length={d} flags={d} reap={d} release={d}", .{ c.handle.instance_id, c.handle.generation, waited, done.exit_code, read, count, done.output_length, done.flags, reaped, released });
        if (read == 0) self.sys.write(text[0..@min(count, text.len)]);
        return waited == 0 and done.exit_code == 0 and read == 0 and count == done.output_length and reaped == 0 and released == 0 and done.handle.generation == c.handle.generation and done.handle.instance_id == c.handle.instance_id and done.flags & (a.program_completion_flag_output_incomplete | a.program_completion_flag_output_truncated) == 0;
    }
    fn held(self: *const Probe, c: Child, sid: bool) bool {
        var done: a.ProgramProcessCompletion = .{};
        const wait = self.sys.programObservationWait(&c.observation, 0, &done);
        const until = self.sys.ticks() +| self.sys.ticksFromMilliseconds(500);
        var acquire: i32 = 0;
        while (true) {
            acquire = if (sid) self.audio.sidAcquire() else self.audio.midiOpenSynth("OPL3");
            if (acquire != a.service_api_result_busy or self.sys.ticks() >= until) break;
            self.sys.sleepTicks(1);
        }
        self.report("P53 RETAINED child={d}:{d} wait={d} competing_acquire={d}", .{ c.handle.instance_id, c.handle.generation, wait, acquire });
        if (acquire > 0) {
            _ = self.controls(.{});
            _ = self.release(sid);
            return false;
        }
        return wait == a.program_handle_error_timeout and acquire == (if (sid) @as(i32, -2) else @as(i32, -3));
    }
    fn awaitCount(self: *const Probe, comptime field: []const u8, minimum: u64) ?p.State {
        const until = self.sys.ticks() +| self.sys.ticksFromMilliseconds(3000);
        while (true) {
            const state = self.snapshot() orelse return null;
            if (@field(state, field) >= minimum) return state;
            if (self.sys.ticks() >= until) return null;
            self.sys.sleepTicks(1);
        }
    }
    fn pending(self: *const Probe) bool {
        const configured = self.controls(.{ .write_result = a.service_api_result_busy });
        const acquired = self.audio.sidAcquire();
        self.report("P53 PENDING_INITIAL controls={d} acquire={d}", .{ @intFromBool(configured), acquired });
        if (!configured or acquired != 1) return false;
        defer {
            _ = self.controls(.{});
            _ = self.release(true);
        }
        const played = self.audio.sidPlayFrame(1, 0x1020, 50);
        const before = self.snapshot() orelse return false;
        if (!self.controls(.{ .write_result = a.service_api_result_busy, .sid_stop_result = -55 })) return false;
        const failed = self.audio.sidRelease(1);
        const rejected = self.snapshot() orelse return false;
        if (!self.controls(.{})) return false;
        const retry = self.play();
        const resumed = self.snapshot() orelse return false;
        const released = self.release(true);
        const after = self.snapshot() orelse return false;
        self.report("P53 PENDING play={d} release_error={d} retry={d} release={d} same_render={d} bytes={d} failed_release_callbacks={d} failed_idle={d} private_stop_acks={d} release_acks={d} callback_scope_checks={d}", .{ played, failed, retry, released, @intFromBool(resumed.sid_renders == before.sid_renders), resumed.bytes - before.bytes, rejected.sid_releases - before.sid_releases, rejected.idle_calls - before.idle_calls, after.sid_stop_acks - before.sid_stop_acks, after.sid_release_acks - before.sid_release_acks, after.callback_scope_checks });
        return played == a.service_api_result_busy and failed == -55 and retry == 0 and released == 0 and resumed.sid_renders == before.sid_renders and resumed.sid_plays == before.sid_plays and resumed.bytes == before.bytes + 3840 and rejected.sid_releases == before.sid_releases and rejected.idle_calls == before.idle_calls and after.sid_stop_acks == before.sid_stop_acks + 1 and after.sid_release_acks == before.sid_release_acks + 1;
    }
    fn reaper(self: *const Probe, sid: bool) bool {
        var control: p.Controls = .{ .sid_stop_result = if (sid) -55 else 0, .midi_stop_result = if (sid) 0 else -55, .sid_release_result = if (sid) -66 else 0, .idle_result = a.service_api_result_busy };
        if (!self.controls(control)) return false;
        const before = self.snapshot() orelse return false;
        const child = self.spawn(sid) orelse return false;
        var finished = false;
        defer {
            _ = self.controls(.{});
            if (!finished) {
                _ = self.sys.programHandleKill(&child.handle);
                _ = self.finish(child);
            }
        }
        const first = if (sid) self.awaitCount("sid_stop_calls", before.sid_stop_calls + 1) else self.awaitCount("midi_stops", before.midi_stops + 1);
        if (first == null or !self.held(child, sid)) return false;
        control.sid_stop_result = 0;
        control.midi_stop_result = 0;
        if (!self.controls(control)) return false;
        _ = self.awaitCount("idle_calls", before.idle_calls + 1) orelse return false;
        if (!self.held(child, sid)) return false;
        control.idle_result = -77;
        if (!self.controls(control)) return false;
        const errors = self.awaitCount("idle_calls", before.idle_calls + 3) orelse return false;
        if (!self.held(child, sid)) return false;
        const own_ack = if (sid) errors.sid_stop_acks - before.sid_stop_acks else errors.midi_stop_acks - before.midi_stop_acks;
        if (own_ack != 1) return false;
        control.idle_result = 0;
        if (!self.controls(control)) return false;
        var release_error: bool = false;
        if (sid) {
            _ = self.awaitCount("sid_releases", before.sid_releases + 1) orelse return false;
            release_error = self.held(child, true);
            if (!release_error) return false;
        }
        if (!self.controls(.{})) return false;
        finished = self.finish(child);
        if (!finished) return false;
        const after = self.snapshot() orelse return false;
        const next = if (sid) self.audio.sidAcquire() else self.audio.midiOpenSynth("OPL3");
        const closed = if (next == 1) self.release(sid) else @as(i32, -99);
        self.report("P53 REAPER kind={s} private_acks={d} idle_acks={d} release_acks={d} release_error_retained={d} next_acquire={d} close={d} callback_scope_checks={d}", .{ if (sid) @as([]const u8, "SID") else @as([]const u8, "MIDI"), if (sid) after.sid_stop_acks - before.sid_stop_acks else after.midi_stop_acks - before.midi_stop_acks, after.idle_acks - before.idle_acks, after.sid_release_acks - before.sid_release_acks, @intFromBool(release_error), next, closed, after.callback_scope_checks });
        return (if (sid) after.sid_stop_acks - before.sid_stop_acks else after.midi_stop_acks - before.midi_stop_acks) == 1 and after.idle_acks == before.idle_acks + 1 and (!sid or after.sid_release_acks == before.sid_release_acks + 1) and next == 1 and closed == 0;
    }
    fn replacement(self: *const Probe) bool {
        if (!self.controls(.{}) or self.audio.sidAcquire() != 1) return false;
        defer {
            _ = self.controls(.{});
            _ = self.release(true);
        }
        const before = self.snapshot() orelse return false;
        if (!self.controls(.{ .replace_requested = 1 })) return false;
        _ = self.dev.performanceSummary() orelse return false;
        const denied = self.snapshot() orelse return false;
        const write = self.audio.sidWriteRegister(1, 4, 0x11);
        const own_release = self.release(true);
        if (!self.controls(.{ .replace_requested = 1 })) return false;
        _ = self.dev.performanceSummary() orelse return false;
        const accepted = self.snapshot() orelse return false;
        const stale = self.audio.sidWriteRegister(1, 4, 0x11);
        const acquired = self.audio.sidAcquire();
        const fresh = if (acquired == 1) self.audio.sidWriteRegister(1, 4, 0x11) else @as(i32, -99);
        const closed = self.release(true);
        self.report("P53 REPLACE active_result={d} active_registrations={d} own_write={d} own_release={d} released_result={d} new_registrations={d} stale={d} acquire={d} fresh={d} close={d}", .{ denied.replace_result, denied.synth_registrations - before.synth_registrations, write, own_release, accepted.replace_result, accepted.synth_registrations - before.synth_registrations, stale, acquired, fresh, closed });
        return denied.replace_result == a.service_api_result_busy and denied.synth_registrations == before.synth_registrations and write == 0 and own_release == 0 and accepted.replace_result == 0 and accepted.synth_registrations == before.synth_registrations + 1 and stale < 0 and acquired == 1 and fresh == 0 and closed == 0;
    }
    fn threads(self: *const Probe, midi: bool) bool {
        if (!self.controls(.{})) return false;
        const before = self.snapshot() orelse return false;
        worker_sys = self.sys;
        worker_audio = self.audio;
        workers_midi = midi;
        @atomicStore(bool, &workers_go, false, .release);
        for (&results) |*r| @atomicStore(i32, r, -99, .release);
        var first: a.ProgramJoinHandle = .{};
        var second: a.ProgramJoinHandle = .{};
        const one = self.sys.threadCreateHandle(worker, 0, 128 * 1024, 0, &first);
        if (one != a.thread_ok) return false;
        const two = self.sys.threadCreateHandle(worker, 1, 128 * 1024, 0, &second);
        @atomicStore(bool, &workers_go, true, .release);
        var e1: i32 = -99;
        var e2: i32 = -99;
        const j1 = self.sys.threadHandleJoin(&first, self.sys.ticksFromMilliseconds(3000), &e1);
        const j2 = if (two == a.thread_ok) self.sys.threadHandleJoin(&second, self.sys.ticksFromMilliseconds(3000), &e2) else two;
        const r1 = @atomicLoad(i32, &results[0], .acquire);
        const r2 = @atomicLoad(i32, &results[1], .acquire);
        const after = self.snapshot() orelse return false;
        const write = if (midi) self.audio.midiSend(1, 0, 0x90, 60, 80) else self.audio.sidWriteRegister(1, 4, 0x11);
        const closed = self.release(!midi);
        const denied: i32 = if (midi) -3 else -2;
        self.report("P53 THREADS kind={s} first={d} second={d} joins={d}/{d} exits={d}/{d} contexts={d}:{d}/{d}:{d} same_process={d} private_acquires={d} callback_scope={d} main_write={d} close={d}", .{ if (midi) @as([]const u8, "MIDI") else @as([]const u8, "SID"), r1, r2, j1, j2, e1, e2, first.thread_id, first.thread_generation, second.thread_id, second.thread_generation, @intFromBool(first.instance_id == second.instance_id and first.instance_generation == second.instance_generation), after.sid_acquires - before.sid_acquires, after.callback_scope_checks - before.callback_scope_checks, write, closed });
        return two == a.thread_ok and j1 == a.thread_ok and j2 == a.thread_ok and ((r1 == 1 and r2 == denied) or (r2 == 1 and r1 == denied)) and e1 == r1 and e2 == r2 and first.instance_id == second.instance_id and first.instance_generation == second.instance_generation and (!midi or after.sid_acquires == before.sid_acquires) and (midi or after.sid_acquires == before.sid_acquires + 1) and write == 0 and closed == 0;
    }
    fn generations(self: *const Probe, midi: bool) bool {
        if (self.internal(0, 0, 0, 0) != 0x53595353) return false;
        const id: u32 = @intCast(self.internal(1, 0, 0, 0));
        const generation: u64 = @intCast(self.internal(2, 0, 0, 0));
        const acquired = if (midi) self.audio.midiOpenSynth("OPL3") else self.audio.sidAcquire();
        if (acquired != 1) return false;
        defer _ = self.release(!midi);
        const foreign_write = self.internal(if (midi) 13 else 10, id, generation + 1, 0);
        const foreign_release = self.internal(if (midi) 14 else 11, id, generation + 1, 0);
        const stale_cleanup = self.internal(12, id, generation + 1, 0);
        const own = self.internal(if (midi) 13 else 10, id, generation, 0);
        const kernel_foreign = self.internal(if (midi) 13 else 10, 0, 0, 0);
        const closed = self.release(!midi);
        const kernel_acquire = self.internal(if (midi) 16 else 15, 0, 0, 0);
        const app_foreign = if (midi) self.audio.midiSend(1, 0, 0x90, 61, 80) else self.audio.sidWriteRegister(1, 4, 0x11);
        const kernel_cleanup = self.internal(12, 0, 0, 0);
        const kernel_release = self.internal(if (midi) 14 else 11, 0, 0, 0);
        const next = if (midi) self.audio.midiOpenSynth("OPL3") else self.audio.sidAcquire();
        const final = self.release(!midi);
        self.report("P53 GENERATIONS kind={s} owner={d}:{d} acquired={d} stale_write={d} stale_release={d} stale_cleanup={d} own={d} kernel_foreign={d} close={d} kernel_acquire={d} app_foreign={d} kernel_cleanup={d} kernel_release={d} next={d} final={d}", .{ if (midi) @as([]const u8, "MIDI") else @as([]const u8, "SID"), id, generation, acquired, foreign_write, foreign_release, stale_cleanup, own, kernel_foreign, closed, kernel_acquire, app_foreign, kernel_cleanup, kernel_release, next, final });
        return acquired == 1 and foreign_write == a.service_api_result_invalid and foreign_release == a.service_api_result_invalid and stale_cleanup == 1 and own == 0 and kernel_foreign == a.service_api_result_invalid and closed == 0 and kernel_acquire == 1 and app_foreign == a.service_api_result_invalid and kernel_cleanup == 0 and kernel_release == 0 and next == 1 and final == 0;
    }
    fn midiWhileSid(self: *const Probe) bool {
        if (!self.controls(.{}) or self.audio.sidAcquire() != 1) return false;
        defer _ = self.release(true);
        if (self.audio.midiOpenSynth("OPL3") != 1) return false;
        defer _ = self.release(false);
        const midi = self.audio.midiSend(1, 0, 0x90, 60, 90);
        const sid = self.play();
        const rendered = self.audio.opl3RenderBlock();
        const perf = self.dev.performanceSummary() orelse return false;
        const before = self.snapshot() orelse return false;
        const closed = self.release(false);
        const after = self.snapshot() orelse return false;
        const continued = self.play();
        const final = self.release(true);
        const end = self.snapshot() orelse return false;
        self.report("P53 MIDI_WHILE_SID core_open={d} midi={d} sid={d} render={d} close={d} idle={d} stop={d} sid_stop={d} midi_stop_acks={d} continued={d} sid_renders={d} sid_release={d} final_idle_acks={d}", .{ perf.audio_open_streams, midi, sid, rendered, closed, after.idle_calls - before.idle_calls, after.stop_calls - before.stop_calls, after.sid_stop_calls - before.sid_stop_calls, after.midi_stop_acks - before.midi_stop_acks, continued, end.sid_renders - after.sid_renders, final, end.idle_acks - after.idle_acks });
        return perf.audio_open_streams == 0 and midi == 0 and sid == 0 and rendered == 0 and closed == 0 and after.idle_calls == before.idle_calls and after.stop_calls == before.stop_calls and after.sid_stop_calls == before.sid_stop_calls and after.midi_stop_acks == before.midi_stop_acks + 1 and continued == 0 and end.sid_renders == after.sid_renders + 1 and final == 0 and end.idle_acks == after.idle_acks + 1;
    }
    fn lifecycle(self: *const Probe, midi: bool) bool {
        if (self.internal(0, 0, 0, 0) != 0x53595353 or !self.controls(.{})) return false;
        const owner: u32 = @intCast(self.internal(3, 0, 0, 0));
        const before = self.internal(4, owner, 0, 0);
        const epoch = self.internal(9, owner, 0, 0);
        const registration = self.internal(if (midi) 8 else 7, 0, 0, 0);
        const acquired = if (midi) self.audio.midiOpenSynth("OPL3") else self.audio.sidAcquire();
        if (acquired != 1) return false;
        defer _ = self.release(!midi);
        const closing_veto = self.internal(20, owner, 0, 1);
        const unregister_veto = self.internal(21, owner, 0, 0);
        const unload_veto = self.internal(22, 0, 0, 0);
        const retained = self.internal(5, owner, 0, 0);
        const same = self.internal(if (midi) 8 else 7, 0, 0, 0);
        const write = if (midi) self.audio.midiSend(1, 0, 0x90, 60, 80) else self.audio.sidWriteRegister(1, 4, 0x11);
        const closed = self.release(!midi);
        const closing = self.internal(20, owner, 0, 1);
        const refused = if (midi) self.audio.midiOpenSynth("OPL3") else self.audio.sidAcquire();
        const restored = self.internal(20, owner, 0, 0);
        const unload = self.internal(22, 0, 0, 0);
        const absent = self.internal(5, owner, 0, 0);
        const stale_heap = self.internal(6, owner, 0, 0);
        const missing = self.internal(if (midi) 8 else 7, 0, 0, 0);
        const loaded = self.internal(23, 0, 0, 0);
        const next_owner: u32 = @intCast(self.internal(3, 0, 0, 0));
        const after = self.internal(4, next_owner, 0, 0);
        const next_epoch = self.internal(9, next_owner, 0, 0);
        const next_registration = self.internal(if (midi) 8 else 7, 0, 0, 0);
        _ = self.dev.performanceSummary() orelse return false;
        const next = if (midi) self.audio.midiOpenSynth("OPL3") else self.audio.sidAcquire();
        const final = self.release(!midi);
        self.report("P53 LIFECYCLE kind={s} owner={d}/{d} driver_generation={d}/{d} epoch={d}/{d} registration={d}/{d} acquire={d} vetoes={d}/{d}/{d} retained={d} same={d} write={d} close={d} closing={d} refused={d} restored={d} unload={d} absent={d} stale_heap={d} missing={d} load={d} next={d} final={d}", .{ if (midi) @as([]const u8, "MIDI") else @as([]const u8, "SID"), owner, next_owner, before, after, epoch, next_epoch, registration, next_registration, acquired, closing_veto, unregister_veto, unload_veto, retained, same, write, closed, closing, refused, restored, unload, absent, stale_heap, missing, loaded, next, final });
        return before > 0 and after > before and epoch > 0 and next_epoch > epoch and registration > 0 and next_registration > registration and acquired == 1 and closing_veto == 0 and unregister_veto == 0 and unload_veto == 0 and retained == 2 and same == registration and write == 0 and closed == 0 and closing == 1 and refused < 0 and restored == 1 and unload == 1 and absent == 0 and stale_heap == a.driver_heap_error_stale and missing == 0 and loaded == 1 and next == 1 and final == 0;
    }
};
pub fn r4_app_main(app: *r4os.App) i32 {
    _ = app.desktop() orelse return 8;
    const args = app.args();
    const address = if (args.len > 2 and args[1] == ':') std.fmt.parseInt(usize, args[2..], 16) catch return 9 else 0;
    const probe = Probe{ .sys = app.system(), .audio = app.audioLowLevel() orelse return 2, .dev = app.devicesLowLevel() orelse return 3, .bridge = if (address == 0) null else @ptrFromInt(address) };
    var input: a.ProtocolBuffer = .{};
    var out: a.ProtocolBuffer = .{};
    if (probe.dev.protocolDispatch(p.role, p.ping, &input, &out) != 0) return 4;
    _ = probe.dev.performanceSummary() orelse return 5;
    _ = probe.snapshot() orelse return 6;
    defer _ = probe.controls(.{});
    if (args.len == 0) return 7;
    const mode = args[0];
    const ok = switch (mode) {
        'P' => probe.pending(),
        'S' => probe.reaper(true),
        'M' => probe.reaper(false),
        'R' => probe.replacement(),
        'T' => probe.threads(false),
        'U' => probe.threads(true),
        'C' => probe.midiWhileSid(),
        'G' => probe.generations(false),
        'H' => probe.generations(true),
        'L' => probe.lifecycle(false),
        'V' => probe.lifecycle(true),
        'W' => blk: {
            const unload = probe.internal(22, 0, 0, 0);
            const load = probe.internal(23, 0, 0, 52);
            probe.report("P53 SWITCH_52 unload={d} load={d}", .{ unload, load });
            break :blk unload == 1 and load == 1;
        },
        else => false,
    };
    probe.report("P53 CONTROLLED result={s} mode={c}; actual Core/private callbacks/Program reaper/thread execution; no DMA/speaker claim", .{ if (ok) @as([]const u8, "PASS") else @as([]const u8, "FAIL"), mode });
    return if (ok) 0 else 1;
}
