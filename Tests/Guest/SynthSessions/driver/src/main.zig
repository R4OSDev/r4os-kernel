const r4os = @import("r4os");
const p = @import("shared.zig");
const a = r4os.abi;
var api: *const r4os.r4dev.DriverApi = undefined;
var state: p.State = .{};
var backend: r4os.r4dev.AudioOutputIdleBackend = undefined;
var synth: a.SynthEngine = .{};
var midi: a.SynthEngine = .{};
comptime { asm(r4os.r4dev.driverEntriesAsm("p52_init", "p52_shutdown")); }
fn dispatch(op: u32, data: ?*anyopaque, len: u32) i32 {
    const ctx = r4os.r4dev.DriverContext.init(api);
    var in_buffer: a.ProtocolBuffer = .{ .data = data, .len = len, .capacity = len };
    var out: a.ProtocolBuffer = .{};
    return ctx.protocolDispatch(p.role, op, &in_buffer, &out);
}
export fn p52_init(table: *const r4os.r4dev.DriverApi) callconv(.c) i32 {
    api = table; const ctx = r4os.r4dev.DriverContext.init(table);
    if (!ctx.apiCompatible()) return -10;
    backend = .{ .base = .{ .base = .{
        .version = a.audio_backend_idle_version, .size = @sizeOf(r4os.r4dev.AudioOutputIdleBackend),
        .write_pcm = write, .stop = stop, .status = status, .min_rate = 8000, .max_rate = 192000, .max_channels = 2,
    }, .outputs = .{ .query = query, .select = select, .active = active } }, .idle = idle };
    if (ctx.registerAudioOutputBackend("PROOF53", &backend.base.base) != 0) return -12;
    synth = .{ .flags = a.synth_engine_flag_sid, .stop = sidStop, .sid_acquire = acquire, .sid_release = release, .sid_write_register = sidWrite, .sid_load_data = load, .sid_init = init, .sid_play_frame = play, .sid_render_pcm = render };
    if (registerSynth(&ctx) != 0) return -13;
    midi = .{ .flags = a.synth_engine_flag_midi | a.synth_engine_flag_opl3, .midi_send = midiSend, .render_pcm = midiRender, .stop = midiStop };
    if (ctx.registerSynthEngineEx("OPL3", &midi) != 0) return -14;
    return 0;
}
export fn p52_shutdown() callconv(.c) i32 { _ = dispatch(p.unregister, null, 0); return 0; }
fn write(_: ?*anyopaque, bytes: [*]const u8, len: u32, _: u32, _: u16, _: u16) callconv(.c) i32 {
    p.count(&state.write_calls);
    const result = p.load(&state.write_result); if (result != 0) return result;
    p.count(&state.writes); _ = @atomicRmw(u64, &state.bytes, .Add, len, .acq_rel);
    @atomicStore(u64, &state.last_hash, p.hash(bytes[0..len]), .release); return 0;
}
fn idle(_: ?*anyopaque) callconv(.c) i32 {
    p.count(&state.idle_calls); const result = p.load(&state.idle_result);
    if (result == 0) { p.count(&state.idle_acks); p.count(&state.reset_generation); }
    return result;
}
fn stop(_: ?*anyopaque) callconv(.c) i32 {
    p.count(&state.stop_calls); const result = p.load(&state.stop_result);
    if (result == 0) { p.count(&state.stop_acks); p.count(&state.reset_generation); }
    return result;
}
fn status(_: ?*anyopaque, out: *a.AudioBackendStatus) callconv(.c) i32 {
    // Generic R4D activation precedes the lazy protocol catalogue. The
    // observer lends this resident record only once that catalogue is ready.
    var registration = p.Registration{ .address = @intFromPtr(&state), .bytes = @sizeOf(p.State) };
    _ = dispatch(p.register, &registration, @sizeOf(p.Registration));
    if (@atomicRmw(u32, &state.replace_requested, .Xchg, 0, .acq_rel) != 0) {
        const ctx = r4os.r4dev.DriverContext.init(api);
        _ = registerSynth(&ctx);
    }
    out.* = .{ .active = 1, .writes = p.load(&state.writes), .refills = p.load(&state.writes), .buffer_bytes = 1920 }; return 0;
}
fn query(_: u64, index: u32, out: *a.AudioOutputInfo) callconv(.c) i32 {
    if (index >= 2) return 0;
    out.* = .{ .id = if (index == 0) fixed("PROOF53_A") else fixed("PROOF53_B"), .name = fixed("Controlled real Kernel callback boundary"), .kind = a.audio_output_kind_speaker, .availability = a.audio_output_available, .flags = if (index == p.load(&state.active_output)) a.audio_output_flag_active else 0, .preferred_rate = 48000, .channels = 2, .format = 1 }; return 1;
}
fn select(_: u64, index: u32) callconv(.c) i32 {
    if (index >= 2) return -1;
    if (p.load(&state.active_output) == index) return 0;
    const result = stop(null);
    if (result != 0) return result;
    @atomicStore(u32, &state.active_output, index, .release); p.count(&state.output_generation); return 0;
}
fn active(_: u64) callconv(.c) i32 { return @intCast(p.load(&state.active_output)); }
fn registerSynth(ctx: *const r4os.r4dev.DriverContext) i32 {
    const result = ctx.registerSynthEngineEx("SID", &synth);
    @atomicStore(i32, &state.replace_result, result, .release);
    if (result == 0) p.count(&state.synth_registrations);
    return result;
}
fn fixed(comptime value: []const u8) [64]u8 { var bytes: [64]u8 = .{0} ** 64; @memcpy(bytes[0..value.len], value); return bytes; }
fn sidStop(_: ?*anyopaque) callconv(.c) i32 {
    p.count(&state.sid_stop_calls); const result = p.load(&state.sid_stop_result); if (result == 0) p.count(&state.sid_stop_acks); return result;
}
fn acquire(_: ?*anyopaque) callconv(.c) i32 {
    p.count(&state.sid_acquires);
    const ctx = r4os.r4dev.DriverContext.init(api);
    // Existing storage-callback admission intentionally forbids heap use.
    if (ctx.heap() != null) return -73;
    ctx.logInfo("P53 SID callback admission; heap correctly unavailable");
    p.count(&state.callback_scope_checks);
    return 1;
}
fn release(_: ?*anyopaque, _: u32) callconv(.c) i32 { p.count(&state.sid_releases); const result=p.load(&state.sid_release_result); if(result==0)p.count(&state.sid_release_acks); return result; }
fn sidWrite(_: ?*anyopaque, _: u32, _: u32, _: u32) callconv(.c) i32 { return 0; }
fn load(_: ?*anyopaque, _: u32, _: u32, _: [*]const u8, _: u32) callconv(.c) i32 { return 0; }
fn init(_: ?*anyopaque, _: u32, _: u32, _: u32) callconv(.c) i32 { return 0; }
fn play(_: ?*anyopaque, _: u32, _: u32, _: u32) callconv(.c) i32 { p.count(&state.sid_plays); return 0; }
fn render(_: ?*anyopaque, _: u32, out: [*]u8, len: u32) callconv(.c) i32 {
    p.count(&state.sid_renders); const bytes = @min(len, 3840) & ~@as(u32, 3);
    for (out[0..bytes], 0..) |*b, i| b.* = @intCast((i *% 13 +% 37) & 255);
    return @intCast(bytes);
}
fn midiSend(_: ?*anyopaque, _: u8, _: u8, _: u8, _: u8) callconv(.c) i32 { return 0; }
fn midiRender(_: ?*anyopaque, out: [*]u8, len: u32, _: u32, _: u16, _: u16) callconv(.c) i32 {
    p.count(&state.midi_renders); @memset(out[0..len], 0x5a); return @intCast(len);
}
fn midiStop(_: ?*anyopaque) callconv(.c) i32 { p.count(&state.midi_stops); const result=p.load(&state.midi_stop_result); if(result==0)p.count(&state.midi_stop_acks); return result; }
