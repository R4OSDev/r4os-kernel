//! Fixed stateless projections for still-Zig kernel consumers, removed in.27.
use super::{core as native, mixer::Owner, types::*};
use crate::contract as a;
use core::slice;
unsafe fn bytes<'a>(p: usize, n: usize) -> &'a [u8] {
    if n == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(p as *const u8, n) }
    }
}
/// # Safety
/// Original kernel callers supply exact live fixed records, callbacks and spans.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_audio_core(op: u32, args: *const [usize; 32]) -> u64 {
    unsafe {
        let a = &*args;
        match op {
            0 => {
                native::init();
                0
            }
            1 => u64::from(native::start_progress_worker()),
            2 => {
                native::configure_sid_model(bytes(a[0], a[1]));
                0
            }
            3 => native::apply_configured_sid_model() as u32 as u64,
            4 => native::sid_model_name_z() as u64,
            5 => {
                (a[31] as *mut SidProtocolStatus).write(native::sid_protocol_status());
                0
            }
            6 => native::open_stream(a[0] as u32, a[1] as u16, a[2] as u16) as u32 as u64,
            7 => native::open_stream_for_owner(
                (a[0] as *const Owner).read(),
                a[1] as u32,
                a[2] as u16,
                a[3] as u16,
            ) as u32 as u64,
            8 => native::write_stream(a[0] as u32, a[1] as *const u8, a[2] as u32) as u32 as u64,
            9 => native::write_stream_for_owner(
                (a[0] as *const Owner).read(),
                a[1] as u32,
                a[2] as *const u8,
                a[3] as u32,
            ) as u32 as u64,
            10 => native::close_stream(a[0] as u32) as u32 as u64,
            11 => native::close_stream_for_owner((a[0] as *const Owner).read(), a[1] as u32) as u32
                as u64,
            12 => u64::from(native::close_streams_for_owner(
                (a[0] as *const Owner).read(),
            )),
            13 => u64::from(native::close_all_streams()),
            14 => native::set_volume(a[0] as u32, a[1] as u32) as u32 as u64,
            15 => native::set_volume_for_owner(
                (a[0] as *const Owner).read(),
                a[1] as u32,
                a[2] as u32,
            ) as u32 as u64,
            16 => native::sid_acquire() as u32 as u64,
            17 => native::sid_write_register(a[0] as u32, a[1] as u8, a[2] as u8) as u32 as u64,
            18 => native::sid_release(a[0] as u32) as u32 as u64,
            19 => native::sid_load_data(a[0] as u32, a[1] as u16, a[2] as *const u8, a[3] as u32)
                as u32 as u64,
            20 => native::sid_init(a[0] as u32, a[1] as u16, a[2] as u16) as u32 as u64,
            21 => native::sid_play_frame(a[0] as u32, a[1] as u16, a[2] as u16) as u32 as u64,
            22 => native::sid_stop(a[0] as u32) as u32 as u64,
            23 => native::midi_open_synth(a[0] as *const u8) as u32 as u64,
            24 => native::midi_send(a[0] as u32, a[1] as u8, a[2] as u8, a[3] as u8, a[4] as u8)
                as u32 as u64,
            25 => native::midi_close(a[0] as u32) as u32 as u64,
            26 => {
                (a[31] as *mut MidiProtocolStatus).write(native::midi_protocol_status());
                0
            }
            27 => {
                (a[31] as *mut Opl3ProtocolStatus).write(native::opl3_protocol_status());
                0
            }
            28 => native::opl3_reset() as u32 as u64,
            29 => native::opl3_write_register(a[0] as u8, a[1] as u8, a[2] as u8) as u32 as u64,
            30 => {
                native::opl3_run_register_demo();
                0
            }
            31 => native::opl3_render_block() as u32 as u64,
            32 => native::opl3_stop() as u32 as u64,
            33 => native::alloc_dma_buffer(a[0] as u32, a[1] as u32),
            34 => {
                native::free_dma_buffer(a[0] as u64, a[1] as u32);
                0
            }
            35 => native::register_audio_backend_z(
                a[0] as *const u8,
                a[1] as *const core::ffi::c_void,
            ) as u32 as u64,
            36 => native::register_mixer_backend_z(
                a[0] as *const u8,
                a[1] as *const core::ffi::c_void,
            ) as u32 as u64,
            37 => {
                native::register_synth_engine_z(a[0] as *const u8, a[1] as *const core::ffi::c_void)
                    as u32 as u64
            }
            38 => {
                native::register_audio_backend(bytes(a[0], a[1]), a[2] as *const core::ffi::c_void);
                0
            }
            39 => native::register_external_audio_backend_z(
                a[0] as *const u8,
                (a[1] as *const BackendRegistration).read(),
            ) as u32 as u64,
            40 => u64::from(native::register_external_audio_outputs(
                bytes(a[0], a[1]),
                (a[2] as *const a::AudioOutputExtension).read(),
                if a[3] == 0 {
                    None
                } else {
                    Some(core::mem::transmute::<usize, StopPcmCtxFn>(a[3]))
                },
            )),
            41 => native::audio_output_info(a[0] as u32, a[1] as *mut a::AudioOutputInfo) as u32
                as u64,
            42 => native::audio_select_output(bytes(a[0], a[1])) as u32 as u64,
            43 => native::register_external_synth_engine_z(
                a[0] as *const u8,
                (a[1] as *const SynthRegistration).read(),
            ) as u32 as u64,
            44 => native::unregister_audio_backend_by_name(bytes(a[0], a[1])) as u32 as u64,
            45 => native::unregister_audio_backend_z(a[0] as *const u8) as u32 as u64,
            46 => u64::from(native::register_synth_engine(
                bytes(a[0], a[1]),
                a[2] as *const core::ffi::c_void,
            )),
            47 => {
                native::register_native_synth_engine(bytes(a[0], a[1]), a[2], a[3], a[4]);
                0
            }
            48 => {
                native::register_native_audio_backend(bytes(a[0], a[1]), a[2], a[3]);
                0
            }
            49 => u64::from(native::select_audio_backend(bytes(a[0], a[1]))),
            50 => u64::from(native::unregister_audio_backend(bytes(a[0], a[1]))),
            51 => u64::from(native::set_audio_backend_closing(
                bytes(a[0], a[1]),
                a[2] != 0,
            )),
            52 => u64::from(native::audio_backend_registered(bytes(a[0], a[1]))),
            53 => {
                (a[31] as *mut BackendFamilyStatus)
                    .write(native::audio_backend_family_status(bytes(a[0], a[1])));
                0
            }
            54 => u64::from(native::audio_backend_active(bytes(a[0], a[1]))),
            55 => u64::from(native::audio_backend_has_output(bytes(a[0], a[1]))),
            56 => {
                (a[31] as *mut PerformanceSummary).write(native::performance_summary());
                0
            }
            57 => {
                native::dump_status();
                0
            }
            _ => panic!("Invalid private audio operation"),
        }
    }
}
