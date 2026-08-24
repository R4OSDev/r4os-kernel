const bootlog = @import("../kernel/bootlog.zig");
const heap = @import("../memory/heap.zig");
const phys = @import("../memory/phys.zig");
const sync = @import("../sched/sync.zig");
const timer = @import("../kernel/timer.zig");
const protocol_api = @import("../kernel/protocol_api.zig");
const r4p = @import("../program/r4p.zig");
const r4x_api = @import("../program/r4x_api.zig");
const r4p_contract = @import("../net/r4p_contract.zig");
const k = @import("../kernel/log.zig");
const backend_contract = @import("backend_contract.zig");
const mixer = @import("mixer.zig");
const pcm = @import("pcm.zig");

pub const FORMAT_S16LE: u16 = backend_contract.FORMAT_S16LE;
pub const FORMAT_U8: u16 = backend_contract.FORMAT_U8;
pub const DEFAULT_RATE: u32 = 48_000;
pub const DEFAULT_CHANNELS: u16 = 2;
pub const RING_BYTES: usize = 16 * 1024;
const MAX_STREAMS: usize = 8;
const MAX_NAME: usize = 32;
const MAX_AUDIO_BACKENDS: usize = 4;
const MAX_SYNTH_ENGINES: usize = 8;
const SID_REGISTER_COUNT: u8 = 25;
const SID_RENDER_BYTES: usize = 3840;
const MIX_QUANTUM_BYTES: usize = 480 * pcm.TARGET_FRAME_BYTES;

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

pub const StreamState = enum(u8) {
    empty,
    open,
    closed,
};

const Stream = struct {
    state: StreamState = .empty,
    id: u32 = 0,
    owner: StreamOwner = .{},
    rate: u32 = 0,
    channels: u16 = 0,
    format: u16 = 0,
    volume: u32 = 0x0001_0000,
    ring: []u8 = &empty_ring,
    read_pos: usize = 0,
    write_pos: usize = 0,
    available: usize = 0,
    deferred_once: bool = false,
    resampler: pcm.ResamplerState = .{},
    total_written: u64 = 0,
};

const NamedBackend = struct {
    active: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    ptr: ?*const anyopaque = null,
};

const SynthEngine = struct {
    registered: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    ptr: ?*const anyopaque = null,
    context: ?*anyopaque = null,
    midi_send: ?SynthMidiSendFn = null,
    render: ?SynthRenderFn = null,
    stop: ?SynthStopFn = null,
    midi_send_ctx: ?SynthMidiSendCtxFn = null,
    render_ctx: ?SynthRenderCtxFn = null,
    stop_ctx: ?SynthStopCtxFn = null,
    status_ctx: ?SynthStatusCtxFn = null,
    opl3_reset_ctx: ?SynthOpl3ResetCtxFn = null,
    opl3_write_register_ctx: ?SynthOpl3WriteRegisterCtxFn = null,
    sid_acquire_ctx: ?SynthSidAcquireCtxFn = null,
    sid_release_ctx: ?SynthSidReleaseCtxFn = null,
    sid_set_model_ctx: ?SynthSidSetModelCtxFn = null,
    sid_write_register_ctx: ?SynthSidWriteRegisterCtxFn = null,
    sid_load_data_ctx: ?SynthSidLoadDataCtxFn = null,
    sid_init_ctx: ?SynthSidInitCtxFn = null,
    sid_play_frame_ctx: ?SynthSidPlayFrameCtxFn = null,
    sid_render_pcm_ctx: ?SynthSidRenderPcmCtxFn = null,
    external_contract: bool = false,
    migration_bridge: bool = false,
    sends: u64 = 0,
    renders: u64 = 0,
    stops: u64 = 0,
};

const AudioBackend = struct {
    registered: bool = false,
    active: bool = false,
    name: [MAX_NAME]u8 = .{0} ** MAX_NAME,
    name_len: usize = 0,
    ptr: ?*const anyopaque = null,
    context: ?*anyopaque = null,
    write_pcm: ?WritePcmFn = null,
    stop_pcm: ?StopPcmFn = null,
    write_pcm_ctx: ?WritePcmCtxFn = null,
    stop_pcm_ctx: ?StopPcmCtxFn = null,
    status_ctx: ?StatusCtxFn = null,
    pcm_limits: ?BackendPcmLimits = null,
};

var empty_ring: [0]u8 = .{};
var streams: [MAX_STREAMS]Stream = .{Stream{}} ** MAX_STREAMS;
var stream_lock = sync.Mutex.initClass("audio-streams", sync.LockRank.audio_core, .sleepable);
var mix_scratch: [MIX_QUANTUM_BYTES]u8 = .{0} ** MIX_QUANTUM_BYTES;
var next_stream_id: u32 = 1;
var audio_backends: [MAX_AUDIO_BACKENDS]AudioBackend = .{AudioBackend{}} ** MAX_AUDIO_BACKENDS;
var active_audio_slot: ?usize = null;
var mixer_backend: NamedBackend = .{};
var synth_engines: [MAX_SYNTH_ENGINES]SynthEngine = .{SynthEngine{}} ** MAX_SYNTH_ENGINES;
var active_midi_synth_slot: ?usize = null;
var total_stream_writes: u64 = 0;
var total_backend_ok: u64 = 0;
var total_backend_fail: u64 = 0;
var stream_available_high_water: u64 = 0;
var stream_write_truncations: u64 = 0;
var stream_dropped_bytes: u64 = 0;
var stream_write_total_ticks: u64 = 0;
var stream_write_max_ticks: u64 = 0;
var stream_write_last_ticks: u64 = 0;
var backend_write_calls: u64 = 0;
var backend_write_total_ticks: u64 = 0;
var backend_write_max_ticks: u64 = 0;
var backend_write_last_ticks: u64 = 0;
var sid_acquired: bool = false;
var sid_write_count: u64 = 0;
var sid_last_register: u8 = 0;
var sid_last_value: u8 = 0;
var midi_acquired: bool = false;
var midi_send_count: u64 = 0;
var midi_last_channel: u8 = 0;
var midi_last_status: u8 = 0;
var midi_last_data1: u8 = 0;
var midi_last_data2: u8 = 0;
var midi_render_ticks: u64 = 0;
var midi_r4p_classify: u64 = 0;
var midi_missing_required: u64 = 0;
var midi_dispatch_failures: u64 = 0;
var midi_last_result: i32 = 0;
var midi_last_event: u8 = 0;
var opl3_r4p_register: u64 = 0;
var opl3_r4p_midi: u64 = 0;
var opl3_missing_required: u64 = 0;
var opl3_dispatch_failures: u64 = 0;
var opl3_last_result: i32 = 0;
var opl3_last_write_kind: u8 = 0;
var opl3_last_action: u8 = 0;
var sid_model_option: []const u8 = "8580";
var sid_last_result: i32 = 0;
var sid_pcm_writes: u64 = 0;
var sid_pcm_ok: u64 = 0;
var sid_pcm_fail: u64 = 0;
var sid_r4p_model: u64 = 0;
var sid_r4p_register: u64 = 0;
var sid_r4p_io: u64 = 0;
var sid_missing_required: u64 = 0;
var sid_dispatch_failures: u64 = 0;
var sid_last_kind: u8 = 0;
var sid_last_voice: u8 = 0;

