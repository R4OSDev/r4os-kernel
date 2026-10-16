pub const name = "R4AUDIO";
extern fn r4os_rust_r4audio_open_stream(rate: u32, channels: u16, format: u16) i32;
pub fn openStream(rate: u32, channels: u16, format: u16) callconv(.c) i32 {
    return r4os_rust_r4audio_open_stream(rate, channels, format);
}
extern fn r4os_rust_r4audio_write(stream_id: u32, data_ptr: [*]const u8, byte_count: u32) i32;
pub fn write(stream_id: u32, data_ptr: [*]const u8, byte_count: u32) callconv(.c) i32 {
    return r4os_rust_r4audio_write(stream_id, data_ptr, byte_count);
}
extern fn r4os_rust_r4audio_close(stream_id: u32) i32;
pub fn close(stream_id: u32) callconv(.c) i32 {
    return r4os_rust_r4audio_close(stream_id);
}
extern fn r4os_rust_r4audio_set_volume(stream_id: u32, fixed_volume: u32) i32;
pub fn setVolume(stream_id: u32, fixed_volume: u32) callconv(.c) i32 {
    return r4os_rust_r4audio_set_volume(stream_id, fixed_volume);
}
extern fn r4os_rust_r4audio_sid_acquire() i32;
pub fn sidAcquire() callconv(.c) i32 {
    return r4os_rust_r4audio_sid_acquire();
}
extern fn r4os_rust_r4audio_sid_write_register(handle: u32, register: u8, value: u8) i32;
pub fn sidWriteRegister(handle: u32, register: u8, value: u8) callconv(.c) i32 {
    return r4os_rust_r4audio_sid_write_register(handle, register, value);
}
extern fn r4os_rust_r4audio_sid_release(handle: u32) i32;
pub fn sidRelease(handle: u32) callconv(.c) i32 {
    return r4os_rust_r4audio_sid_release(handle);
}
extern fn r4os_rust_r4audio_sid_load_data(handle: u32, load_addr: u16, data_ptr: [*]const u8, byte_count: u32) i32;
pub fn sidLoadData(handle: u32, load_addr: u16, data_ptr: [*]const u8, byte_count: u32) callconv(.c) i32 {
    return r4os_rust_r4audio_sid_load_data(handle, load_addr, data_ptr, byte_count);
}
extern fn r4os_rust_r4audio_sid_init(handle: u32, init_addr: u16, song: u16) i32;
pub fn sidInit(handle: u32, init_addr: u16, song: u16) callconv(.c) i32 {
    return r4os_rust_r4audio_sid_init(handle, init_addr, song);
}
extern fn r4os_rust_r4audio_sid_play_frame(handle: u32, play_addr: u16, frame_hz: u16) i32;
pub fn sidPlayFrame(handle: u32, play_addr: u16, frame_hz: u16) callconv(.c) i32 {
    return r4os_rust_r4audio_sid_play_frame(handle, play_addr, frame_hz);
}
extern fn r4os_rust_r4audio_sid_stop(handle: u32) i32;
pub fn sidStop(handle: u32) callconv(.c) i32 {
    return r4os_rust_r4audio_sid_stop(handle);
}
extern fn r4os_rust_r4audio_sid_model_name() [*:0]const u8;
pub fn sidModelName() callconv(.c) [*:0]const u8 {
    return r4os_rust_r4audio_sid_model_name();
}
extern fn r4os_rust_r4audio_midi_open_synth(backend: [*:0]const u8) i32;
pub fn midiOpenSynth(backend: [*:0]const u8) callconv(.c) i32 {
    return r4os_rust_r4audio_midi_open_synth(backend);
}
extern fn r4os_rust_r4audio_midi_send(handle: u32, channel: u8, status: u8, data1: u8, data2: u8) i32;
pub fn midiSend(handle: u32, channel: u8, status: u8, data1: u8, data2: u8) callconv(.c) i32 {
    return r4os_rust_r4audio_midi_send(handle, channel, status, data1, data2);
}
extern fn r4os_rust_r4audio_midi_close(handle: u32) i32;
pub fn midiClose(handle: u32) callconv(.c) i32 {
    return r4os_rust_r4audio_midi_close(handle);
}
extern fn r4os_rust_r4audio_opl3_write_register(bank: u8, reg: u8, value: u8) i32;
pub fn opl3WriteRegister(bank: u8, reg: u8, value: u8) callconv(.c) i32 {
    return r4os_rust_r4audio_opl3_write_register(bank, reg, value);
}
extern fn r4os_rust_r4audio_opl3_reset() i32;
pub fn opl3Reset() callconv(.c) i32 {
    return r4os_rust_r4audio_opl3_reset();
}
extern fn r4os_rust_r4audio_opl3_render_block() i32;
pub fn opl3RenderBlock() callconv(.c) i32 {
    return r4os_rust_r4audio_opl3_render_block();
}
extern fn r4os_rust_r4audio_opl3_stop() i32;
pub fn opl3Stop() callconv(.c) i32 {
    return r4os_rust_r4audio_opl3_stop();
}
