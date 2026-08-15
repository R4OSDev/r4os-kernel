const audio = @import("../audio/core.zig");

pub const name = "R4AUDIO";

pub fn openStream(rate: u32, channels: u16, format: u16) callconv(.c) i32 {
    return audio.openStream(rate, channels, format);
}

pub fn write(stream_id: u32, data_ptr: [*]const u8, byte_count: u32) callconv(.c) i32 {
    return audio.writeStream(stream_id, data_ptr, byte_count);
}

pub fn close(stream_id: u32) callconv(.c) i32 {
    return audio.closeStream(stream_id);
}

pub fn setVolume(stream_id: u32, fixed_volume: u32) callconv(.c) i32 {
    return audio.setVolume(stream_id, fixed_volume);
}

pub fn sidAcquire() callconv(.c) i32 {
    return audio.sidAcquire();
}

pub fn sidWriteRegister(handle: u32, register: u8, value: u8) callconv(.c) i32 {
    return audio.sidWriteRegister(handle, register, value);
}

pub fn sidRelease(handle: u32) callconv(.c) i32 {
    return audio.sidRelease(handle);
}

pub fn sidLoadData(handle: u32, load_addr: u16, data_ptr: [*]const u8, byte_count: u32) callconv(.c) i32 {
    return audio.sidLoadData(handle, load_addr, data_ptr, byte_count);
}

pub fn sidInit(handle: u32, init_addr: u16, song: u16) callconv(.c) i32 {
    return audio.sidInit(handle, init_addr, song);
}

pub fn sidPlayFrame(handle: u32, play_addr: u16, frame_hz: u16) callconv(.c) i32 {
    return audio.sidPlayFrame(handle, play_addr, frame_hz);
}

pub fn sidStop(handle: u32) callconv(.c) i32 {
    return audio.sidStop(handle);
}

pub fn sidModelName() callconv(.c) [*:0]const u8 {
    return audio.sidModelNameZ();
}

pub fn midiOpenSynth(backend: [*:0]const u8) callconv(.c) i32 {
    return audio.midiOpenSynth(backend);
}

pub fn midiSend(handle: u32, channel: u8, status: u8, data1: u8, data2: u8) callconv(.c) i32 {
    return audio.midiSend(handle, channel, status, data1, data2);
}

pub fn midiClose(handle: u32) callconv(.c) i32 {
    return audio.midiClose(handle);
}

pub fn opl3WriteRegister(bank: u8, reg: u8, value: u8) callconv(.c) i32 {
    return audio.opl3WriteRegister(bank, reg, value);
}

pub fn opl3Reset() callconv(.c) i32 {
    return audio.opl3Reset();
}

pub fn opl3RenderBlock() callconv(.c) i32 {
    return audio.opl3RenderBlock();
}

pub fn opl3Stop() callconv(.c) i32 {
    return audio.opl3Stop();
}