pub const PerformanceSummary = struct {
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

pub fn init() void {
    for (&streams) |*stream| {
        const reusable_ring = stream.ring;
        stream.* = .{};
        if (reusable_ring.len == RING_BYTES) stream.ring = reusable_ring;
    }
    stream_lock = sync.Mutex.initClass("audio-streams", sync.LockRank.audio_core, .sleepable);
    next_stream_id = 1;
    audio_backends = .{AudioBackend{}} ** MAX_AUDIO_BACKENDS;
    active_audio_slot = null;
    mixer_backend = .{};
    synth_engines = .{SynthEngine{}} ** MAX_SYNTH_ENGINES;
    active_midi_synth_slot = null;
    total_stream_writes = 0;
    total_backend_ok = 0;
    total_backend_fail = 0;
    stream_available_high_water = 0;
    stream_write_truncations = 0;
    stream_dropped_bytes = 0;
    stream_write_total_ticks = 0;
    stream_write_max_ticks = 0;
    stream_write_last_ticks = 0;
    backend_write_calls = 0;
    backend_write_total_ticks = 0;
    backend_write_max_ticks = 0;
    backend_write_last_ticks = 0;
    sid_acquired = false;
    sid_write_count = 0;
    sid_last_register = 0;
    sid_last_value = 0;
    midi_acquired = false;
    midi_send_count = 0;
    midi_last_channel = 0;
    midi_last_status = 0;
    midi_last_data1 = 0;
    midi_last_data2 = 0;
    midi_render_ticks = 0;
    midi_r4p_classify = 0;
    midi_missing_required = 0;
    midi_dispatch_failures = 0;
    midi_last_result = 0;
    midi_last_event = 0;
    opl3_r4p_register = 0;
    opl3_r4p_midi = 0;
    opl3_missing_required = 0;
    opl3_dispatch_failures = 0;
    opl3_last_result = 0;
    opl3_last_write_kind = 0;
    opl3_last_action = 0;
    sid_model_option = "8580";
    sid_last_result = 0;
    sid_pcm_writes = 0;
    sid_pcm_ok = 0;
    sid_pcm_fail = 0;
    resetSidProtocolCounters();
    registerMixerBackend("SimpleKernelMixer", null);
    bootlog.puts("[AUDIO] core ready pcm u8/s16le mono/stereo\r\n");
}

pub fn configureSidModel(model: []const u8) void {
    sid_model_option = model;
    if (sidEngine()) |engine| {
        if (engine.sid_set_model_ctx) |set_model| {
            sid_last_result = set_model(engine.context, sidModelId());
        }
    }
}

pub fn sidModelNameZ() [*:0]const u8 {
    if (nameEq(sid_model_option, "6581")) return "6581";
    return "8580";
}

pub fn sidProtocolStatus() SidProtocolStatus {
    return .{
        .source = r4p.requiredSourceName("audio.sid"),
        .model = sid_r4p_model,
        .register = sid_r4p_register,
        .io = sid_r4p_io,
        .missing_required = sid_missing_required,
        .dispatch_fail = sid_dispatch_failures,
        .last_result = sid_last_result,
        .last_kind = sid_last_kind,
        .last_voice = sid_last_voice,
    };
}

pub fn openStream(rate: u32, channels: u16, format: u16) i32 {
    return openStreamForOwner(kernel_stream_owner, rate, channels, format);
}

pub fn openStreamForOwner(owner: StreamOwner, rate: u32, channels: u16, format: u16) i32 {
    if (!owner.valid() or !validStreamFormat(rate, channels, format)) return r4x_api.service_api_result_invalid;
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return r4x_api.service_api_result_busy;
    defer _ = stream_lock.unlock();
    if (!activeOutputPresent()) return r4x_api.service_api_result_no_endpoint;
    const slot = freeStreamSlotLocked() orelse return r4x_api.service_api_result_full;
    const ring = if (streams[slot].ring.len == RING_BYTES)
        streams[slot].ring
    else
        heap.alloc(RING_BYTES, 16) orelse return -3;
    const id = next_stream_id;
    next_stream_id +%= 1;
    if (next_stream_id == 0) next_stream_id = 1;

    streams[slot] = .{
        .state = .open,
        .id = id,
        .owner = owner,
        .rate = rate,
        .channels = channels,
        .format = format,
        .ring = ring,
    };
    bootlog.puts("[AUDIO] stream open id=");
    bootlog.putDec(id);
    bootlog.puts("\r\n");
    return @intCast(id);
}

pub fn writeStream(id: u32, ptr: [*]const u8, byte_count: u32) i32 {
    return writeStreamForOwner(kernel_stream_owner, id, ptr, byte_count);
}

pub fn writeStreamForOwner(owner: StreamOwner, id: u32, ptr: [*]const u8, byte_count: u32) i32 {
    const write_start = timer.tickCount();
    if (!owner.valid()) return r4x_api.service_api_result_invalid;
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return r4x_api.service_api_result_busy;
    defer _ = stream_lock.unlock();
    const stream = streamByOwnerLocked(owner, id) orelse return r4x_api.service_api_result_not_found;
    if (byte_count == 0) return 0;
    if (!activeOutputPresent()) {
        total_backend_fail +%= 1;
        recordTickStat(&stream_write_total_ticks, &stream_write_max_ticks, &stream_write_last_ticks, write_start);
        return r4x_api.service_api_result_no_endpoint;
    }

    const frame_bytes = pcm.sourceFrameBytes(stream.channels, stream.format) orelse return r4x_api.service_api_result_invalid;
    const requested_bytes: usize = @intCast(byte_count);
    const requested_frames = requested_bytes / frame_bytes;
    if (requested_frames == 0) return r4x_api.service_api_result_invalid;

    var free_output_frames = (stream.ring.len - stream.available) / pcm.TARGET_FRAME_BYTES;
    var fitting_frames: usize = @intCast((@as(u64, free_output_frames) * stream.rate) / pcm.TARGET_RATE);
    var accepted_frames = @min(requested_frames, fitting_frames);
    if (accepted_frames == 0) {
        const pre_pump = pumpAvailableLocked(true);
        if (pre_pump < 0) {
            recordTickStat(&stream_write_total_ticks, &stream_write_max_ticks, &stream_write_last_ticks, write_start);
            return pre_pump;
        }
        free_output_frames = (stream.ring.len - stream.available) / pcm.TARGET_FRAME_BYTES;
        fitting_frames = @intCast((@as(u64, free_output_frames) * stream.rate) / pcm.TARGET_RATE);
        accepted_frames = @min(requested_frames, fitting_frames);
    }
    if (accepted_frames == 0) {
        recordTickStat(&stream_write_total_ticks, &stream_write_max_ticks, &stream_write_last_ticks, write_start);
        return r4x_api.service_api_result_busy;
    }

    const accepted_bytes = accepted_frames * frame_bytes;
    const input = ptr[0..accepted_bytes];
    const was_empty = stream.available == 0;
    stream.resampler.beginChunk(stream.rate, stream.channels, stream.format);
    while (!stream.resampler.chunk_done) {
        const produced = pcm.convertStreamingToStereoS16(
            &stream.resampler,
            input,
            stream.rate,
            stream.channels,
            stream.format,
            mix_scratch[0..],
        );
        if (produced == 0 or mixer.ringWrite(stream.ring, &stream.write_pos, &stream.available, mix_scratch[0..produced]) != produced) {
            recordTickStat(&stream_write_total_ticks, &stream_write_max_ticks, &stream_write_last_ticks, write_start);
            return r4x_api.service_api_result_busy;
        }
    }
    if (was_empty) stream.deferred_once = false;
    if (stream.available > stream_available_high_water) stream_available_high_water = @intCast(stream.available);
    stream.total_written +%= accepted_bytes;
    total_stream_writes +%= 1;
    if (accepted_bytes < requested_bytes) {
        stream_write_truncations +%= 1;
        stream_dropped_bytes +%= requested_bytes - accepted_bytes;
    }

    _ = pumpAvailableLocked(false);
    recordTickStat(&stream_write_total_ticks, &stream_write_max_ticks, &stream_write_last_ticks, write_start);
    return @intCast(accepted_bytes);
}

pub fn closeStream(id: u32) i32 {
    return closeStreamForOwner(kernel_stream_owner, id);
}

pub fn closeStreamForOwner(owner: StreamOwner, id: u32) i32 {
    if (!owner.valid()) return r4x_api.service_api_result_invalid;
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return r4x_api.service_api_result_busy;
    defer _ = stream_lock.unlock();
    const stream = streamByOwnerLocked(owner, id) orelse return r4x_api.service_api_result_not_found;
    const pump_result = pumpAvailableLocked(true);
    if (pump_result < 0) return pump_result;
    if (openStreamCountLocked() == 1 and !sid_acquired and !midi_acquired) {
        const stop_result = stopActivePcmResult();
        if (stop_result < 0) return stop_result;
    }
    if (!releaseStreamLocked(stream)) return -3;
    bootlog.puts("[AUDIO] stream close id=");
    bootlog.putDec(id);
    bootlog.puts("\r\n");
    return 0;
}

pub fn closeStreamsForOwner(owner: StreamOwner) bool {
    if (!owner.valid() or (owner.instance_id == 0 and owner.generation == 0)) return false;
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return false;
    defer _ = stream_lock.unlock();

    var matching: u32 = 0;
    var owner_queued = false;
    for (&streams) |*stream| {
        if (stream.state != .open or !mixer.sameOwner(stream.owner, owner)) continue;
        matching += 1;
        owner_queued = owner_queued or stream.available != 0;
    }
    if (matching == 0) return true;
    if (owner_queued and activeOutputPresent() and pumpAvailableLocked(true) < 0) return false;
    if (matching == openStreamCountLocked() and !sid_acquired and !midi_acquired) {
        if (stopActivePcmResult() < 0) return false;
    }

    var released = true;
    for (&streams) |*stream| {
        if (stream.state != .open or !mixer.sameOwner(stream.owner, owner)) continue;
        stream_dropped_bytes +%= stream.available;
        stream.available = 0;
        if (!releaseStreamLocked(stream)) released = false;
    }
    return released;
}

pub fn closeAllStreams() void {
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return;
    defer _ = stream_lock.unlock();
    _ = stopActivePcmResult();
    for (&streams) |*stream| {
        if (stream.state != .open) continue;
        stream_dropped_bytes +%= stream.available;
        stream.available = 0;
        _ = releaseStreamLocked(stream);
    }
}

pub fn setVolume(id: u32, fixed_volume: u32) i32 {
    return setVolumeForOwner(kernel_stream_owner, id, fixed_volume);
}

pub fn setVolumeForOwner(owner: StreamOwner, id: u32, fixed_volume: u32) i32 {
    if (!owner.valid()) return r4x_api.service_api_result_invalid;
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return r4x_api.service_api_result_busy;
    defer _ = stream_lock.unlock();
    const stream = streamByOwnerLocked(owner, id) orelse return r4x_api.service_api_result_not_found;
    stream.volume = fixed_volume;
    return 0;
}

pub fn sidAcquire() i32 {
    const engine = sidEngine() orelse return -1;
    const acquire = engine.sid_acquire_ctx orelse return -3;
    if (sid_acquired) return -2;
    const result = acquire(engine.context);
    sid_last_result = result;
    if (result > 0) sid_acquired = true;
    return result;
}

pub fn sidWriteRegister(handle: u32, register: u8, value: u8) i32 {
    const engine = sidEngine() orelse return -3;
    const write_register = engine.sid_write_register_ctx orelse return -3;
    if (handle != 1 or !sid_acquired) return -1;
    if (register >= SID_REGISTER_COUNT) return -2;
    sid_last_register = register;
    sid_last_value = value;
    sid_write_count += 1;
    sid_last_result = write_register(engine.context, handle, register, value);
    return sid_last_result;
}

pub fn sidRelease(handle: u32) i32 {
    const engine = sidEngine() orelse return -3;
    const release = engine.sid_release_ctx orelse return -3;
    if (handle != 1 or !sid_acquired) return -1;
    sid_last_result = release(engine.context, handle);
    sid_acquired = false;
    return sid_last_result;
}

pub fn sidLoadData(handle: u32, load_addr: u16, ptr: [*]const u8, byte_count: u32) i32 {
    const engine = sidEngine() orelse return -3;
    const load_data = engine.sid_load_data_ctx orelse return -3;
    if (handle != 1 or !sid_acquired) return -1;
    if (byte_count == 0 or byte_count > 65_536) return -2;
    sid_last_result = load_data(engine.context, handle, load_addr, ptr, byte_count);
    return if (sid_last_result == 0) @intCast(byte_count) else sid_last_result;
}

pub fn sidInit(handle: u32, init_addr: u16, song: u16) i32 {
    const engine = sidEngine() orelse return -3;
    const init_sid = engine.sid_init_ctx orelse return -3;
    if (handle != 1 or !sid_acquired) return -1;
    sid_last_result = init_sid(engine.context, handle, init_addr, song);
    return sid_last_result;
}

pub fn sidPlayFrame(handle: u32, play_addr: u16, frame_hz: u16) i32 {
    const engine = sidEngine() orelse return -3;
    const play_frame = engine.sid_play_frame_ctx orelse return -3;
    const render_pcm = engine.sid_render_pcm_ctx orelse return -3;
    if (handle != 1 or !sid_acquired) return -1;
    sid_last_result = play_frame(engine.context, handle, play_addr, frame_hz);
    if (sid_last_result == 0) {
        var block: [SID_RENDER_BYTES]u8 = undefined;
        const rendered = render_pcm(engine.context, handle, block[0..].ptr, SID_RENDER_BYTES);
        sid_last_result = rendered;
        if (rendered < 0) {
            sid_pcm_fail += 1;
            total_backend_fail += 1;
            return rendered;
        }
        sid_pcm_writes += 1;
        engine.renders +%= 1;
        const len: usize = @intCast(rendered);
        const result = writeActivePcm(block[0..len], DEFAULT_RATE, DEFAULT_CHANNELS, FORMAT_S16LE) orelse return 0;
        if (result == 0) {
            sid_pcm_ok += 1;
            total_backend_ok += 1;
        } else {
            sid_pcm_fail += 1;
            total_backend_fail += 1;
        }
        return 0;
    }
    return sid_last_result;
}

pub fn sidStop(handle: u32) i32 {
    const engine = sidEngine() orelse return -3;
    const stop = engine.stop_ctx orelse return -3;
    if (handle != 1 or !sid_acquired) return -1;
    sid_last_result = stop(engine.context);
    if (sid_last_result == 0) engine.stops +%= 1;
    stopActivePcm();
    return sid_last_result;
}

pub fn midiOpenSynth(backend_z: [*:0]const u8) i32 {
    var buf: [MAX_NAME]u8 = undefined;
    const requested = copyZ(backend_z, buf[0..]) orelse return -1;
    if (requested.len == 0) {
        if (findSynthEngine("OPL3") == null and findSynthEngine("MIDI") == null) return -2;
    } else if (findSynthEngine(requested) == null) {
        return -2;
    }
    if (midi_acquired) return -3;
    active_midi_synth_slot = findRenderableSynthEngine() orelse return -4;
    midi_acquired = true;
    return 1;
}

pub fn midiSend(handle: u32, channel: u8, status: u8, data1: u8, data2: u8) i32 {
    if (handle != 1 or !midi_acquired) return -1;
    if (channel > 15) return -2;
    if (status == 0) {
        renderActiveMidiSynth();
        return 0;
    }
    const event = classifyMidiEvent(channel, status, data1, data2) orelse return -3;
    midi_last_channel = channel;
    midi_last_status = event.normalized_status;
    midi_last_data1 = data1;
    midi_last_data2 = data2;
    midi_last_event = event.event;
    midi_send_count += 1;
    sendActiveMidiSynth(channel, event.normalized_status, data1, data2);
    return 0;
}

pub fn midiClose(handle: u32) i32 {
    if (handle != 1 or !midi_acquired) return -1;
    stopActiveMidiSynth();
    active_midi_synth_slot = null;
    midi_acquired = false;
    stopPcmIfNoStreams();
    return 0;
}

pub fn midiProtocolStatus() MidiProtocolStatus {
    return .{
        .source = r4p.requiredSourceName("audio.midi"),
        .classify = midi_r4p_classify,
        .missing_required = midi_missing_required,
        .dispatch_fail = midi_dispatch_failures,
        .last_result = midi_last_result,
        .last_event = midi_last_event,
    };
}

pub fn opl3ProtocolStatus() Opl3ProtocolStatus {
    return .{
        .source = r4p.requiredSourceName("audio.opl3"),
        .registers = opl3_r4p_register,
        .midi = opl3_r4p_midi,
        .missing_required = opl3_missing_required,
        .dispatch_fail = opl3_dispatch_failures,
        .last_result = opl3_last_result,
        .last_kind = opl3_last_write_kind,
        .last_action = opl3_last_action,
    };
}

pub fn opl3Reset() i32 {
    var op: r4p_contract.AudioOpl3Op = .{};
    if (r4p.hasActiveR4p("audio.opl3")) _ = dispatchOpl3(r4p_contract.AUDIO_OPL3_OP_RESET, &op);
    const slot = findSynthEngine("OPL3") orelse return -2;
    const reset = synth_engines[slot].opl3_reset_ctx orelse return -3;
    return reset(synth_engines[slot].context);
}

pub fn opl3WriteRegister(bank: u8, reg: u8, value: u8) i32 {
    _ = classifyOpl3Register(bank, reg, value) orelse return -1;
    const slot = findSynthEngine("OPL3") orelse return -2;
    const write = synth_engines[slot].opl3_write_register_ctx orelse return -3;
    return write(synth_engines[slot].context, bank, reg, value);
}

pub fn opl3RunRegisterDemo() void {
    _ = opl3Reset();
    _ = opl3WriteRegister(1, 0x05, 0x01);
    _ = opl3WriteRegister(0, 0x01, 0x20);
    _ = opl3WriteRegister(0, 0x20, 0x21);
    _ = opl3WriteRegister(0, 0x23, 0x01);
    _ = opl3WriteRegister(0, 0x40, 0x18);
    _ = opl3WriteRegister(0, 0x43, 0x00);
    _ = opl3WriteRegister(0, 0x60, 0xF3);
    _ = opl3WriteRegister(0, 0x63, 0xF3);
    _ = opl3WriteRegister(0, 0x80, 0x77);
    _ = opl3WriteRegister(0, 0x83, 0x77);
    _ = opl3WriteRegister(0, 0xE0, 0x00);
    _ = opl3WriteRegister(0, 0xE3, 0x00);
    _ = opl3WriteRegister(0, 0xA0, 0x98);
    _ = opl3WriteRegister(0, 0xB0, 0x31);
    _ = opl3WriteRegister(0, 0xC0, 0x31);
}

pub fn opl3RenderBlock() i32 {
    const slot = findSynthEngine("OPL3") orelse return -2;
    const render = synth_engines[slot].render_ctx orelse return -3;
    const result = render(synth_engines[slot].context);
    if (result >= 0) synth_engines[slot].renders += 1;
    return result;
}

pub fn opl3Stop() i32 {
    const slot = findSynthEngine("OPL3") orelse return -2;
    if (synth_engines[slot].stop_ctx) |stop| {
        const result = stop(synth_engines[slot].context);
        if (result >= 0) synth_engines[slot].stops += 1;
        return result;
    }
    return 0;
}

pub fn allocDmaBuffer(bytes: u32, alignment: u32) u64 {
    _ = alignment;
    if (bytes == 0 or bytes > phys.FRAME_SIZE) return 0;
    return phys.allocFrame() orelse 0;
}

pub fn freeDmaBuffer(phys_addr: u64, bytes: u32) void {
    _ = bytes;
    if (phys_addr == 0) return;
    phys.freeFrame(phys_addr);
}

pub fn registerAudioBackendZ(name: [*:0]const u8, backend: *const anyopaque) i32 {
    var buf: [MAX_NAME]u8 = undefined;
    const name_slice = copyZ(name, buf[0..]) orelse return -1;
    return if (registerAudioBackendInternal(name_slice, backend, null, null, null, null, null, null, null)) 0 else -2;
}

pub fn registerMixerBackendZ(name: [*:0]const u8, backend: *const anyopaque) i32 {
    return registerZ(&mixer_backend, name, backend, "mixer");
}

pub fn registerSynthEngineZ(name: [*:0]const u8, engine: *const anyopaque) i32 {
    var buf: [MAX_NAME]u8 = undefined;
    const name_slice = copyZ(name, buf[0..]) orelse return -1;
    return if (registerSynthEngine(name_slice, engine)) 0 else -2;
}

pub fn registerAudioBackend(name: []const u8, backend: ?*const anyopaque) void {
    _ = registerAudioBackendInternal(name, backend, null, null, null, null, null, null, null);
}

pub fn registerExternalAudioBackendZ(name: [*:0]const u8, limits: BackendPcmLimits, context: ?*anyopaque, write_pcm: WritePcmCtxFn, stop_pcm: ?StopPcmCtxFn, status: ?StatusCtxFn) i32 {
    var buf: [MAX_NAME]u8 = undefined;
    const name_slice = copyZ(name, buf[0..]) orelse return -1;
    return if (registerAudioBackendInternal(name_slice, null, null, null, context, write_pcm, stop_pcm, status, limits)) 0 else -2;
}

pub fn registerExternalSynthEngineZ(name: [*:0]const u8, context: ?*anyopaque, send_midi: ?SynthMidiSendCtxFn, render: ?SynthRenderCtxFn, stop: ?SynthStopCtxFn, status: ?SynthStatusCtxFn, opl3_reset: ?SynthOpl3ResetCtxFn, opl3_write_register: ?SynthOpl3WriteRegisterCtxFn, sid_acquire: ?SynthSidAcquireCtxFn, sid_release: ?SynthSidReleaseCtxFn, sid_set_model: ?SynthSidSetModelCtxFn, sid_write_register: ?SynthSidWriteRegisterCtxFn, sid_load_data: ?SynthSidLoadDataCtxFn, sid_init: ?SynthSidInitCtxFn, sid_play_frame: ?SynthSidPlayFrameCtxFn, sid_render_pcm: ?SynthSidRenderPcmCtxFn) i32 {
    var buf: [MAX_NAME]u8 = undefined;
    const name_slice = copyZ(name, buf[0..]) orelse return -1;
    return if (registerSynthEngineInternal(name_slice, null, null, null, null, context, send_midi, render, stop, status, opl3_reset, opl3_write_register, sid_acquire, sid_release, sid_set_model, sid_write_register, sid_load_data, sid_init, sid_play_frame, sid_render_pcm, true, false)) 0 else -2;
}

pub fn unregisterAudioBackendByName(name: []const u8) i32 {
    const slot = findAudioBackend(name) orelse return -1;
    const was_active = active_audio_slot == slot;
    if (was_active) stopActivePcm();
    audio_backends[slot] = .{};
    if (was_active) {
        active_audio_slot = null;
        activateFirstNativeBackend();
    }
    bootlog.puts("[AUDIO] audio backend unregistered ");
    bootlog.puts(name);
    bootlog.puts(" [OK]\r\n");
    return 0;
}

pub fn unregisterAudioBackendZ(name: [*:0]const u8) i32 {
    var buf: [MAX_NAME]u8 = undefined;
    const name_slice = copyZ(name, buf[0..]) orelse return -1;
    return unregisterAudioBackendByName(name_slice);
}

pub fn registerSynthEngine(name: []const u8, engine: ?*const anyopaque) bool {
    return registerSynthEngineInternal(name, engine, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, false, nameEq(name, "SID"));
}

pub fn registerNativeSynthEngine(name: []const u8, send_midi: ?SynthMidiSendFn, render: ?SynthRenderFn, stop: ?SynthStopFn) void {
    _ = registerSynthEngineInternal(name, null, send_midi, render, stop, null, null, null, null, null, null, null, null, null, null, null, null, null, null, null, false, false);
}

fn registerSynthEngineInternal(name: []const u8, engine: ?*const anyopaque, send_midi: ?SynthMidiSendFn, render: ?SynthRenderFn, stop: ?SynthStopFn, context: ?*anyopaque, send_midi_ctx: ?SynthMidiSendCtxFn, render_ctx: ?SynthRenderCtxFn, stop_ctx: ?SynthStopCtxFn, status_ctx: ?SynthStatusCtxFn, opl3_reset_ctx: ?SynthOpl3ResetCtxFn, opl3_write_register_ctx: ?SynthOpl3WriteRegisterCtxFn, sid_acquire_ctx: ?SynthSidAcquireCtxFn, sid_release_ctx: ?SynthSidReleaseCtxFn, sid_set_model_ctx: ?SynthSidSetModelCtxFn, sid_write_register_ctx: ?SynthSidWriteRegisterCtxFn, sid_load_data_ctx: ?SynthSidLoadDataCtxFn, sid_init_ctx: ?SynthSidInitCtxFn, sid_play_frame_ctx: ?SynthSidPlayFrameCtxFn, sid_render_pcm_ctx: ?SynthSidRenderPcmCtxFn, external_contract: bool, migration_bridge: bool) bool {
    const slot = findSynthEngine(name) orelse freeSynthEngineSlot() orelse {
        bootlog.puts("[AUDIO][WARN] synth engine registry full\r\n");
        return false;
    };
    const entry = &synth_engines[slot];
    const sends = entry.sends;
    const renders = entry.renders;
    const stops = entry.stops;
    entry.* = .{
        .registered = true,
        .ptr = engine,
        .context = context,
        .midi_send = send_midi,
        .render = render,
        .stop = stop,
        .midi_send_ctx = send_midi_ctx,
        .render_ctx = render_ctx,
        .stop_ctx = stop_ctx,
        .status_ctx = status_ctx,
        .opl3_reset_ctx = opl3_reset_ctx,
        .opl3_write_register_ctx = opl3_write_register_ctx,
        .sid_acquire_ctx = sid_acquire_ctx,
        .sid_release_ctx = sid_release_ctx,
        .sid_set_model_ctx = sid_set_model_ctx,
        .sid_write_register_ctx = sid_write_register_ctx,
        .sid_load_data_ctx = sid_load_data_ctx,
        .sid_init_ctx = sid_init_ctx,
        .sid_play_frame_ctx = sid_play_frame_ctx,
        .sid_render_pcm_ctx = sid_render_pcm_ctx,
        .external_contract = external_contract,
        .migration_bridge = migration_bridge,
        .sends = sends,
        .renders = renders,
        .stops = stops,
    };
    entry.name_len = if (name.len < MAX_NAME) name.len else MAX_NAME - 1;
    if (entry.name_len > 0) @memcpy(entry.name[0..entry.name_len], name[0..entry.name_len]);
    bootlog.puts("[AUDIO] synth engine registered ");
    bootlog.puts(entry.name[0..entry.name_len]);
    if (external_contract) {
        bootlog.puts(" external");
    } else if (migration_bridge) {
        bootlog.puts(" migration");
    } else {
        bootlog.puts(if (send_midi != null or render != null) " native" else " generic");
    }
    bootlog.puts(" [OK]\r\n");
    return true;
}

pub fn registerNativeAudioBackend(name: []const u8, write_pcm: WritePcmFn, stop_pcm: ?StopPcmFn) void {
    _ = registerAudioBackendInternal(name, null, write_pcm, stop_pcm, null, null, null, null, null);
}

pub fn selectAudioBackend(name: []const u8) bool {
    const slot = findAudioBackend(name) orelse {
        bootlog.puts("[AUDIO][WARN] audio backend not found ");
        bootlog.puts(name);
        bootlog.puts("\r\n");
        return false;
    };
    if (audio_backends[slot].write_pcm == null) {
        bootlog.puts("[AUDIO][WARN] audio backend has no PCM output ");
        bootlog.puts(name);
        bootlog.puts("\r\n");
        return false;
    }
    if (active_audio_slot == slot) return true;
    stopActivePcm();
    setActiveAudioBackend(slot);
    return true;
}

pub fn unregisterAudioBackend(name: []const u8) bool {
    const slot = findAudioBackend(name) orelse return false;
    const was_active = active_audio_slot == slot;
    if (was_active) {
        stopActivePcm();
        active_audio_slot = null;
    }
    audio_backends[slot] = .{};
    bootlog.puts("[AUDIO] audio backend unregistered ");
    bootlog.puts(name);
    bootlog.puts("\r\n");
    if (was_active) activateFirstNativeBackend();
    return true;
}

pub fn audioBackendRegistered(name: []const u8) bool {
    return findAudioBackend(name) != null;
}

pub fn audioBackendActive(name: []const u8) bool {
    const slot = findAudioBackend(name) orelse return false;
    return active_audio_slot == slot and audio_backends[slot].active;
}

pub fn audioBackendHasOutput(name: []const u8) bool {
    const slot = findAudioBackend(name) orelse return false;
    return audio_backends[slot].write_pcm != null or audio_backends[slot].write_pcm_ctx != null;
}

pub fn performanceSummary() PerformanceSummary {
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return .{};
    defer _ = stream_lock.unlock();
    var out = PerformanceSummary{
        .total_stream_writes = total_stream_writes,
        .backend_ok = total_backend_ok,
        .backend_fail = total_backend_fail,
        .stream_available_high_water_bytes = stream_available_high_water,
        .stream_write_truncations = stream_write_truncations,
        .stream_dropped_bytes = stream_dropped_bytes,
        .stream_write_total_ticks = stream_write_total_ticks,
        .stream_write_max_ticks = stream_write_max_ticks,
        .stream_write_last_ticks = stream_write_last_ticks,
        .backend_write_calls = backend_write_calls,
        .backend_write_total_ticks = backend_write_total_ticks,
        .backend_write_max_ticks = backend_write_max_ticks,
        .backend_write_last_ticks = backend_write_last_ticks,
        .sid_writes = sid_write_count,
        .sid_pcm_writes = sid_pcm_writes,
        .sid_pcm_fail = sid_pcm_fail,
        .midi_sends = midi_send_count,
        .midi_renders = midi_render_ticks,
    };
    var i: usize = 0;
    while (i < streams.len) : (i += 1) {
        out.stream_ring_bytes +%= @intCast(streams[i].ring.len);
        switch (streams[i].state) {
            .open => {
                out.open_streams += 1;
                out.stream_available_bytes +%= @intCast(streams[i].available);
            },
            .closed => out.closed_streams += 1,
            .empty => {},
        }
    }
    i = 0;
    while (i < audio_backends.len) : (i += 1) {
        const backend = &audio_backends[i];
        if (!backend.registered) continue;
        out.registered_backends += 1;
        if (backend.active) out.active_backends += 1;
        if (backend.status_ctx) |status_fn| {
            var status: BackendStatus = .{ .active = if (backend.active) 1 else 0 };
            if (status_fn(backend.context, &status) == 0) {
                out.backend_status_writes +%= status.writes;
                out.backend_status_underruns +%= status.underruns;
                out.backend_status_errors +%= status.errors;
                out.backend_status_refills +%= status.refills;
                out.backend_status_silence_refills +%= status.silence_refills;
                out.backend_status_buffer_bytes +%= status.buffer_bytes;
                out.backend_status_queued_buffers +%= status.queued_buffers;
                out.backend_status_last_buffer_bytes +%= status.last_buffer_bytes;
                if (status.total_write_ticks > out.backend_write_total_ticks) out.backend_write_total_ticks = status.total_write_ticks;
                if (status.max_write_ticks > out.backend_write_max_ticks) out.backend_write_max_ticks = status.max_write_ticks;
                if (status.last_write_ticks > out.backend_write_last_ticks) out.backend_write_last_ticks = status.last_write_ticks;
                out.backend_status_refill_total_ticks +%= status.total_refill_ticks;
                if (status.max_refill_ticks > out.backend_status_refill_max_ticks) out.backend_status_refill_max_ticks = status.max_refill_ticks;
                if (status.last_refill_ticks > out.backend_status_refill_last_ticks) out.backend_status_refill_last_ticks = status.last_refill_ticks;
            }
        }
    }
    i = 0;
    while (i < synth_engines.len) : (i += 1) {
        if (synth_engines[i].registered) out.registered_synths += 1;
    }
    return out;
}

pub fn dumpStatus() void {
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return;
    defer _ = stream_lock.unlock();
    k.puts("Audio:\r\n");
    k.puts("  Backend: ");
    if (active_audio_slot) |slot| {
        const backend = &audio_backends[slot];
        k.puts(backend.name[0..backend.name_len]);
    } else {
        k.puts("none");
    }
    k.puts("\r\n");
    k.puts("  Mixer: ");
    if (mixer_backend.active) {
        k.puts(mixer_backend.name[0..mixer_backend.name_len]);
    } else {
        k.puts("none");
    }
    k.puts("\r\n");
    k.puts("  Writes: total=");
    k.putDec(total_stream_writes);
    k.puts(" backend_ok=");
    k.putDec(total_backend_ok);
    k.puts(" backend_fail=");
    k.putDec(total_backend_fail);
    k.puts("\r\n");
    dumpAudioBackends();
    dumpSynthEngines();
    dumpSidStatus();
    dumpMidiStatus();

    var open_count: u32 = 0;
    var i: usize = 0;
    while (i < streams.len) : (i += 1) {
        const s = &streams[i];
        if (s.state != .open) continue;
        open_count += 1;
        k.puts("  Stream ");
        k.putDec(s.id);
        k.puts(": rate=");
        k.putDec(s.rate);
        k.puts(" ch=");
        k.putDec(s.channels);
        k.puts(" fmt=");
        k.putDec(s.format);
        k.puts(" bytes=");
        k.putDec(s.total_written);
        k.puts("\r\n");
    }
    if (open_count == 0) k.puts("  Streams: none open\r\n");
}

fn dumpSidStatus() void {
    k.puts("  SID: available=");
    const engine = sidEngine();
    k.puts(if (engine != null) "yes" else "no");
    k.puts(" external=");
    k.puts(if (engine != null and engine.?.external_contract) "yes" else "no");
    k.puts(" acquired=");
    k.puts(if (sid_acquired) "yes" else "no");
    k.puts(" model=");
    k.puts(sidModelNameZ());
    k.puts(" writes=");
    k.putDec(sid_write_count);
    k.puts(" last_reg=");
    k.putDec(sid_last_register);
    k.puts(" last_val=");
    k.putDec(sid_last_value);
    k.puts(" last=");
    putSigned(sid_last_result);
    k.puts(" pcm=");
    k.putDec(sid_pcm_writes);
    k.puts(" ok=");
    k.putDec(sid_pcm_ok);
    k.puts(" fail=");
    k.putDec(sid_pcm_fail);
    if (engine) |synth| {
        if (synth.status_ctx) |status_fn| {
            var status = SynthStatus{};
            if (status_fn(synth.context, &status) == 0) {
                k.puts(" r4d_renders=");
                k.putDec(status.renders);
                k.puts(" r4d_stops=");
                k.putDec(status.stops);
                k.puts(" r4d_last=");
                putSigned(status.last_result);
            }
        }
    }
    k.puts("\r\n");
}

fn dumpMidiStatus() void {
    k.puts("  MIDI: available=");
    k.puts(if (findSynthEngine("MIDI") != null or findSynthEngine("OPL3") != null) "yes" else "no");
    k.puts(" acquired=");
    k.puts(if (midi_acquired) "yes" else "no");
    k.puts(" sends=");
    k.putDec(midi_send_count);
    k.puts(" last_ch=");
    k.putDec(midi_last_channel);
    k.puts(" last_status=");
    k.putDec(midi_last_status);
    k.puts(" data=");
    k.putDec(midi_last_data1);
    k.putc('/');
    k.putDec(midi_last_data2);
    k.puts(" render_ticks=");
    k.putDec(midi_render_ticks);
    k.puts(" source=");
    k.puts(r4p.requiredSourceName("audio.midi"));
    k.puts(" r4p=");
    k.putDec(midi_r4p_classify);
    k.puts("/");
    k.putDec(midi_dispatch_failures);
    k.puts(" required_missing=");
    k.putDec(midi_missing_required);
    k.puts(" event=");
    k.putDec(midi_last_event);
    k.puts("\r\n");
    k.puts("  OPL3 engine: external=");
    k.puts(if (findSynthEngine("OPL3") != null) "yes" else "no");
    k.puts(" source=");
    k.puts(r4p.requiredSourceName("audio.opl3"));
    k.puts(" r4p=");
    k.putDec(opl3_r4p_register);
    k.puts("/");
    k.putDec(opl3_r4p_midi);
    k.puts("/");
    k.putDec(opl3_dispatch_failures);
    k.puts(" required_missing=");
    k.putDec(opl3_missing_required);
    k.puts(" kind=");
    k.putDec(opl3_last_write_kind);
    k.puts(" action=");
    k.putDec(opl3_last_action);
    k.puts("\r\n");
}

fn sendActiveMidiSynth(channel: u8, status: u8, data1: u8, data2: u8) void {
    const slot = active_midi_synth_slot orelse return;
    const engine = &synth_engines[slot];
    if (engine.midi_send_ctx) |send| {
        if (send(engine.context, channel, status, data1, data2) >= 0) {
            engine.sends += 1;
        }
        return;
    }
    if (engine.midi_send) |send| {
        send(channel, status, data1, data2);
        engine.sends += 1;
    }
}

fn classifyMidiEvent(channel: u8, status: u8, data1: u8, data2: u8) ?r4p_contract.AudioMidiOp {
    if (classifyMidiEventR4p(channel, status, data1, data2)) |op| {
        midi_last_event = op.event;
        return op;
    }
    midi_missing_required +%= 1;
    midi_last_result = -5;
    midi_last_event = r4p_contract.AUDIO_MIDI_EVENT_IGNORE;
    return null;
}

fn classifyMidiEventR4p(channel: u8, status: u8, data1: u8, data2: u8) ?r4p_contract.AudioMidiOp {
    if (!r4p.hasActiveR4p("audio.midi")) return null;
    var op = r4p_contract.AudioMidiOp{
        .channel = channel,
        .status = status,
        .data1 = data1,
        .data2 = data2,
    };
    if (!dispatchMidi(r4p_contract.AUDIO_MIDI_OP_CLASSIFY_EVENT, &op) or op.result != r4p_contract.AUDIO_MIDI_RESULT_OK) return null;
    midi_r4p_classify +%= 1;
    return op;
}

fn dispatchMidi(opcode: u32, op: *r4p_contract.AudioMidiOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = op,
        .len = @sizeOf(r4p_contract.AudioMidiOp),
        .capacity = @sizeOf(r4p_contract.AudioMidiOp),
        .flags = 0,
        .reserved = 0,
    };
    var out: protocol_api.ProtocolBuffer = .{};
    const result = r4p.dispatch("audio.midi", opcode, &buffer, &out);
    midi_last_result = result;
    if (result != r4p_contract.AUDIO_MIDI_RESULT_OK) {
        midi_dispatch_failures +%= 1;
        return false;
    }
    return true;
}

