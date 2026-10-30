use crate::audio::core as audio;
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_open_stream(
    rate: u32,
    channels: u16,
    format: u16,
) -> i32 {
    unsafe { audio::open_stream(rate, channels, format) }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_write(
    stream_id: u32,
    data_ptr: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe { audio::write_stream(stream_id, data_ptr, byte_count) }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_close(stream_id: u32) -> i32 {
    unsafe { audio::close_stream(stream_id) }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_set_volume(stream_id: u32, fixed_volume: u32) -> i32 {
    unsafe { audio::set_volume(stream_id, fixed_volume) }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_acquire() -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::sid_acquire_for_owner(owner)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_write_register(
    handle: u32,
    register: u8,
    value: u8,
) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::sid_write_register_for_owner(owner, handle, register, value)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_release(handle: u32) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::sid_release_for_owner(owner, handle)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_load_data(
    handle: u32,
    load_addr: u16,
    data_ptr: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::sid_load_data_for_owner(owner, handle, load_addr, data_ptr, byte_count)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_init(handle: u32, init_addr: u16, song: u16) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::sid_init_for_owner(owner, handle, init_addr, song)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_play_frame(
    handle: u32,
    play_addr: u16,
    frame_hz: u16,
) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::sid_play_frame_for_owner(owner, handle, play_addr, frame_hz)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_stop(handle: u32) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::sid_stop_for_owner(owner, handle)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_sid_model_name() -> *const u8 {
    unsafe { audio::sid_model_name_z() }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_midi_open_synth(backend: *const u8) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::midi_open_synth_for_owner(owner, backend)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_midi_send(
    handle: u32,
    channel: u8,
    status: u8,
    data1: u8,
    data2: u8,
) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::midi_send_for_owner(owner, handle, channel, status, data1, data2)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_midi_close(handle: u32) -> i32 {
    unsafe {
        let Some(owner) = super::domain_api::audio_owner() else {
            return crate::contract::service_api_result_invalid;
        };
        audio::midi_close_for_owner(owner, handle)
    }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_opl3_write_register(
    bank: u8,
    reg: u8,
    value: u8,
) -> i32 {
    unsafe { audio::opl3_write_register(bank, reg, value) }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_opl3_reset() -> i32 {
    unsafe { audio::opl3_reset() }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_opl3_render_block() -> i32 {
    unsafe { audio::opl3_render_block() }
}
/// # Safety
/// Original R4AUDIO caller retains readable PCM/name spans until return.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_r4audio_opl3_stop() -> i32 {
    unsafe { audio::opl3_stop() }
}
