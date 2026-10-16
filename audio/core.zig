const outputs_contract = @import("r4os_kernel_contract");
const backend_contract = @import("backend_contract.zig");
const mixer = @import("mixer.zig");
pub const AudioOutputInfo = outputs_contract.AudioOutputInfo;
pub const AudioOutputExtension = outputs_contract.AudioOutputExtension;

pub const FORMAT_S16LE: u16 = backend_contract.FORMAT_S16LE;
pub const FORMAT_U8: u16 = backend_contract.FORMAT_U8;
pub const DEFAULT_RATE: u32 = 48_000;
pub const DEFAULT_CHANNELS: u16 = 2;
pub const RING_BYTES: usize = 16 * 1024;
const MAX_STREAMS: usize = 8;
const MAX_NAME: usize = 32;
const MAX_AUDIO_BACKENDS: usize = 8;
const MAX_SYNTH_ENGINES: usize = 8;
const SID_REGISTER_COUNT: u8 = 25;
const SID_RENDER_BYTES: usize = 3840;
const SYNTH_RENDER_MAX_FRAMES: u32 = 1024;
const SYNTH_RENDER_MAX_BYTES: usize = SYNTH_RENDER_MAX_FRAMES * @sizeOf(i16) * DEFAULT_CHANNELS;
const SYNTH_ENGINE_FLAG_MIDI: u32 = 1 << 0;
const MIX_QUANTUM_BYTES: usize = 480 * 4;

pub const StreamOwner = mixer.Owner;
pub const kernel_stream_owner: StreamOwner = .{};
pub const BackendPcmLimits = backend_contract.Limits;

pub const MidiProtocolStatus = struct {
    source: []const u8 = "none",
    classify: u64 = 0,
    missing_required: u64 = 0,
    dispatch_fail: u64 = 0,
    last_result: i32 = 0,
    last_event: u8 = 0,
};

pub const Opl3ProtocolStatus = struct {
    source: []const u8 = "none",
    registers: u64 = 0,
    midi: u64 = 0,
    missing_required: u64 = 0,
    dispatch_fail: u64 = 0,
    last_result: i32 = 0,
    last_kind: u8 = 0,
    last_action: u8 = 0,
};

pub const SidProtocolStatus = struct {
    source: []const u8 = "none",
    model: u64 = 0,
    register: u64 = 0,
    io: u64 = 0,
    missing_required: u64 = 0,
    dispatch_fail: u64 = 0,
    last_result: i32 = 0,
    last_kind: u8 = 0,
    last_voice: u8 = 0,
};

pub const WritePcmFn = *const fn ([]const u8, u32, u16, u16) bool;
pub const StopPcmFn = *const fn () void;
pub const WritePcmCtxFn = *const fn (?*anyopaque, [*]const u8, u32, u32, u16, u16) callconv(.c) i32;
pub const StopPcmCtxFn = *const fn (?*anyopaque) callconv(.c) i32;
pub const BackendStatus = extern struct {
    active: u32 = 0,
    writes: u64 = 0,
    underruns: u64 = 0,
    errors: u64 = 0,
    last_result: i32 = 0,
    reserved: u32 = 0,
    refills: u64 = 0,
    silence_refills: u64 = 0,
    buffer_bytes: u64 = 0,
    queued_buffers: u64 = 0,
    last_buffer_bytes: u64 = 0,
    last_write_ticks: u64 = 0,
    max_write_ticks: u64 = 0,
    total_write_ticks: u64 = 0,
    last_refill_ticks: u64 = 0,
    max_refill_ticks: u64 = 0,
    total_refill_ticks: u64 = 0,
};
pub const StatusCtxFn = *const fn (?*anyopaque, *BackendStatus) callconv(.c) i32;
pub const SynthMidiSendFn = *const fn (u8, u8, u8, u8) void;
pub const SynthRenderFn = *const fn () void;
pub const SynthStopFn = *const fn () void;
pub const SynthMidiSendCtxFn = *const fn (?*anyopaque, u8, u8, u8, u8) callconv(.c) i32;
pub const SynthRenderCtxFn = *const fn (?*anyopaque) callconv(.c) i32;
pub const SynthRenderPcmCtxFn = *const fn (?*anyopaque, [*]u8, u32, u32, u16, u16) callconv(.c) i32;
pub const SynthStopCtxFn = *const fn (?*anyopaque) callconv(.c) i32;
pub const SynthOpl3ResetCtxFn = *const fn (?*anyopaque) callconv(.c) i32;
pub const SynthOpl3WriteRegisterCtxFn = *const fn (?*anyopaque, u8, u8, u8) callconv(.c) i32;
pub const SynthStatus = extern struct {
    active: u32 = 0,
    sends: u64 = 0,
    renders: u64 = 0,
    stops: u64 = 0,
    errors: u64 = 0,
    last_result: i32 = 0,
    reserved: u32 = 0,
};
pub const SynthStatusCtxFn = *const fn (?*anyopaque, *SynthStatus) callconv(.c) i32;
pub const SynthSidAcquireCtxFn = *const fn (?*anyopaque) callconv(.c) i32;
pub const SynthSidReleaseCtxFn = *const fn (?*anyopaque, u32) callconv(.c) i32;
pub const SynthSidSetModelCtxFn = *const fn (?*anyopaque, u32) callconv(.c) i32;
pub const SynthSidWriteRegisterCtxFn = *const fn (?*anyopaque, u32, u32, u32) callconv(.c) i32;
pub const SynthSidLoadDataCtxFn = *const fn (?*anyopaque, u32, u32, [*]const u8, u32) callconv(.c) i32;
pub const SynthSidInitCtxFn = *const fn (?*anyopaque, u32, u32, u32) callconv(.c) i32;
pub const SynthSidPlayFrameCtxFn = *const fn (?*anyopaque, u32, u32, u32) callconv(.c) i32;
pub const SynthSidRenderPcmCtxFn = *const fn (?*anyopaque, u32, [*]u8, u32) callconv(.c) i32;