fn classifyOpl3Register(bank: u8, reg: u8, value: u8) ?r4p_contract.AudioOpl3Op {
    if (classifyOpl3RegisterR4p(bank, reg, value)) |op| {
        opl3_last_write_kind = op.write_kind;
        return op;
    }
    opl3_missing_required +%= 1;
    opl3_last_result = -5;
    opl3_last_write_kind = r4p_contract.AUDIO_OPL3_WRITE_OTHER;
    return null;
}

fn classifyOpl3RegisterR4p(bank: u8, reg: u8, value: u8) ?r4p_contract.AudioOpl3Op {
    if (!r4p.hasActiveR4p("audio.opl3")) return null;
    var op = r4p_contract.AudioOpl3Op{
        .bank = bank,
        .register = reg,
        .value = value,
    };
    if (!dispatchOpl3(r4p_contract.AUDIO_OPL3_OP_WRITE_REGISTER, &op) or op.result != r4p_contract.AUDIO_OPL3_RESULT_OK) return null;
    opl3_r4p_register +%= 1;
    opl3_last_write_kind = op.write_kind;
    return op;
}

fn classifyOpl3Midi(channel: u8, status: u8, data1: u8, data2: u8) ?r4p_contract.AudioOpl3Op {
    if (classifyOpl3MidiR4p(channel, status, data1, data2)) |op| {
        opl3_last_action = op.action;
        return op;
    }
    opl3_missing_required +%= 1;
    opl3_last_result = -5;
    opl3_last_action = r4p_contract.AUDIO_OPL3_ACTION_IGNORE;
    return null;
}

