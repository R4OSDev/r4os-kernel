//! Fixed private audio projections; no Zig slice, optional record or enum ABI.
use super::{backend_contract::Limits, mixer::Owner, pcm::ResamplerState};
use crate::contract as a;
use core::{ffi::c_void, ptr, slice};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Span {
    pub pointer: *const u8,
    pub len: usize,
}
impl Span {
    pub const EMPTY: Self = Self {
        pointer: ptr::null(),
        len: 0,
    };
    pub const fn literal(s: &'static [u8]) -> Self {
        Self {
            pointer: s.as_ptr(),
            len: s.len(),
        }
    }
    pub(crate) unsafe fn bytes<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(self.pointer, self.len) }
        }
    }
}
impl Default for Span {
    fn default() -> Self {
        Self::EMPTY
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Ring {
    pub pointer: *mut u8,
    pub len: usize,
}
impl Ring {
    pub const EMPTY: Self = Self {
        pointer: ptr::null_mut(),
        len: 0,
    };
    pub(crate) unsafe fn bytes_mut<'a>(self) -> &'a mut [u8] {
        if self.len == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(self.pointer, self.len) }
        }
    }
}
pub type BackendStatus = a::AudioBackendStatus;
pub type SynthStatus = a::SynthEngineStatus;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MidiProtocolStatus {
    pub source: Span,
    pub classify: u64,
    pub missing_required: u64,
    pub dispatch_fail: u64,
    pub last_result: i32,
    pub last_event: u8,
}
impl Default for MidiProtocolStatus {
    fn default() -> Self {
        Self {
            source: Span::literal(b"none"),
            classify: 0,
            missing_required: 0,
            dispatch_fail: 0,
            last_result: 0,
            last_event: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Opl3ProtocolStatus {
    pub source: Span,
    pub registers: u64,
    pub midi: u64,
    pub missing_required: u64,
    pub dispatch_fail: u64,
    pub last_result: i32,
    pub last_kind: u8,
    pub last_action: u8,
}
impl Default for Opl3ProtocolStatus {
    fn default() -> Self {
        Self {
            source: Span::literal(b"none"),
            registers: 0,
            midi: 0,
            missing_required: 0,
            dispatch_fail: 0,
            last_result: 0,
            last_kind: 0,
            last_action: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SidProtocolStatus {
    pub source: Span,
    pub model: u64,
    pub register: u64,
    pub io: u64,
    pub missing_required: u64,
    pub dispatch_fail: u64,
    pub last_result: i32,
    pub last_kind: u8,
    pub last_voice: u8,
}
impl Default for SidProtocolStatus {
    fn default() -> Self {
        Self {
            source: Span::literal(b"none"),
            model: 0,
            register: 0,
            io: 0,
            missing_required: 0,
            dispatch_fail: 0,
            last_result: 0,
            last_kind: 0,
            last_voice: 0,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PerformanceSummary {
    pub max_streams: u32,
    pub open_streams: u32,
    pub closed_streams: u32,
    pub max_backends: u32,
    pub registered_backends: u32,
    pub active_backends: u32,
    pub max_synths: u32,
    pub registered_synths: u32,
    pub total_stream_writes: u64,
    pub backend_ok: u64,
    pub backend_fail: u64,
    pub backend_status_writes: u64,
    pub backend_status_underruns: u64,
    pub backend_status_errors: u64,
    pub stream_ring_bytes: u64,
    pub stream_available_bytes: u64,
    pub stream_available_high_water_bytes: u64,
    pub stream_write_truncations: u64,
    pub stream_dropped_bytes: u64,
    pub backend_write_calls: u64,
    pub backend_status_refills: u64,
    pub backend_status_silence_refills: u64,
    pub backend_status_buffer_bytes: u64,
    pub backend_status_queued_buffers: u64,
    pub backend_status_last_buffer_bytes: u64,
    pub sid_writes: u64,
    pub sid_pcm_writes: u64,
    pub sid_pcm_fail: u64,
    pub midi_sends: u64,
    pub midi_renders: u64,
}
impl Default for PerformanceSummary {
    fn default() -> Self {
        Self {
            max_streams: 8,
            open_streams: 0,
            closed_streams: 0,
            max_backends: 8,
            registered_backends: 0,
            active_backends: 0,
            max_synths: 8,
            registered_synths: 0,
            total_stream_writes: 0,
            backend_ok: 0,
            backend_fail: 0,
            backend_status_writes: 0,
            backend_status_underruns: 0,
            backend_status_errors: 0,
            stream_ring_bytes: 0,
            stream_available_bytes: 0,
            stream_available_high_water_bytes: 0,
            stream_write_truncations: 0,
            stream_dropped_bytes: 0,
            backend_write_calls: 0,
            backend_status_refills: 0,
            backend_status_silence_refills: 0,
            backend_status_buffer_bytes: 0,
            backend_status_queued_buffers: 0,
            backend_status_last_buffer_bytes: 0,
            sid_writes: 0,
            sid_pcm_writes: 0,
            sid_pcm_fail: 0,
            midi_sends: 0,
            midi_renders: 0,
        }
    }
}
pub type WritePcmCtxFn = unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u16, u16) -> i32;
pub type StopPcmCtxFn = unsafe extern "C" fn(*mut c_void) -> i32;
pub type StatusCtxFn = unsafe extern "C" fn(*mut c_void, *mut BackendStatus) -> i32;
pub type TrackedWriteCtxFn = unsafe extern "C" fn(
    *mut c_void,
    *const u8,
    u32,
    u32,
    u16,
    u16,
    *mut a::AudioBackendWriteReceipt,
) -> i32;
pub type PlaybackCtxFn = unsafe extern "C" fn(*mut c_void, *mut a::AudioBackendPlayback) -> i32;
pub type FlowCtxFn = unsafe extern "C" fn(*mut c_void, *const a::AudioBackendFlow) -> i32;
pub type SynthMidiSendCtxFn = unsafe extern "C" fn(*mut c_void, u8, u8, u8, u8) -> i32;
pub type SynthRenderCtxFn = unsafe extern "C" fn(*mut c_void) -> i32;
pub type SynthRenderPcmCtxFn =
    unsafe extern "C" fn(*mut c_void, *mut u8, u32, u32, u16, u16) -> i32;
pub type SynthStopCtxFn = unsafe extern "C" fn(*mut c_void) -> i32;
pub type SynthOpl3ResetCtxFn = unsafe extern "C" fn(*mut c_void) -> i32;
pub type SynthOpl3WriteRegisterCtxFn = unsafe extern "C" fn(*mut c_void, u8, u8, u8) -> i32;
pub type SynthStatusCtxFn = unsafe extern "C" fn(*mut c_void, *mut SynthStatus) -> i32;
pub type SynthSidAcquireCtxFn = unsafe extern "C" fn(*mut c_void) -> i32;
pub type SynthSidReleaseCtxFn = unsafe extern "C" fn(*mut c_void, u32) -> i32;
pub type SynthSidSetModelCtxFn = unsafe extern "C" fn(*mut c_void, u32) -> i32;
pub type SynthSidWriteRegisterCtxFn = unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32;
pub type SynthSidLoadDataCtxFn = unsafe extern "C" fn(*mut c_void, u32, u32, *const u8, u32) -> i32;
pub type SynthSidInitCtxFn = unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32;
pub type SynthSidPlayFrameCtxFn = unsafe extern "C" fn(*mut c_void, u32, u32, u32) -> i32;
pub type SynthSidRenderPcmCtxFn = unsafe extern "C" fn(*mut c_void, u32, *mut u8, u32) -> i32;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BackendRegistration {
    pub context: *mut c_void,
    pub write: Option<WritePcmCtxFn>,
    pub stop: Option<StopPcmCtxFn>,
    pub status: Option<StatusCtxFn>,
    pub limits_present: bool,
    pub limits: Limits,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SynthRegistration {
    pub flags: u32,
    pub context: *mut c_void,
    pub send: Option<SynthMidiSendCtxFn>,
    pub render: Option<SynthRenderCtxFn>,
    pub render_pcm: Option<SynthRenderPcmCtxFn>,
    pub stop: Option<SynthStopCtxFn>,
    pub status: Option<SynthStatusCtxFn>,
    pub opl3_reset: Option<SynthOpl3ResetCtxFn>,
    pub opl3_write: Option<SynthOpl3WriteRegisterCtxFn>,
    pub sid_acquire: Option<SynthSidAcquireCtxFn>,
    pub sid_release: Option<SynthSidReleaseCtxFn>,
    pub sid_model: Option<SynthSidSetModelCtxFn>,
    pub sid_write: Option<SynthSidWriteRegisterCtxFn>,
    pub sid_load: Option<SynthSidLoadDataCtxFn>,
    pub sid_init: Option<SynthSidInitCtxFn>,
    pub sid_play: Option<SynthSidPlayFrameCtxFn>,
    pub sid_pcm: Option<SynthSidRenderPcmCtxFn>,
    pub external: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BackendFamilyStatus {
    pub registered: bool,
    pub active: bool,
    pub has_output: bool,
}
#[derive(Clone, Copy, PartialEq)]
pub enum StreamState {
    Empty,
    Open,
    Closed,
}
pub struct Stream {
    pub state: StreamState,
    pub id: u32,
    pub owner: Owner,
    pub rate: u32,
    pub channels: u16,
    pub format: u16,
    pub volume: u32,
    pub ring: Ring,
    pub read_pos: usize,
    pub write_pos: usize,
    pub available: usize,
    pub resampler: ResamplerState,
    pub total_written: u64,
    pub transaction: bool,
    pub read_reserved: bool,
    pub progress: super::playback::Source<64>,
    pub low_water_frames: u32,
    pub high_water_frames: u32,
    pub progress_eligible: bool,
}
impl Stream {
    pub const EMPTY: Self = Self {
        state: StreamState::Empty,
        id: 0,
        owner: Owner::KERNEL,
        rate: 0,
        channels: 0,
        format: 0,
        volume: 65_536,
        ring: Ring::EMPTY,
        read_pos: 0,
        write_pos: 0,
        available: 0,
        resampler: ResamplerState {
            rate: 0,
            channels: 0,
            format: 0,
            phase_q16: 0,
            phase_remainder: 0,
            previous: super::pcm::StereoFrame { left: 0, right: 0 },
            previous_valid: false,
            chunk_done: false,
        },
        total_written: 0,
        transaction: false,
        read_reserved: false,
        progress: super::playback::Source::EMPTY,
        low_water_frames: 960,
        high_water_frames: 1920,
        progress_eligible: true,
    };
}
#[derive(Clone, Copy)]
pub struct NamedBackend {
    pub active: bool,
    pub name: [u8; 32],
    pub name_len: usize,
    pub pointer: *const c_void,
}
impl NamedBackend {
    pub const EMPTY: Self = Self {
        active: false,
        name: [0; 32],
        name_len: 0,
        pointer: ptr::null(),
    };
}
#[derive(Clone, Copy)]
pub struct OutputEpochBinding {
    pub callback: a::AudioOutputEpochFn,
    pub query:
        unsafe extern "C" fn(*mut c_void, a::AudioOutputEpochFn, *mut a::AudioOutputEpoch) -> i32,
}
pub struct AudioBackend {
    pub registered: bool,
    pub active: bool,
    pub closing: bool,
    pub name: [u8; 32],
    pub name_len: usize,
    pub config: BackendRegistration,
    pub idle: Option<StopPcmCtxFn>,
    pub outputs: Option<a::AudioOutputExtension>,
    pub output_epoch: Option<OutputEpochBinding>,
    pub generation: u64,
    pub tracked_write: Option<TrackedWriteCtxFn>,
    pub playback: Option<PlaybackCtxFn>,
    pub flow: Option<FlowCtxFn>,
}
impl AudioBackend {
    pub const EMPTY: Self = Self {
        registered: false,
        active: false,
        closing: false,
        name: [0; 32],
        name_len: 0,
        config: BackendRegistration {
            context: ptr::null_mut(),
            write: None,
            stop: None,
            status: None,
            limits_present: false,
            limits: Limits {
                formats: 0,
                min_rate: 0,
                max_rate: 0,
                max_channels: 0,
            },
        },
        idle: None,
        outputs: None,
        output_epoch: None,
        generation: 0,
        tracked_write: None,
        playback: None,
        flow: None,
    };
}
pub struct SynthEngine {
    pub registered: bool,
    pub closing: bool,
    pub driver_owner: u32,
    pub generation: u64,
    pub name: [u8; 32],
    pub name_len: usize,
    pub config: SynthRegistration,
    pub pcm_pending: [u8; 4096],
    pub pcm_pending_len: u32,
    pub sends: u64,
    pub renders: u64,
    pub stops: u64,
}
impl SynthEngine {
    pub const EMPTY: Self = Self {
        registered: false,
        closing: false,
        driver_owner: 0,
        generation: 0,
        name: [0; 32],
        name_len: 0,
        config: SynthRegistration {
            flags: 0,
            context: ptr::null_mut(),
            send: None,
            render: None,
            render_pcm: None,
            stop: None,
            status: None,
            opl3_reset: None,
            opl3_write: None,
            sid_acquire: None,
            sid_release: None,
            sid_model: None,
            sid_write: None,
            sid_load: None,
            sid_init: None,
            sid_play: None,
            sid_pcm: None,
            external: false,
        },
        pcm_pending: [0; 4096],
        pcm_pending_len: 0,
        sends: 0,
        renders: 0,
        stops: 0,
    };
}
const _: () = {
    assert!(size_of::<BackendRegistration>() == 56);
    assert!(size_of::<SynthRegistration>() == 144);
};

const _: () = {
    assert!(size_of::<MidiProtocolStatus>() == 48);
    assert!(size_of::<Opl3ProtocolStatus>() == 56);
    assert!(size_of::<SidProtocolStatus>() == 64);
};