pub const PerformanceSummary = extern struct {
    max_streams: u32 = @intCast(MAX_STREAMS),
    open_streams: u32 = 0,
    closed_streams: u32 = 0,
    max_backends: u32 = @intCast(MAX_AUDIO_BACKENDS),
    registered_backends: u32 = 0,
    active_backends: u32 = 0,
    max_synths: u32 = @intCast(MAX_SYNTH_ENGINES),
    registered_synths: u32 = 0,
    total_stream_writes: u64 = 0,
    backend_ok: u64 = 0,
    backend_fail: u64 = 0,
    backend_status_writes: u64 = 0,
    backend_status_underruns: u64 = 0,
    backend_status_errors: u64 = 0,
    stream_ring_bytes: u64 = 0,
    stream_available_bytes: u64 = 0,
    stream_available_high_water_bytes: u64 = 0,
    stream_write_truncations: u64 = 0,
    stream_dropped_bytes: u64 = 0,
    stream_write_total_ticks: u64 = 0,
    stream_write_max_ticks: u64 = 0,
    stream_write_last_ticks: u64 = 0,
    backend_write_calls: u64 = 0,
    backend_write_total_ticks: u64 = 0,
    backend_write_max_ticks: u64 = 0,
    backend_write_last_ticks: u64 = 0,
    backend_status_refills: u64 = 0,
    backend_status_silence_refills: u64 = 0,
    backend_status_buffer_bytes: u64 = 0,
    backend_status_queued_buffers: u64 = 0,
    backend_status_last_buffer_bytes: u64 = 0,
    backend_status_refill_total_ticks: u64 = 0,
    backend_status_refill_max_ticks: u64 = 0,
    backend_status_refill_last_ticks: u64 = 0,
    sid_writes: u64 = 0,
    sid_pcm_writes: u64 = 0,
    sid_pcm_fail: u64 = 0,
    midi_sends: u64 = 0,
    midi_renders: u64 = 0,
};