fn classifyOpl3MidiR4p(channel: u8, status: u8, data1: u8, data2: u8) ?r4p_contract.AudioOpl3Op {
    if (!r4p.hasActiveR4p("audio.opl3")) return null;
    var op = r4p_contract.AudioOpl3Op{
        .channel = channel,
        .status = status,
        .data1 = data1,
        .data2 = data2,
    };
    if (!dispatchOpl3(r4p_contract.AUDIO_OPL3_OP_MIDI_EVENT, &op) or op.result != r4p_contract.AUDIO_OPL3_RESULT_OK) return null;
    opl3_r4p_midi +%= 1;
    opl3_last_action = op.action;
    return op;
}

fn dispatchOpl3(opcode: u32, op: *r4p_contract.AudioOpl3Op) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = op,
        .len = @sizeOf(r4p_contract.AudioOpl3Op),
        .capacity = @sizeOf(r4p_contract.AudioOpl3Op),
        .flags = 0,
        .reserved = 0,
    };
    var out: protocol_api.ProtocolBuffer = .{};
    const result = r4p.dispatch("audio.opl3", opcode, &buffer, &out);
    opl3_last_result = result;
    if (result != r4p_contract.AUDIO_OPL3_RESULT_OK) {
        opl3_dispatch_failures +%= 1;
        return false;
    }
    return true;
}

