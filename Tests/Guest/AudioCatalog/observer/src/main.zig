const std = @import("std");
const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var line: [768]u8 = undefined;
var rows: [257]a.AudioOutputInfo = @splat(.{});
var receipt: a.AudioOutputCatalog = .{};
var alias_storage: [257 * @sizeOf(a.AudioOutputInfo) + @sizeOf(a.AudioOutputCatalog)]u8 align(8) = @splat(0);
var pcm_audio: r4os.r4audio.Context = undefined;
var pcm_sys: r4os.r4sys.Context = undefined;
var pcm_stop: bool = false;
var pcm_bytes: u64 = 0;
var pcm_writes: u64 = 0;
fn pcmThread(_: u64) callconv(.c) i32 {
    const stream = pcm_audio.audioOpenStream(48000, 2, .s16le);
    if (stream < 0) return stream;
    const buffer: [1920]u8 = @splat(0);
    while (!@atomicLoad(bool, &pcm_stop, .acquire)) {
        const n = pcm_audio.audioWrite(@intCast(stream), &buffer);
        if (n > 0) {
            _ = @atomicRmw(u64, &pcm_bytes, .Add, @intCast(n), .acq_rel);
            _ = @atomicRmw(u64, &pcm_writes, .Add, 1, .acq_rel);
        } else if (n != 0 and n != a.service_api_result_busy) return n;
        pcm_sys.sleepTicks(@max(pcm_sys.ticksFromMilliseconds(5), 1));
    }
    return pcm_audio.audioClose(@intCast(stream));
}
const Probe = struct {
    sys: r4os.r4sys.Context,
    audio: r4os.r4audio.Context,
    dev: r4os.r4dev.Context,
    fn report(self: *const Probe, comptime format: []const u8, args: anytype) void {
        self.sys.println(std.fmt.bufPrint(&line, format, args) catch "P55 report overflow");
    }
    fn control(self: *const Probe, value: p.Controls) bool {
        var copy = value;
        var data: a.ProtocolBuffer = .{ .data = &copy, .len = @sizeOf(p.Controls), .capacity = @sizeOf(p.Controls) };
        var out: a.ProtocolBuffer = .{};
        for (0..100) |_| {
            const rc = self.dev.protocolDispatch(p.role, p.configure, &data, &out);
            if (rc == 0) return true;
            if (rc != a.service_api_result_busy) return false;
            self.sys.sleepTicks(1);
        }
        return false;
    }
    fn snapshot(self: *const Probe) ?p.State {
        var result: p.State = .{};
        var data: a.ProtocolBuffer = .{};
        var out: a.ProtocolBuffer = .{ .data = &result, .capacity = @sizeOf(p.State) };
        if (self.dev.protocolDispatch(p.role, p.snapshot, &data, &out) != 0 or out.len != @sizeOf(p.State) or result.magic != p.magic) return null;
        return result;
    }
    fn collect(self: *const Probe, count: u32) bool {
        var i: u32 = 0;
        while (i <= count) : (i += 1) {
            const got = self.audio.audioOutputInfo(i, &rows[i]);
            if (got != (if (i < count) @as(i32, 1) else @as(i32, 0))) {
                self.report("P55 BAD_QUERY index={d} result={d}", .{ i, got });
                return false;
            }
        }
        return true;
    }
    fn scalar(self: *const Probe, count: u32, queries: u64, actives: u64) bool {
        var id: [64]u8 = @splat(0); @memcpy(id[0..5], "P55-0");
        if (self.audio.audioSelectOutput(&id) != 0) return false;
        if (!self.control(.{ .output_count = count })) return false;
        if (!self.collect(count)) return false;
        const state = self.snapshot() orelse return false;
        self.report("P55 SCALAR count={d} queries={d} actives={d} generation={d} measurement={s}; actual Core callbacks", .{ count, state.queries, state.actives, state.generation, if (state.queries == queries and state.actives == actives) @as([]const u8, "EXACT") else @as([]const u8, "UNQUALIFIED") });
        return state.queries == queries and state.actives == actives;
    }
    fn generation(name: *const [64]u8) ?u64 {
        const end = std.mem.indexOfScalar(u8, name, '/') orelse return null;
        if (!std.mem.startsWith(u8, name, "generation:")) return null;
        return std.fmt.parseInt(u64, name[11..end], 10) catch null;
    }
    fn capture(self: *const Probe, n: u32, expected_queries: u64) bool {
        if (!self.control(.{ .output_count = n })) return false;
        rows[256].preferred_rate = 0x55555555;
        receipt = .{};
        const result = self.audio.audioOutputCatalog(&rows, &receipt);
        const observed = self.snapshot() orelse return false;
        if (result != 1 or receipt.count != n or receipt.provider_count != 2 or
            receipt.providers[0].generation != observed.generation or receipt.providers[0].count != n or
            observed.queries != expected_queries or observed.actives != 0 or observed.epochs != 2 or
            observed.secondary_queries != 1 or observed.secondary_epochs != 2 or rows[256].preferred_rate != 0x55555555) return false;
        for (rows[0..n]) |row| if (generation(&row.name) != observed.generation) return false;
        self.report("P55 BULK count={d} queries={d} actives={d} epochs={d} secondaryQueries={d} secondaryEpochs={d} generation={d} registry={d} registration={d} result=PASS", .{ n, observed.queries, observed.actives, observed.epochs, observed.secondary_queries, observed.secondary_epochs, observed.generation, receipt.registry_generation, receipt.providers[0].registration });
        return true;
    }
    fn failedCapture(self: *const Probe, controls: p.Controls, expected: i32, label: []const u8) bool {
        if (!self.control(controls)) return false;
        receipt = .{ .registry_generation = 0x1234_5678_0000_0055 };
        const before = receipt;
        rows[256].preferred_rate = 0x55555555;
        const result = self.audio.audioOutputCatalog(&rows, &receipt);
        const observed = self.snapshot() orelse return false;
        const ok = result == expected and std.mem.eql(u8, std.mem.asBytes(&before), std.mem.asBytes(&receipt)) and rows[256].preferred_rate == 0x55555555;
        self.report("P55 REJECT case={s} result={d} queries={d} epochs={d} secondaryQueries={d} receiptUntouched={d} pass={d}", .{ label, result, observed.queries, observed.epochs, observed.secondary_queries, @intFromBool(std.mem.eql(u8, std.mem.asBytes(&before), std.mem.asBytes(&receipt))), @intFromBool(ok) });
        return ok;
    }
    fn reentrantRegistry(self: *const Probe) bool {
        if (!self.control(.{ .output_count = 2, .reentrant_at_query = 1 })) return false;
        receipt = .{};
        if (self.audio.audioOutputCatalog(&rows, &receipt) != 1) return false;
        const observed = self.snapshot() orelse return false;
        const ok = observed.reentrant_attempts == 1 and observed.reentrant_result == a.service_api_result_busy and receipt.count == 2;
        self.report("P55 REGISTRY_DURING_CAPTURE attempts={d} unregister={d} count={d} registry={d} registration={d} pass={d}", .{ observed.reentrant_attempts, observed.reentrant_result, receipt.count, receipt.registry_generation, receipt.providers[0].registration, @intFromBool(ok) });
        return ok;
    }
    fn rebind(self: *const Probe) bool {
        if (!self.control(.{ .output_count = 2 })) return false;
        receipt = .{};
        if (self.audio.audioOutputCatalog(&rows, &receipt) != 1) return false;
        const before = receipt;
        const original = rows[0];
        var data: a.ProtocolBuffer = .{}; var out: a.ProtocolBuffer = .{};
        if (self.dev.protocolDispatch(p.role, p.rebind, &data, &out) != 0) return false;
        var seen: p.State = self.snapshot() orelse return false;
        const deadline = self.sys.ticks() +| self.sys.ticksFromMilliseconds(3000);
        while (seen.rebind_completed != 1 and self.sys.ticks() < deadline) {
            self.sys.sleepTicks(1); seen = self.snapshot() orelse return false;
        }
        if (seen.rebind_completed != 1 or seen.rebind_result != 0) return false;
        receipt = .{};
        if (self.audio.audioOutputCatalog(&rows, &receipt) != 1) return false;
        const ok = receipt.registry_generation > before.registry_generation and receipt.providers[0].registration != before.providers[0].registration and std.mem.eql(u8, &original.id, &rows[0].id) and std.mem.eql(u8, &original.name, &rows[0].name);
        self.report("P55 REGISTRY_REBIND oldRegistry={d} newRegistry={d} oldRegistration={d} newRegistration={d} samePersistentID={d} workResult={d} pass={d}", .{ before.registry_generation, receipt.registry_generation, before.providers[0].registration, receipt.providers[0].registration, @intFromBool(std.mem.eql(u8, &original.id, &rows[0].id)), seen.rebind_result, @intFromBool(ok) });
        return ok;
    }
    fn boundaries(self: *const Probe) bool {
        if (!self.control(.{ .output_count = 2 })) return false;
        var out: a.AudioOutputCatalog = .{ .registry_generation = 0x100000055 };
        const before = out;
        for ([_]usize{0, 1, 255}) |n| if (self.audio.audioOutputCatalog(rows[0..n], &out) != a.service_api_result_invalid or !std.mem.eql(u8, std.mem.asBytes(&before), std.mem.asBytes(&out))) return false;
        out.size = 223;
        if (self.audio.audioOutputCatalog(&rows, &out) != a.service_api_result_invalid or out.registry_generation != before.registry_generation) return false;
        const seen = self.snapshot() orelse return false;
        if (seen.queries != 0 or seen.epochs != 0 or seen.secondary_queries != 0) return false;
        const alias_rows: [*]a.AudioOutputInfo = @ptrCast(@alignCast(&alias_storage));
        // Also reject a receipt in the caller's declared row tail, beyond
        // the first256 entries that the Kernel will actually populate.
        for ([_]usize{ 0, 256 }) |index| {
            const header: *a.AudioOutputCatalog = @ptrFromInt(@intFromPtr(alias_rows) + index * @sizeOf(a.AudioOutputInfo));
            header.* = .{ .registry_generation = 0x1234_5678_0000_0055 };
            const original = header.*;
            if (self.audio.audioOutputCatalog(alias_rows[0..257], header) != a.service_api_result_invalid or
                !std.mem.eql(u8, std.mem.asBytes(&original), std.mem.asBytes(header))) return false;
        }
        const after_alias = self.snapshot() orelse return false;
        if (after_alias.queries != 0 or after_alias.epochs != 0) return false;
        var large: extern struct { header: a.AudioOutputCatalog = .{}, tail: [16]u8 = @splat(0xa5) } = .{};
        large.header.size = @sizeOf(@TypeOf(large));
        if (self.audio.audioOutputCatalog(&rows, &large.header) != 1 or large.header.size != 224) return false;
        for (large.tail) |byte| if (byte != 0xa5) return false;
        self.report("P55 BOUNDARIES shortCapacity=0/1/255 shortHeader=223 overlap=0/256 negativeCallbacks=0 receiptUntouched=1 extendedHeaderTail=unchanged pass=1", .{});
        return true;
    }
    fn service(self: *const Probe, app: *r4os.App) bool {
        const audio = app.audio() orelse return false;
        const timeout = r4os.time_contract.timeoutFinite(.{ .nanoseconds = 2_000_000_000 });
        if (!self.control(.{ .output_count = 32, .secondary_count = 1 })) return false;
        self.sys.sleepTicks(self.sys.ticksFromMilliseconds(650));
        var page: a.AudioServiceOutputState = .{};
        var request: a.AudioServiceOutputRequest = .{};
        if (audio.outputs(&request, timeout, &page) != .ok or page.total != 33 or page.count != 8) return false;
        const epoch = page.service_epoch; const revision = page.revision;
        var seen: usize = page.count;
        while (seen < 33) {
            request = .{ .service_epoch = epoch, .revision = revision, .index = @intCast(seen) };
            if (audio.outputs(&request, timeout, &page) != .ok or page.service_epoch != epoch or page.revision != revision or page.total != 33 or page.count == 0) return false;
            seen += page.count;
        }
        self.report("P55 SERVICE_PAGES epoch={d} revision={d} total={d} pages=5 coherent=1", .{ epoch, revision, seen });
        request = .{}; @memcpy(request.id[0..5], "P55-0");
        if (audio.selectOutput(&request, timeout, &page) != .ok or !std.mem.eql(u8, &page.desired_id, &request.id)) return false;
        pcm_audio = self.audio; pcm_sys = self.sys;
        @atomicStore(bool, &pcm_stop, false, .release);
        @atomicStore(u64, &pcm_bytes, 0, .release); @atomicStore(u64, &pcm_writes, 0, .release);
        var worker: a.ProgramJoinHandle = .{};
        if (self.sys.threadCreateHandle(pcmThread, 0, 128 * 1024, 0, &worker) != a.thread_ok) return false;
        var joined = false;
        defer if (!joined) {
            @atomicStore(bool, &pcm_stop, true, .release);
            var exit: i32 = -99;
            _ = self.sys.threadHandleJoin(&worker, self.sys.ticksFromMilliseconds(5000), &exit);
        };
        const desired = request.id;
        if (!self.control(.{ .output_count = 0, .secondary_count = 1 })) return false;
        self.sys.sleepTicks(self.sys.ticksFromMilliseconds(650));
        request = .{};
        if (audio.outputs(&request, timeout, &page) != .ok or page.total != 1 or !std.mem.startsWith(u8, &page.active_id, "P55B-0") or !std.mem.eql(u8, &page.desired_id, &desired)) return false;
        self.report("P55 SERVICE_WITHDRAW total={d} desired=P55-0 active=P55B-0 reason={d} pass=1", .{ page.total, page.reason });
        if (!self.control(.{ .output_count = 2, .secondary_count = 1 })) return false;
        self.sys.sleepTicks(self.sys.ticksFromMilliseconds(650));
        if (audio.outputs(&request, timeout, &page) != .ok or page.total != 3 or !std.mem.eql(u8, &page.active_id, &desired) or page.reason != a.audio_output_reason_preferred) return false;
        const retained = page;
        if (!self.control(.{ .output_count = 2, .secondary_count = 1, .fail_at_query = 1 })) return false;
        self.sys.sleepTicks(self.sys.ticksFromMilliseconds(650));
        if (audio.outputs(&request, timeout, &page) != .ok or page.reason != a.audio_output_reason_catalog_busy or page.total != retained.total or !std.mem.eql(u8, std.mem.asBytes(&page.outputs), std.mem.asBytes(&retained.outputs))) return false;
        var status: a.AudioServiceStatus = .{};
        if (audio.status(timeout, &status) != .ok or status.flags & a.audio_service_flag_backend_present != 0) return false;
        self.report("P55 SERVICE_BUSY retained={d} reason={d} backendFlag={d} pass=1", .{ page.total, page.reason, status.flags & a.audio_service_flag_backend_present });
        if (!self.control(.{ .output_count = 2, .secondary_count = 1 })) return false;
        self.sys.sleepTicks(self.sys.ticksFromMilliseconds(650));
        if (audio.outputs(&request, timeout, &page) != .ok or !std.mem.eql(u8, &page.active_id, &desired)) return false;
        var captures: u32 = 0; var busy: u32 = 0;
        for (0..60) |_| {
            receipt = .{};
            const rc = self.audio.audioOutputCatalog(&rows, &receipt);
            if (rc == 1) { if (receipt.count != 3) return false; captures += 1; }
            else if (rc == a.service_api_result_busy) busy += 1 else return false;
            self.sys.sleepTicks(1);
        }
        @atomicStore(bool, &pcm_stop, true, .release);
        var exit: i32 = -99;
        const j = self.sys.threadHandleJoin(&worker, self.sys.ticksFromMilliseconds(5000), &exit);
        joined = j == a.thread_ok;
        const bytes = @atomicLoad(u64, &pcm_bytes, .acquire); const writes = @atomicLoad(u64, &pcm_writes, .acquire);
        const observed = self.snapshot() orelse return false;
        const ok = joined and exit == 0 and bytes > 0 and writes > 0 and captures > 0 and observed.writes > 0;
        self.report("P55 PARALLEL_PCM thread={d}:{d} join={d} exit={d} acceptedBytes={d} writes={d} backendWrites={d} completeCaptures={d} busy={d} pass={d}; software ACK only", .{ worker.thread_id, worker.thread_generation, j, exit, bytes, writes, observed.writes, captures, busy, @intFromBool(ok) });
        return ok;
    }
    fn mixed(self: *const Probe) bool {
        if (!self.control(.{ .output_count = 2, .change_at_query = 2 })) return false;
        const before = self.snapshot() orelse return false;
        if (!self.collect(2)) return false;
        const first = generation(&rows[0].name) orelse return false;
        const second = generation(&rows[1].name) orelse return false;
        const after = self.snapshot() orelse return false;
        const observed_mix = first == before.generation and second == before.generation + 1 and after.generation == second;
        self.report("P55 SCALAR_MIX before={d} after={d} first={d} second={d} queries={d} actives={d} mixed={d}; desired whole-catalog coherence fails on controlled real-provider change, not an AUDSVC/hardware claim", .{ before.generation, after.generation, first, second, after.queries, after.actives, @intFromBool(observed_mix) });
        return observed_mix and after.queries == 6 and after.actives == 5;
    }
};
pub fn r4_app_main(app: *r4os.App) i32 {
    const probe = Probe{ .sys = app.system(), .audio = app.audioLowLevel() orelse return 2, .dev = app.devicesLowLevel() orelse return 3 };
    var data: a.ProtocolBuffer = .{};
    var out: a.ProtocolBuffer = .{};
    if (probe.dev.protocolDispatch(p.role, p.ping, &data, &out) != 0) return 4;
    _ = probe.dev.performanceSummary() orelse return 5;
    _ = probe.snapshot() orelse return 6;
    if (app.args().len != 0 and app.args()[0] == 'S') {
        const ok = probe.service(app);
        probe.report("P55 CURRENT_SERVICE result={s}", .{if (ok) @as([]const u8, "PASS") else @as([]const u8, "FAIL")});
        return if (ok) 0 else 1;
    }
    var ok = true;
    ok = probe.capture(1, 2) and ok;
    ok = probe.capture(2, 3) and ok;
    ok = probe.capture(32, 32) and ok;
    ok = probe.failedCapture(.{ .output_count = 2, .change_at_query = 2 }, a.service_api_result_busy, "provider-change") and ok;
    ok = probe.failedCapture(.{ .output_count = 2, .secondary_count = 1, .cross_change = 1 }, a.service_api_result_busy, "earlier-provider-changed-by-later-provider") and ok;
    ok = probe.failedCapture(.{ .output_count = 2, .fail_at_query = 2 }, a.service_api_result_busy, "partial-query-busy") and ok;
    for (1..6) |fault| ok = probe.failedCapture(.{ .output_count = 2, .bad_epoch = @intCast(fault) }, a.service_api_result_invalid, "invalid-real-provider-metadata") and ok;
    ok = probe.reentrantRegistry() and ok;
    ok = probe.rebind() and ok;
    ok = probe.boundaries() and ok;
    ok = probe.scalar(1, 3, 2) and ok;
    ok = probe.scalar(2, 6, 5) and ok;
    ok = probe.scalar(32, 560, 560) and ok;
    probe.report("P55 CURRENT_CORE result={s}; actual Core/R4D callbacks, registry Work and scalar consumer; no timing/DMA/speaker claim", .{if (ok) @as([]const u8, "PASS") else @as([]const u8, "FAIL")});
    return if (ok) 0 else 1;
}