pub const BackendFamilyStatus = extern struct { registered: bool = false, active: bool = false, has_output: bool = false };
const WireSpan = extern struct {
    pointer: ?[*]const u8 = null,
    len: usize = 0,
    fn bytes(self: @This()) []const u8 {
        return if (self.len == 0) "" else self.pointer.?[0..self.len];
    }
};
const WireMidiProtocolStatus = extern struct {
    source: WireSpan = .{},
    classify: u64 = 0,
    missing_required: u64 = 0,
    dispatch_fail: u64 = 0,
    last_result: i32 = 0,
    last_event: u8 = 0,
};
const WireOpl3ProtocolStatus = extern struct {
    source: WireSpan = .{},
    registers: u64 = 0,
    midi: u64 = 0,
    missing_required: u64 = 0,
    dispatch_fail: u64 = 0,
    last_result: i32 = 0,
    last_kind: u8 = 0,
    last_action: u8 = 0,
};
const WireSidProtocolStatus = extern struct {
    source: WireSpan = .{},
    model: u64 = 0,
    register: u64 = 0,
    io: u64 = 0,
    missing_required: u64 = 0,
    dispatch_fail: u64 = 0,
    last_result: i32 = 0,
    last_kind: u8 = 0,
    last_voice: u8 = 0,
};
const BackendRegistration = extern struct { pointer: ?*const anyopaque = null, context: ?*anyopaque = null, write_legacy: usize = 0, stop_legacy: usize = 0, write: ?WritePcmCtxFn = null, stop: ?StopPcmCtxFn = null, status: ?StatusCtxFn = null, limits_present: bool = false, limits: BackendPcmLimits = .{} };
const SynthRegistration = extern struct { flags: u32 = 0, pointer: ?*const anyopaque = null, context: ?*anyopaque = null, send_legacy: usize = 0, render_legacy: usize = 0, stop_legacy: usize = 0, send: ?SynthMidiSendCtxFn = null, render: ?SynthRenderCtxFn = null, render_pcm: ?SynthRenderPcmCtxFn = null, stop: ?SynthStopCtxFn = null, status: ?SynthStatusCtxFn = null, opl3_reset: ?SynthOpl3ResetCtxFn = null, opl3_write: ?SynthOpl3WriteRegisterCtxFn = null, sid_acquire: ?SynthSidAcquireCtxFn = null, sid_release: ?SynthSidReleaseCtxFn = null, sid_model: ?SynthSidSetModelCtxFn = null, sid_write: ?SynthSidWriteRegisterCtxFn = null, sid_load: ?SynthSidLoadDataCtxFn = null, sid_init: ?SynthSidInitCtxFn = null, sid_play: ?SynthSidPlayFrameCtxFn = null, sid_pcm: ?SynthSidRenderPcmCtxFn = null, external: bool = false, migration: bool = false };
const Args = [32]usize;
extern fn r4os_rust_audio_core(op: u32, args: *const Args) u64;
fn call(op: u32, args: Args) u64 {
    return r4os_rust_audio_core(op, &args);
}
pub fn init() void {
    const args: Args = .{0} ** 32;
    _ = call(0, args);
}
pub fn startProgressWorker() bool {
    const args: Args = .{0} ** 32;
    return call(1, args) != 0;
}
pub fn configureSidModel(model: []const u8) void {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(model.ptr);
    args[1] = model.len;
    _ = call(2, args);
}
pub fn applyConfiguredSidModel() i32 {
    const args: Args = .{0} ** 32;
    return @bitCast(@as(u32, @truncate(call(3, args))));
}
pub fn sidModelNameZ() [*:0]const u8 {
    const args: Args = .{0} ** 32;
    return @ptrFromInt(call(4, args));
}
pub fn sidProtocolStatus() SidProtocolStatus {
    var args: Args = .{0} ** 32;
    var out: WireSidProtocolStatus = undefined;
    args[31] = @intFromPtr(&out);
    _ = call(5, args);
    return .{ .source = out.source.bytes(), .model = out.model, .register = out.register, .io = out.io, .missing_required = out.missing_required, .dispatch_fail = out.dispatch_fail, .last_result = out.last_result, .last_kind = out.last_kind, .last_voice = out.last_voice };
}
pub fn openStream(rate: u32, channels: u16, format: u16) i32 {
    var args: Args = .{0} ** 32;
    args[0] = rate;
    args[1] = channels;
    args[2] = format;
    return @bitCast(@as(u32, @truncate(call(6, args))));
}
pub fn openStreamForOwner(owner: StreamOwner, rate: u32, channels: u16, format: u16) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(&owner);
    args[1] = rate;
    args[2] = channels;
    args[3] = format;
    return @bitCast(@as(u32, @truncate(call(7, args))));
}
pub fn writeStream(id: u32, ptr: [*]const u8, byte_count: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = id;
    args[1] = @intFromPtr(ptr);
    args[2] = byte_count;
    return @bitCast(@as(u32, @truncate(call(8, args))));
}
pub fn writeStreamForOwner(owner: StreamOwner, id: u32, ptr: [*]const u8, byte_count: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(&owner);
    args[1] = id;
    args[2] = @intFromPtr(ptr);
    args[3] = byte_count;
    return @bitCast(@as(u32, @truncate(call(9, args))));
}
pub fn closeStream(id: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = id;
    return @bitCast(@as(u32, @truncate(call(10, args))));
}
pub fn closeStreamForOwner(owner: StreamOwner, id: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(&owner);
    args[1] = id;
    return @bitCast(@as(u32, @truncate(call(11, args))));
}
pub fn closeStreamsForOwner(owner: StreamOwner) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(&owner);
    return call(12, args) != 0;
}
pub fn closeAllStreams() bool {
    const args: Args = .{0} ** 32;
    return call(13, args) != 0;
}
pub fn setVolume(id: u32, fixed_volume: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = id;
    args[1] = fixed_volume;
    return @bitCast(@as(u32, @truncate(call(14, args))));
}
pub fn setVolumeForOwner(owner: StreamOwner, id: u32, fixed_volume: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(&owner);
    args[1] = id;
    args[2] = fixed_volume;
    return @bitCast(@as(u32, @truncate(call(15, args))));
}
pub fn sidAcquire() i32 {
    const args: Args = .{0} ** 32;
    return @bitCast(@as(u32, @truncate(call(16, args))));
}
pub fn sidWriteRegister(handle: u32, register: u8, value: u8) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    args[1] = register;
    args[2] = value;
    return @bitCast(@as(u32, @truncate(call(17, args))));
}
pub fn sidRelease(handle: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    return @bitCast(@as(u32, @truncate(call(18, args))));
}
pub fn sidLoadData(handle: u32, load_addr: u16, ptr: [*]const u8, byte_count: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    args[1] = load_addr;
    args[2] = @intFromPtr(ptr);
    args[3] = byte_count;
    return @bitCast(@as(u32, @truncate(call(19, args))));
}
pub fn sidInit(handle: u32, init_addr: u16, song: u16) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    args[1] = init_addr;
    args[2] = song;
    return @bitCast(@as(u32, @truncate(call(20, args))));
}
pub fn sidPlayFrame(handle: u32, play_addr: u16, frame_hz: u16) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    args[1] = play_addr;
    args[2] = frame_hz;
    return @bitCast(@as(u32, @truncate(call(21, args))));
}
pub fn sidStop(handle: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    return @bitCast(@as(u32, @truncate(call(22, args))));
}
pub fn midiOpenSynth(backend_z: [*:0]const u8) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(backend_z);
    return @bitCast(@as(u32, @truncate(call(23, args))));
}
pub fn midiSend(handle: u32, channel: u8, status: u8, data1: u8, data2: u8) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    args[1] = channel;
    args[2] = status;
    args[3] = data1;
    args[4] = data2;
    return @bitCast(@as(u32, @truncate(call(24, args))));
}
pub fn midiClose(handle: u32) i32 {
    var args: Args = .{0} ** 32;
    args[0] = handle;
    return @bitCast(@as(u32, @truncate(call(25, args))));
}
pub fn midiProtocolStatus() MidiProtocolStatus {
    var args: Args = .{0} ** 32;
    var out: WireMidiProtocolStatus = undefined;
    args[31] = @intFromPtr(&out);
    _ = call(26, args);
    return .{ .source = out.source.bytes(), .classify = out.classify, .missing_required = out.missing_required, .dispatch_fail = out.dispatch_fail, .last_result = out.last_result, .last_event = out.last_event };
}
pub fn opl3ProtocolStatus() Opl3ProtocolStatus {
    var args: Args = .{0} ** 32;
    var out: WireOpl3ProtocolStatus = undefined;
    args[31] = @intFromPtr(&out);
    _ = call(27, args);
    return .{ .source = out.source.bytes(), .registers = out.registers, .midi = out.midi, .missing_required = out.missing_required, .dispatch_fail = out.dispatch_fail, .last_result = out.last_result, .last_kind = out.last_kind, .last_action = out.last_action };
}
pub fn opl3Reset() i32 {
    const args: Args = .{0} ** 32;
    return @bitCast(@as(u32, @truncate(call(28, args))));
}
pub fn opl3WriteRegister(bank: u8, reg: u8, value: u8) i32 {
    var args: Args = .{0} ** 32;
    args[0] = bank;
    args[1] = reg;
    args[2] = value;
    return @bitCast(@as(u32, @truncate(call(29, args))));
}
pub fn opl3RunRegisterDemo() void {
    const args: Args = .{0} ** 32;
    _ = call(30, args);
}
pub fn opl3RenderBlock() i32 {
    const args: Args = .{0} ** 32;
    return @bitCast(@as(u32, @truncate(call(31, args))));
}
pub fn opl3Stop() i32 {
    const args: Args = .{0} ** 32;
    return @bitCast(@as(u32, @truncate(call(32, args))));
}
pub fn allocDmaBuffer(bytes: u32, alignment: u32) u64 {
    var args: Args = .{0} ** 32;
    args[0] = bytes;
    args[1] = alignment;
    return call(33, args);
}
pub fn freeDmaBuffer(phys_addr: u64, bytes: u32) void {
    var args: Args = .{0} ** 32;
    args[0] = phys_addr;
    args[1] = bytes;
    _ = call(34, args);
}
pub fn registerAudioBackendZ(name: [*:0]const u8, backend: *const anyopaque) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name);
    args[1] = @intFromPtr(backend);
    return @bitCast(@as(u32, @truncate(call(35, args))));
}
pub fn registerMixerBackendZ(name: [*:0]const u8, backend: *const anyopaque) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name);
    args[1] = @intFromPtr(backend);
    return @bitCast(@as(u32, @truncate(call(36, args))));
}
pub fn registerSynthEngineZ(name: [*:0]const u8, engine: *const anyopaque) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name);
    args[1] = @intFromPtr(engine);
    return @bitCast(@as(u32, @truncate(call(37, args))));
}
pub fn registerAudioBackend(name: []const u8, backend: ?*const anyopaque) void {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    args[2] = if (backend) |v| @intFromPtr(v) else 0;
    _ = call(38, args);
}
pub fn registerExternalAudioBackendZ(name: [*:0]const u8, limits: BackendPcmLimits, context: ?*anyopaque, write_pcm: WritePcmCtxFn, stop_pcm: ?StopPcmCtxFn, status: ?StatusCtxFn) i32 {
    const config = BackendRegistration{ .context = context, .write = write_pcm, .stop = stop_pcm, .status = status, .limits_present = true, .limits = limits };
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name);
    args[1] = @intFromPtr(&config);
    return @bitCast(@as(u32, @truncate(call(39, args))));
}
pub fn registerExternalAudioOutputs(name: []const u8, outputs: AudioOutputExtension, idle_pcm: ?StopPcmCtxFn) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    args[2] = @intFromPtr(&outputs);
    args[3] = if (idle_pcm) |v| @intFromPtr(v) else 0;
    return call(40, args) != 0;
}
pub fn audioOutputInfo(index: u32, out: *AudioOutputInfo) i32 {
    var args: Args = .{0} ** 32;
    args[0] = index;
    args[1] = @intFromPtr(out);
    return @bitCast(@as(u32, @truncate(call(41, args))));
}
pub fn audioSelectOutput(id: []const u8) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(id.ptr);
    args[1] = id.len;
    return @bitCast(@as(u32, @truncate(call(42, args))));
}
pub fn registerExternalSynthEngineZ(name: [*:0]const u8, flags: u32, context: ?*anyopaque, send_midi: ?SynthMidiSendCtxFn, render: ?SynthRenderCtxFn, stop: ?SynthStopCtxFn, status: ?SynthStatusCtxFn, opl3_reset: ?SynthOpl3ResetCtxFn, opl3_write_register: ?SynthOpl3WriteRegisterCtxFn, sid_acquire: ?SynthSidAcquireCtxFn, sid_release: ?SynthSidReleaseCtxFn, sid_set_model: ?SynthSidSetModelCtxFn, sid_write_register: ?SynthSidWriteRegisterCtxFn, sid_load_data: ?SynthSidLoadDataCtxFn, sid_init: ?SynthSidInitCtxFn, sid_play_frame: ?SynthSidPlayFrameCtxFn, sid_render_pcm: ?SynthSidRenderPcmCtxFn, render_pcm: ?SynthRenderPcmCtxFn) i32 {
    const config = SynthRegistration{ .flags = flags, .context = context, .external = true, .send = send_midi, .render = render, .render_pcm = render_pcm, .stop = stop, .status = status, .opl3_reset = opl3_reset, .opl3_write = opl3_write_register, .sid_acquire = sid_acquire, .sid_release = sid_release, .sid_model = sid_set_model, .sid_write = sid_write_register, .sid_load = sid_load_data, .sid_init = sid_init, .sid_play = sid_play_frame, .sid_pcm = sid_render_pcm };
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name);
    args[1] = @intFromPtr(&config);
    return @bitCast(@as(u32, @truncate(call(43, args))));
}
pub fn unregisterAudioBackendByName(name: []const u8) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    return @bitCast(@as(u32, @truncate(call(44, args))));
}
pub fn unregisterAudioBackendZ(name: [*:0]const u8) i32 {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name);
    return @bitCast(@as(u32, @truncate(call(45, args))));
}
pub fn registerSynthEngine(name: []const u8, engine: ?*const anyopaque) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    args[2] = if (engine) |v| @intFromPtr(v) else 0;
    return call(46, args) != 0;
}
pub fn registerNativeSynthEngine(name: []const u8, send_midi: ?SynthMidiSendFn, render: ?SynthRenderFn, stop: ?SynthStopFn) void {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    args[2] = if (send_midi) |v| @intFromPtr(v) else 0;
    args[3] = if (render) |v| @intFromPtr(v) else 0;
    args[4] = if (stop) |v| @intFromPtr(v) else 0;
    _ = call(47, args);
}
pub fn registerNativeAudioBackend(name: []const u8, write_pcm: WritePcmFn, stop_pcm: ?StopPcmFn) void {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    args[2] = @intFromPtr(write_pcm);
    args[3] = if (stop_pcm) |v| @intFromPtr(v) else 0;
    _ = call(48, args);
}
pub fn selectAudioBackend(name: []const u8) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    return call(49, args) != 0;
}
pub fn unregisterAudioBackend(name: []const u8) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    return call(50, args) != 0;
}
pub fn setAudioBackendClosing(name: []const u8, closing: bool) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    args[2] = @intFromBool(closing);
    return call(51, args) != 0;
}
pub fn audioBackendRegistered(name: []const u8) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    return call(52, args) != 0;
}
pub fn audioBackendFamilyStatus(family: []const u8) BackendFamilyStatus {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(family.ptr);
    args[1] = family.len;
    var out: BackendFamilyStatus = undefined;
    args[31] = @intFromPtr(&out);
    _ = call(53, args);
    return out;
}
pub fn audioBackendActive(name: []const u8) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    return call(54, args) != 0;
}
pub fn audioBackendHasOutput(name: []const u8) bool {
    var args: Args = .{0} ** 32;
    args[0] = @intFromPtr(name.ptr);
    args[1] = name.len;
    return call(55, args) != 0;
}
pub fn performanceSummary() PerformanceSummary {
    var args: Args = .{0} ** 32;
    var out: PerformanceSummary = undefined;
    args[31] = @intFromPtr(&out);
    _ = call(56, args);
    return out;
}
pub fn dumpStatus() void {
    const args: Args = .{0} ** 32;
    _ = call(57, args);
}
export fn r4os_zig_audio_log(data: [*]const u8, len: usize) callconv(.c) void {
    @import("../kernel/log.zig").puts(data[0..len]);
}
export fn r4os_zig_audio_legacy(op: u32, code: usize, data: ?[*]const u8, len: usize, rate: u32, channels: u16, format: u16, a0: u8, b0: u8, c0: u8, d0: u8) callconv(.c) i32 {
    switch (op) {
        0 => {
            const f: WritePcmFn = @ptrFromInt(code);
            return @intFromBool(f(data.?[0..len], rate, channels, format));
        },
        1 => {
            const f: StopPcmFn = @ptrFromInt(code);
            f();
        },
        2 => {
            const f: SynthMidiSendFn = @ptrFromInt(code);
            f(a0, b0, c0, d0);
        },
        3 => {
            const f: SynthStopFn = @ptrFromInt(code);
            f();
        },
        else => unreachable,
    }
    return 0;
}
comptime {
    if (@sizeOf(StreamOwner) != 16 or @sizeOf(BackendPcmLimits) != 16 or @sizeOf(BackendRegistration) != 80 or @sizeOf(SynthRegistration) != 176) @compileError("private audio record layout");
}