fn classifySidModel(model: u8) ?r4p_contract.AudioSidOp {
    if (!r4p.hasActiveR4p("audio.sid")) {
        sid_missing_required +%= 1;
        sid_last_result = -5;
        return null;
    }
    var op = r4p_contract.AudioSidOp{ .model = model };
    if (!dispatchSid(r4p_contract.AUDIO_SID_OP_CONFIGURE_MODEL, &op) or op.result != r4p_contract.AUDIO_SID_RESULT_OK) return null;
    sid_r4p_model +%= 1;
    return op;
}

fn classifySidRegister(register: u8, value: u8) ?r4p_contract.AudioSidOp {
    if (!r4p.hasActiveR4p("audio.sid")) {
        sid_missing_required +%= 1;
        sid_last_result = -5;
        sid_last_kind = r4p_contract.AUDIO_SID_REGISTER_OTHER;
        sid_last_voice = 0;
        return null;
    }
    var op = r4p_contract.AudioSidOp{
        .register = register,
        .value = value,
    };
    if (!dispatchSid(r4p_contract.AUDIO_SID_OP_WRITE_REGISTER, &op) or op.result != r4p_contract.AUDIO_SID_RESULT_OK) return null;
    sid_r4p_register +%= 1;
    sid_last_kind = op.kind;
    sid_last_voice = op.voice;
    return op;
}

fn resolveSidIoAddress(address: u16, value: u8) ?r4p_contract.AudioSidOp {
    if (!r4p.hasActiveR4p("audio.sid")) {
        sid_missing_required +%= 1;
        sid_last_result = -5;
        sid_last_kind = r4p_contract.AUDIO_SID_REGISTER_OTHER;
        sid_last_voice = 0;
        return null;
    }
    var op = r4p_contract.AudioSidOp{
        .address = address,
        .value = value,
    };
    if (!dispatchSid(r4p_contract.AUDIO_SID_OP_RESOLVE_IO, &op) or op.result != r4p_contract.AUDIO_SID_RESULT_OK) return null;
    sid_r4p_io +%= 1;
    sid_last_kind = op.kind;
    sid_last_voice = op.voice;
    return op;
}

fn dispatchSid(opcode: u32, op: *r4p_contract.AudioSidOp) bool {
    var buffer = protocol_api.ProtocolBuffer{
        .data = op,
        .len = @sizeOf(r4p_contract.AudioSidOp),
        .capacity = @sizeOf(r4p_contract.AudioSidOp),
        .flags = 0,
        .reserved = 0,
    };
    var out: protocol_api.ProtocolBuffer = .{};
    const result = r4p.dispatch("audio.sid", opcode, &buffer, &out);
    sid_last_result = result;
    if (result != r4p_contract.AUDIO_SID_RESULT_OK) {
        sid_dispatch_failures +%= 1;
        return false;
    }
    return true;
}

fn resetSidProtocolCounters() void {
    sid_r4p_model = 0;
    sid_r4p_register = 0;
    sid_r4p_io = 0;
    sid_missing_required = 0;
    sid_dispatch_failures = 0;
    sid_last_result = 0;
    sid_last_kind = 0;
    sid_last_voice = 0;
}

fn renderActiveMidiSynth() void {
    const slot = active_midi_synth_slot orelse return;
    const engine = &synth_engines[slot];
    midi_render_ticks +%= 1;
    if (engine.render_ctx) |render| {
        if (render(engine.context) >= 0) {
            engine.renders += 1;
        }
        return;
    }
    if (engine.render) |render| {
        render();
        engine.renders += 1;
    }
}

fn stopActiveMidiSynth() void {
    const slot = active_midi_synth_slot orelse return;
    const engine = &synth_engines[slot];
    if (engine.stop_ctx) |stop| {
        if (stop(engine.context) >= 0) {
            engine.stops += 1;
        }
        return;
    }
    if (engine.stop) |stop| {
        stop();
        engine.stops += 1;
    }
}

fn registerMixerBackend(name: []const u8, backend: ?*const anyopaque) void {
    storeBackend(&mixer_backend, name, backend);
    bootlog.puts("[AUDIO] mixer backend ");
    bootlog.puts(name);
    bootlog.puts(" [OK]\r\n");
}

fn registerAudioBackendInternal(name: []const u8, backend: ?*const anyopaque, write_pcm: ?WritePcmFn, stop_pcm: ?StopPcmFn, context: ?*anyopaque, write_pcm_ctx: ?WritePcmCtxFn, stop_pcm_ctx: ?StopPcmCtxFn, status_ctx: ?StatusCtxFn, pcm_limits: ?BackendPcmLimits) bool {
    const slot = findAudioBackend(name) orelse freeAudioBackendSlot() orelse {
        bootlog.puts("[AUDIO][WARN] audio backend registry full\r\n");
        return false;
    };

    const was_active = active_audio_slot == slot;
    const entry = &audio_backends[slot];
    entry.* = .{
        .registered = true,
        .active = false,
        .ptr = backend,
        .context = context,
        .write_pcm = write_pcm,
        .stop_pcm = stop_pcm,
        .write_pcm_ctx = write_pcm_ctx,
        .stop_pcm_ctx = stop_pcm_ctx,
        .status_ctx = status_ctx,
        .pcm_limits = pcm_limits,
    };
    entry.name_len = if (name.len < MAX_NAME) name.len else MAX_NAME - 1;
    if (entry.name_len > 0) @memcpy(entry.name[0..entry.name_len], name[0..entry.name_len]);

    bootlog.puts("[AUDIO] audio backend registered ");
    bootlog.puts(entry.name[0..entry.name_len]);
    bootlog.puts(if (write_pcm != null) " native" else if (write_pcm_ctx != null) " r4d" else " generic");
    bootlog.puts(" [OK]\r\n");

    if (was_active or (active_audio_slot == null and (write_pcm != null or write_pcm_ctx != null))) setActiveAudioBackend(slot);
    return true;
}

fn setActiveAudioBackend(slot: usize) void {
    var i: usize = 0;
    while (i < audio_backends.len) : (i += 1) {
        audio_backends[i].active = false;
    }
    audio_backends[slot].active = true;
    active_audio_slot = slot;
    bootlog.puts("[AUDIO] active audio backend ");
    bootlog.puts(audio_backends[slot].name[0..audio_backends[slot].name_len]);
    bootlog.puts(" [OK]\r\n");
}

fn writeActivePcm(data: []const u8, rate: u32, channels: u16, format: u16) ?i32 {
    const slot = active_audio_slot orelse return null;
    const backend = &audio_backends[slot];
    if (backend.pcm_limits) |limits| {
        if (!limits.accepts(rate, channels, format)) return r4x_api.service_api_result_invalid;
    }
    if (backend.write_pcm) |write_pcm| return if (write_pcm(data, rate, channels, format)) 0 else -1;
    if (backend.write_pcm_ctx) |write_pcm_ctx| return write_pcm_ctx(backend.context, data.ptr, @intCast(data.len), rate, channels, format);
    return null;
}

fn stopActivePcm() void {
    _ = stopActivePcmResult();
}

fn stopActivePcmResult() i32 {
    const slot = active_audio_slot orelse return 0;
    const backend = &audio_backends[slot];
    if (backend.stop_pcm) |stop_pcm| {
        stop_pcm();
        return 0;
    }
    if (backend.stop_pcm_ctx) |stop_pcm_ctx| return stop_pcm_ctx(backend.context);
    return 0;
}

fn activateFirstNativeBackend() void {
    var i: usize = 0;
    while (i < audio_backends.len) : (i += 1) {
        if (!audio_backends[i].registered or (audio_backends[i].write_pcm == null and audio_backends[i].write_pcm_ctx == null)) continue;
        setActiveAudioBackend(i);
        return;
    }
    bootlog.puts("[AUDIO][WARN] no active audio backend\r\n");
}

fn dumpAudioBackends() void {
    k.puts("  Backends:");
    var count: u32 = 0;
    var i: usize = 0;
    while (i < audio_backends.len) : (i += 1) {
        const backend = &audio_backends[i];
        if (!backend.registered) continue;
        count += 1;
        k.puts(" ");
        if (backend.active) k.puts("*");
        k.puts(backend.name[0..backend.name_len]);
        k.puts(if (backend.write_pcm != null) "(native)" else if (backend.write_pcm_ctx != null) "(r4d)" else "(generic)");
    }
    if (count == 0) k.puts(" none");
    k.puts("\r\n");
    dumpAudioBackendStatuses();
}

fn dumpAudioBackendStatuses() void {
    var i: usize = 0;
    while (i < audio_backends.len) : (i += 1) {
        const backend = &audio_backends[i];
        if (!backend.registered) continue;
        const status_fn = backend.status_ctx orelse continue;
        var status: BackendStatus = .{ .active = if (backend.active) 1 else 0 };
        const result = status_fn(backend.context, &status);
        k.puts("  Backend ");
        k.puts(backend.name[0..backend.name_len]);
        k.puts(" status: ");
        if (result != 0) {
            k.puts("rc=");
            putSigned(result);
            k.puts("\r\n");
            continue;
        }
        k.puts("active=");
        k.puts(if (status.active != 0) "yes" else "no");
        k.puts(" writes=");
        k.putDec(status.writes);
        k.puts(" underruns=");
        k.putDec(status.underruns);
        k.puts(" errors=");
        k.putDec(status.errors);
        k.puts(" last=");
        putSigned(status.last_result);
        k.puts("\r\n");
    }
}

fn dumpSynthEngines() void {
    k.puts("  Synths:");
    var count: u32 = 0;
    var i: usize = 0;
    while (i < synth_engines.len) : (i += 1) {
        const synth = &synth_engines[i];
        if (!synth.registered) continue;
        count += 1;
        k.puts(" ");
        if (active_midi_synth_slot == i) k.puts("*");
        k.puts(synth.name[0..synth.name_len]);
        k.puts("(");
        if (synth.external_contract) k.puts("r4d:");
        if (synth.migration_bridge) k.puts("migration:");
        if (synth.midi_send != null or synth.midi_send_ctx != null) k.puts("midi");
        if (synth.render != null or synth.render_ctx != null) k.puts("+pcm");
        if (synth.midi_send == null and synth.render == null and synth.midi_send_ctx == null and synth.render_ctx == null) k.puts("generic");
        k.puts(")");
    }
    if (count == 0) k.puts(" none");
    k.puts("\r\n");
    dumpSynthEngineCounters();
}

fn dumpSynthEngineCounters() void {
    var i: usize = 0;
    while (i < synth_engines.len) : (i += 1) {
        const synth = &synth_engines[i];
        if (!synth.registered) continue;
        if (synth.sends == 0 and synth.renders == 0 and synth.stops == 0) continue;
        k.puts("  Synth ");
        k.puts(synth.name[0..synth.name_len]);
        k.puts(": sends=");
        k.putDec(synth.sends);
        k.puts(" renders=");
        k.putDec(synth.renders);
        k.puts(" stops=");
        k.putDec(synth.stops);
        k.puts("\r\n");
    }
}

fn findAudioBackend(name: []const u8) ?usize {
    var i: usize = 0;
    while (i < audio_backends.len) : (i += 1) {
        const backend = &audio_backends[i];
        if (!backend.registered) continue;
        if (nameEq(backend.name[0..backend.name_len], name)) return i;
    }
    return null;
}

fn findSynthEngine(name: []const u8) ?usize {
    var i: usize = 0;
    while (i < synth_engines.len) : (i += 1) {
        const synth = &synth_engines[i];
        if (!synth.registered) continue;
        if (nameEq(synth.name[0..synth.name_len], name)) return i;
    }
    return null;
}

fn sidEngine() ?*SynthEngine {
    const slot = findSynthEngine("SID") orelse return null;
    return &synth_engines[slot];
}

fn sidModelId() u32 {
    if (nameEq(sid_model_option, "6581")) return r4p_contract.AUDIO_SID_MODEL_6581;
    return r4p_contract.AUDIO_SID_MODEL_8580;
}

fn findRenderableSynthEngine() ?usize {
    if (findSynthEngine("OPL3") == null and findSynthEngine("MIDI") == null) return null;
    if (findSynthEngine("OPL3")) |slot| {
        if (synthCanRenderMidi(slot)) return slot;
    }
    if (findSynthEngine("MIDI")) |slot| {
        if (synthCanRenderMidi(slot)) return slot;
    }
    var i: usize = 0;
    while (i < synth_engines.len) : (i += 1) {
        if (!synth_engines[i].registered) continue;
        if (synthCanRenderMidi(i)) return i;
    }
    return null;
}

fn synthCanRenderMidi(slot: usize) bool {
    const synth = &synth_engines[slot];
    return (synth.midi_send != null or synth.midi_send_ctx != null) and
        (synth.render != null or synth.render_ctx != null);
}

fn freeAudioBackendSlot() ?usize {
    var i: usize = 0;
    while (i < audio_backends.len) : (i += 1) {
        if (!audio_backends[i].registered) return i;
    }
    return null;
}

fn recordTickStat(total: *u64, max: *u64, last: *u64, start_tick: u64) void {
    const now = timer.tickCount();
    const elapsed = if (now >= start_tick) now - start_tick else 0;
    total.* +%= elapsed;
    last.* = elapsed;
    if (elapsed > max.*) max.* = elapsed;
}

fn putSigned(value: i32) void {
    if (value < 0) {
        k.puts("-");
        k.putDec(@intCast(-value));
    } else {
        k.putDec(@intCast(value));
    }
}

fn freeSynthEngineSlot() ?usize {
    var i: usize = 0;
    while (i < synth_engines.len) : (i += 1) {
        if (!synth_engines[i].registered) return i;
    }
    return null;
}

fn registerZ(target: *NamedBackend, name_z: [*:0]const u8, backend: *const anyopaque, kind: []const u8) i32 {
    var buf: [MAX_NAME]u8 = undefined;
    const name = copyZ(name_z, buf[0..]) orelse return -1;
    storeBackend(target, name, backend);
    bootlog.puts("[AUDIO] ");
    bootlog.puts(kind);
    bootlog.puts(" backend ");
    bootlog.puts(name);
    bootlog.puts(" [OK]\r\n");
    return 0;
}

fn storeBackend(target: *NamedBackend, name: []const u8, backend: ?*const anyopaque) void {
    target.* = .{ .active = true, .ptr = backend };
    target.name_len = if (name.len < MAX_NAME) name.len else MAX_NAME - 1;
    if (target.name_len > 0) @memcpy(target.name[0..target.name_len], name[0..target.name_len]);
}

fn validStreamFormat(rate: u32, channels: u16, format: u16) bool {
    if (rate == 0 or rate > 192_000) return false;
    if (channels == 0 or channels > 2) return false;
    return format == FORMAT_S16LE or format == FORMAT_U8;
}

fn activeOutputPresent() bool {
    const slot = active_audio_slot orelse return false;
    const backend = &audio_backends[slot];
    return backend.registered and backend.active and
        (backend.write_pcm != null or backend.write_pcm_ctx != null);
}

fn freeStreamSlotLocked() ?usize {
    var i: usize = 0;
    while (i < streams.len) : (i += 1) {
        if (streams[i].state == .empty or streams[i].state == .closed) return i;
    }
    return null;
}

fn streamByOwnerLocked(owner: StreamOwner, id: u32) ?*Stream {
    var i: usize = 0;
    while (i < streams.len) : (i += 1) {
        if (streams[i].state == .open and streams[i].id == id and mixer.sameOwner(streams[i].owner, owner)) return &streams[i];
    }
    return null;
}

fn openStreamCountLocked() u32 {
    var count: u32 = 0;
    var i: usize = 0;
    while (i < streams.len) : (i += 1) {
        if (streams[i].state == .open) count += 1;
    }
    return count;
}

fn releaseStreamLocked(stream: *Stream) bool {
    const id = stream.id;
    const reusable_ring = stream.ring;
    stream.* = .{ .state = .closed, .id = id, .ring = reusable_ring };
    return true;
}

fn pumpAvailableLocked(force: bool) i32 {
    const max_passes = MAX_STREAMS * (RING_BYTES / MIX_QUANTUM_BYTES + 2);
    var pass: usize = 0;
    while (pass < max_passes) : (pass += 1) {
        var selected: [MAX_STREAMS]bool = .{false} ** MAX_STREAMS;
        var open_count: u32 = 0;
        var ready_count: u32 = 0;
        var single_ready: usize = 0;
        var chunk_bytes: usize = MIX_QUANTUM_BYTES;

        for (&streams, 0..) |*stream, index| {
            if (stream.state != .open) continue;
            open_count += 1;
            if (stream.available < pcm.TARGET_FRAME_BYTES) continue;
            ready_count += 1;
            single_ready = index;
        }
        if (ready_count == 0) return 0;

        if (ready_count == 1) {
            const stream = &streams[single_ready];
            if (open_count > 1 and !force and !stream.deferred_once) {
                stream.deferred_once = true;
                return 0;
            }
            selected[single_ready] = true;
            chunk_bytes = @min(chunk_bytes, stream.available);
        } else {
            for (&streams, 0..) |*stream, index| {
                if (stream.state != .open or stream.available < pcm.TARGET_FRAME_BYTES) continue;
                selected[index] = true;
                chunk_bytes = @min(chunk_bytes, stream.available);
            }
        }

        chunk_bytes -= chunk_bytes % pcm.TARGET_FRAME_BYTES;
        if (chunk_bytes == 0) return 0;
        var byte_offset: usize = 0;
        while (byte_offset < chunk_bytes) : (byte_offset += 2) {
            var total: i64 = 0;
            for (&streams, 0..) |*stream, index| {
                if (!selected[index]) continue;
                total = mixer.accumulateSample(
                    total,
                    mixer.ringReadS16(stream.ring, stream.read_pos, byte_offset),
                    stream.volume,
                );
            }
            mixer.writeS16(mix_scratch[0..], byte_offset, mixer.clampSample(total));
        }

        const backend_start = timer.tickCount();
        const result = writeActivePcm(
            mix_scratch[0..chunk_bytes],
            pcm.TARGET_RATE,
            pcm.TARGET_CHANNELS,
            pcm.FORMAT_S16LE,
        ) orelse r4x_api.service_api_result_no_endpoint;
        backend_write_calls +%= 1;
        recordTickStat(&backend_write_total_ticks, &backend_write_max_ticks, &backend_write_last_ticks, backend_start);
        if (result != 0) {
            if (result != r4x_api.service_api_result_busy) total_backend_fail +%= 1;
            return result;
        }
        total_backend_ok +%= 1;
        for (&streams, 0..) |*stream, index| {
            if (!selected[index]) continue;
            mixer.ringConsume(stream.ring.len, &stream.read_pos, &stream.available, chunk_bytes);
            stream.deferred_once = false;
        }
    }
    return r4x_api.service_api_result_busy;
}

fn stopPcmIfNoStreams() void {
    if (!stream_lock.lock(sync.WAIT_FOREVER)) return;
    defer _ = stream_lock.unlock();
    if (openStreamCountLocked() == 0) stopActivePcm();
}

fn copyZ(ptr: [*:0]const u8, out: []u8) ?[]const u8 {
    var len: usize = 0;
    while (len < out.len and ptr[len] != 0) : (len += 1) out[len] = ptr[len];
    if (len == out.len) return null;
    return out[0..len];
}

fn nameEq(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var i: usize = 0;
    while (i < a.len) : (i += 1) {
        if (upper(a[i]) != upper(b[i])) return false;
    }
    return true;
}

fn upper(c: u8) u8 {
    if (c >= 'a' and c <= 'z') return c - ('a' - 'A');
    return c;
}
