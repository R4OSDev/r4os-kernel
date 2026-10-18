//! Sole PCM/stream/backend/synth owner; original callback and wait edges.
use super::{
    backend_contract::{self, EndMode},
    mixer::{self, Owner},
    pcm,
    types::*,
};
use crate::{
    boot::log as bootlog,
    contract as a,
    fs::vfs_types::Bytes,
    loader::r4p,
    memory::heap,
    platform::timer,
    sched::{
        scheduler, sync_locks, sync_objects,
        sync_types::{Event, Mutex, Semaphore, Text, WaitQueue},
        task::task_api,
        task_context,
    },
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    ptr, slice,
    sync::atomic::{AtomicU8, AtomicU64, AtomicUsize, Ordering},
};
const BUSY: i32 = a::service_api_result_busy;
const INVALID: i32 = a::service_api_result_invalid;
const NO_ENDPOINT: i32 = a::service_api_result_no_endpoint;
pub const FORMAT_S16LE: u16 = 1;
pub const FORMAT_U8: u16 = 2;
pub const DEFAULT_RATE: u32 = 48000;
pub const DEFAULT_CHANNELS: u16 = 2;
const RING_BYTES: usize = 16 * 1024;
const MAX_STREAMS: usize = mixer::MAX_SOURCES;
const MAX_NAME: usize = 32;
const MAX_AUDIO_BACKENDS: usize = 8;
const MAX_SYNTH_ENGINES: usize = 8;
const SID_RENDER_BYTES: usize = 3840;
const SYNTH_RENDER_MAX_FRAMES: u32 = 1024;
const MIX_QUANTUM_BYTES: usize = 480 * pcm::TARGET_FRAME_BYTES;
struct Resident<T>(UnsafeCell<T>);
// SAFETY: the original stream mutex/output permit and serialized synth/boot
// callers own each field. No borrowed State spans admission, callback or wait.
unsafe impl<T> Sync for Resident<T> {}
struct State {
    progress_worker_started: bool,
    streams: [Stream; MAX_STREAMS],
    mix_scratch: [u8; MIX_QUANTUM_BYTES],
    next_stream_id: u32,
    audio_backends: [AudioBackend; MAX_AUDIO_BACKENDS],
    active_audio_slot: Option<usize>,
    initial_output_pending: bool,
    mixer_backend: NamedBackend,
    synth_engines: [SynthEngine; MAX_SYNTH_ENGINES],
    active_midi_synth_slot: Option<usize>,
    total_stream_writes: u64,
    total_backend_ok: u64,
    total_backend_fail: u64,
    stream_available_high_water: u64,
    stream_write_truncations: u64,
    stream_dropped_bytes: u64,
    stream_write_total_ticks: u64,
    stream_write_max_ticks: u64,
    stream_write_last_ticks: u64,
    backend_write_calls: u64,
    backend_write_total_ticks: u64,
    backend_write_max_ticks: u64,
    backend_write_last_ticks: u64,
    sid_acquired: bool,
    sid_write_count: u64,
    sid_last_register: u8,
    sid_last_value: u8,
    midi_acquired: bool,
    midi_send_count: u64,
    midi_last_channel: u8,
    midi_last_status: u8,
    midi_last_data1: u8,
    midi_last_data2: u8,
    midi_render_ticks: u64,
    midi_r4p_classify: u64,
    midi_missing_required: u64,
    midi_dispatch_failures: u64,
    midi_last_result: i32,
    midi_last_event: u8,
    opl3_r4p_register: u64,
    opl3_r4p_midi: u64,
    opl3_missing_required: u64,
    opl3_dispatch_failures: u64,
    opl3_last_result: i32,
    opl3_last_write_kind: u8,
    opl3_last_action: u8,
    sid_model_option: Span,
    sid_last_result: i32,
    sid_pcm_writes: u64,
    sid_pcm_ok: u64,
    sid_pcm_fail: u64,
    sid_pcm_pending: [u8; SID_RENDER_BYTES],
    sid_pcm_pending_len: u32,
    sid_r4p_model: u64,
    sid_r4p_register: u64,
    sid_r4p_io: u64,
    sid_missing_required: u64,
    sid_dispatch_failures: u64,
    sid_last_kind: u8,
    sid_last_voice: u8,
}
static STATE: Resident<State> = Resident(UnsafeCell::new(State {
    progress_worker_started: false,
    streams: [const { Stream::EMPTY }; MAX_STREAMS],
    mix_scratch: [0; MIX_QUANTUM_BYTES],
    next_stream_id: 1,
    audio_backends: [const { AudioBackend::EMPTY }; MAX_AUDIO_BACKENDS],
    active_audio_slot: None,
    initial_output_pending: false,
    mixer_backend: NamedBackend::EMPTY,
    synth_engines: [const { SynthEngine::EMPTY }; MAX_SYNTH_ENGINES],
    active_midi_synth_slot: None,
    total_stream_writes: 0,
    total_backend_ok: 0,
    total_backend_fail: 0,
    stream_available_high_water: 0,
    stream_write_truncations: 0,
    stream_dropped_bytes: 0,
    stream_write_total_ticks: 0,
    stream_write_max_ticks: 0,
    stream_write_last_ticks: 0,
    backend_write_calls: 0,
    backend_write_total_ticks: 0,
    backend_write_max_ticks: 0,
    backend_write_last_ticks: 0,
    sid_acquired: false,
    sid_write_count: 0,
    sid_last_register: 0,
    sid_last_value: 0,
    midi_acquired: false,
    midi_send_count: 0,
    midi_last_channel: 0,
    midi_last_status: 0,
    midi_last_data1: 0,
    midi_last_data2: 0,
    midi_render_ticks: 0,
    midi_r4p_classify: 0,
    midi_missing_required: 0,
    midi_dispatch_failures: 0,
    midi_last_result: 0,
    midi_last_event: 0,
    opl3_r4p_register: 0,
    opl3_r4p_midi: 0,
    opl3_missing_required: 0,
    opl3_dispatch_failures: 0,
    opl3_last_result: 0,
    opl3_last_write_kind: 0,
    opl3_last_action: 0,
    sid_model_option: Span::literal(b"8580"),
    sid_last_result: 0,
    sid_pcm_writes: 0,
    sid_pcm_ok: 0,
    sid_pcm_fail: 0,
    sid_pcm_pending: [0; SID_RENDER_BYTES],
    sid_pcm_pending_len: 0,
    sid_r4p_model: 0,
    sid_r4p_register: 0,
    sid_r4p_io: 0,
    sid_missing_required: 0,
    sid_dispatch_failures: 0,
    sid_last_kind: 0,
    sid_last_voice: 0,
}));
unsafe fn state() -> *mut State {
    STATE.0.get()
}
macro_rules! g {
    ($f:ident) => {
        (*state()).$f
    };
}
unsafe fn stream(i: usize) -> *mut Stream {
    unsafe { ptr::addr_of_mut!(g!(streams)).cast::<Stream>().add(i) }
}
unsafe fn backend(i: usize) -> *mut AudioBackend {
    unsafe {
        ptr::addr_of_mut!(g!(audio_backends))
            .cast::<AudioBackend>()
            .add(i)
    }
}
unsafe fn synth(i: usize) -> *mut SynthEngine {
    unsafe {
        ptr::addr_of_mut!(g!(synth_engines))
            .cast::<SynthEngine>()
            .add(i)
    }
}
const fn mutex() -> Mutex {
    Mutex {
        rank: 280,
        mode: 0,
        name: Text::literal(b"audio-streams"),
        ..Mutex::EMPTY
    }
}
static STREAM_LOCK: Resident<Mutex> = Resident(UnsafeCell::new(mutex()));
static OUTPUT_PERMIT: Resident<Semaphore> = Resident(UnsafeCell::new(Semaphore {
    count: 1,
    max_count: 1,
    queue: WaitQueue::EMPTY,
}));
static OUTPUT_OWNER: AtomicUsize = AtomicUsize::new(0);
static OUTPUT_PRESENT: AtomicU8 = AtomicU8::new(0);
static PROGRESS: Resident<Event> = Resident(UnsafeCell::new(Event::AUTO));
static MIX_DUE: AtomicU64 = AtomicU64::new(u64::MAX);
static BACKEND_DUE: AtomicU64 = AtomicU64::new(u64::MAX);

pub(super) fn log(s: &[u8]) {
    unsafe { audio_log(s.as_ptr(), s.len()) }
}

fn boot(s: &[u8]) {
    unsafe { bootlog::r4os_rust_bootlog_write(s.as_ptr(), s.len()) }
}
fn boot_dec(v: u64) {
    bootlog::r4os_rust_bootlog_dec(v)
}
unsafe fn lock() -> bool {
    unsafe { sync_locks::mutex_lock(STREAM_LOCK.0.get(), u64::MAX) }
}
unsafe fn unlock() {
    unsafe {
        sync_locks::mutex_unlock(STREAM_LOCK.0.get());
    }
}
unsafe fn relock() {
    while !unsafe { lock() } {
        scheduler::yield_task();
    }
}
struct StreamLock;
impl StreamLock {
    unsafe fn acquire() -> Option<Self> {
        if unsafe { lock() } { Some(Self) } else { None }
    }
}
impl Drop for StreamLock {
    fn drop(&mut self) {
        unsafe { unlock() }
    }
}
struct OutputGuard(task_context::Unwind);
impl OutputGuard {
    unsafe fn acquire(wait: bool) -> Option<Self> {
        unsafe {
            let current = scheduler::current();
            let identity = if current.is_null() {
                1
            } else {
                current as usize
            };
            if OUTPUT_OWNER.load(Ordering::Acquire) == identity {
                return None;
            }
            let unwind = task_context::enter();
            if !unwind.admitted() {
                return None;
            }
            if sync_objects::r4os_rust_sync_semaphore(
                OUTPUT_PERMIT.0.get(),
                1,
                if wait { u64::MAX } else { 0 },
                ptr::null(),
                ptr::null_mut(),
            ) != 1
            {
                task_context::leave(unwind);
                return None;
            }
            OUTPUT_OWNER.store(identity, Ordering::Release);
            Some(Self(unwind))
        }
    }
}
impl Drop for OutputGuard {
    fn drop(&mut self) {
        unsafe {
            OUTPUT_OWNER.store(0, Ordering::Release);
            sync_objects::r4os_rust_sync_semaphore(
                OUTPUT_PERMIT.0.get(),
                4,
                1,
                ptr::null(),
                ptr::null_mut(),
            );
            task_context::leave(self.0);
        }
    }
}
struct Transaction {
    stream: *mut Stream,
    token: task_context::Unwind,
}
impl Drop for Transaction {
    fn drop(&mut self) {
        unsafe {
            (*self.stream).transaction = false;
            task_context::leave(self.token);
        }
    }
}
struct Transactions {
    reserved: [bool; MAX_STREAMS],
    token: task_context::Unwind,
}
impl Drop for Transactions {
    fn drop(&mut self) {
        unsafe {
            for i in 0..MAX_STREAMS {
                if self.reserved[i] {
                    (*stream(i)).transaction = false;
                }
            }
            task_context::leave(self.token);
        }
    }
}
struct WriteTiming(u64);
impl Drop for WriteTiming {
    fn drop(&mut self) {
        unsafe {
            record_tick_stat(
                ptr::addr_of_mut!(g!(stream_write_total_ticks)),
                ptr::addr_of_mut!(g!(stream_write_max_ticks)),
                ptr::addr_of_mut!(g!(stream_write_last_ticks)),
                self.0,
            )
        }
    }
}
// Preserve the original functional clock reversal check and timing formula.
#[allow(clippy::implicit_saturating_sub)]
unsafe fn record_tick_stat(total: *mut u64, max: *mut u64, last: *mut u64, start: u64) {
    unsafe {
        let now = timer::tick_count();
        let elapsed = if now >= start { now - start } else { 0 };
        *total = (*total).wrapping_add(elapsed);
        *last = elapsed;
        if elapsed > *max {
            *max = elapsed;
        }
    }
}
pub(crate) unsafe fn init() {
    unsafe {
        MIX_DUE.store(u64::MAX, Ordering::Release);
        BACKEND_DUE.store(u64::MAX, Ordering::Release);
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            let ring = (*p).ring;
            p.write(Stream::EMPTY);
            if ring.len == RING_BYTES {
                (*p).ring = ring;
            }
        }
        STREAM_LOCK.0.get().write(mutex());
        OUTPUT_PERMIT.0.get().write(Semaphore {
            count: 1,
            max_count: 1,
            queue: WaitQueue::EMPTY,
        });
        OUTPUT_OWNER.store(0, Ordering::Relaxed);
        OUTPUT_PRESENT.store(0, Ordering::Relaxed);
        g!(next_stream_id) = 1;
        for i in 0..8 {
            backend(i).write(AudioBackend::EMPTY);
        }
        g!(active_audio_slot) = None;
        g!(initial_output_pending) = false;
        g!(mixer_backend) = NamedBackend::EMPTY;
        for i in 0..8 {
            synth(i).write(SynthEngine::EMPTY);
        }
        g!(active_midi_synth_slot) = None;
        g!(total_stream_writes) = 0;
        g!(total_backend_ok) = 0;
        g!(total_backend_fail) = 0;
        g!(stream_available_high_water) = 0;
        g!(stream_write_truncations) = 0;
        g!(stream_dropped_bytes) = 0;
        g!(stream_write_total_ticks) = 0;
        g!(stream_write_max_ticks) = 0;
        g!(stream_write_last_ticks) = 0;
        g!(backend_write_calls) = 0;
        g!(backend_write_total_ticks) = 0;
        g!(backend_write_max_ticks) = 0;
        g!(backend_write_last_ticks) = 0;
        g!(sid_acquired) = false;
        g!(sid_write_count) = 0;
        g!(sid_last_register) = 0;
        g!(sid_last_value) = 0;
        g!(midi_acquired) = false;
        g!(midi_send_count) = 0;
        g!(midi_last_channel) = 0;
        g!(midi_last_status) = 0;
        g!(midi_last_data1) = 0;
        g!(midi_last_data2) = 0;
        g!(midi_render_ticks) = 0;
        g!(midi_r4p_classify) = 0;
        g!(midi_missing_required) = 0;
        g!(midi_dispatch_failures) = 0;
        g!(midi_last_result) = 0;
        g!(midi_last_event) = 0;
        g!(opl3_r4p_register) = 0;
        g!(opl3_r4p_midi) = 0;
        g!(opl3_missing_required) = 0;
        g!(opl3_dispatch_failures) = 0;
        g!(opl3_last_result) = 0;
        g!(opl3_last_write_kind) = 0;
        g!(opl3_last_action) = 0;
        g!(sid_model_option) = Span::literal(b"8580");
        g!(sid_last_result) = 0;
        g!(sid_pcm_writes) = 0;
        g!(sid_pcm_ok) = 0;
        g!(sid_pcm_fail) = 0;
        g!(sid_pcm_pending_len) = 0;
        g!(sid_r4p_model) = 0;
        g!(sid_r4p_register) = 0;
        g!(sid_r4p_io) = 0;
        g!(sid_missing_required) = 0;
        g!(sid_dispatch_failures) = 0;
        g!(sid_last_kind) = 0;
        g!(sid_last_voice) = 0;
        register_mixer_backend(b"SimpleKernelMixer", ptr::null());
        boot(b"[AUDIO] core ready pcm u8/s16le mono/stereo\r\n");
    }
}

fn role(name: &[u8]) -> Bytes {
    Bytes {
        data: name.as_ptr(),
        len: name.len(),
    }
}

unsafe fn has_protocol(name: &[u8]) -> bool {
    unsafe { r4p::r4os_rust_r4p_has_active(role(name)) }
}

unsafe fn copy_z(data: *const u8, out: &mut [u8]) -> Option<&[u8]> {
    unsafe {
        let mut len = 0;
        while len < out.len() && *data.add(len) != 0 {
            out[len] = *data.add(len);
            len += 1;
        }
        if len == out.len() {
            None
        } else {
            Some(&out[..len])
        }
    }
}
fn name_eq(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}
pub(crate) unsafe fn performance_summary() -> PerformanceSummary {
    unsafe {
        if !lock() {
            return PerformanceSummary::default();
        }
        let mut out = PerformanceSummary {
            total_stream_writes: g!(total_stream_writes),
            backend_ok: g!(total_backend_ok),
            backend_fail: g!(total_backend_fail),
            stream_available_high_water_bytes: g!(stream_available_high_water),
            stream_write_truncations: g!(stream_write_truncations),
            stream_dropped_bytes: g!(stream_dropped_bytes),
            stream_write_total_ticks: g!(stream_write_total_ticks),
            stream_write_max_ticks: g!(stream_write_max_ticks),
            stream_write_last_ticks: g!(stream_write_last_ticks),
            backend_write_calls: g!(backend_write_calls),
            backend_write_total_ticks: g!(backend_write_total_ticks),
            backend_write_max_ticks: g!(backend_write_max_ticks),
            backend_write_last_ticks: g!(backend_write_last_ticks),
            sid_writes: g!(sid_write_count),
            sid_pcm_writes: g!(sid_pcm_writes),
            sid_pcm_fail: g!(sid_pcm_fail),
            midi_sends: g!(midi_send_count),
            midi_renders: g!(midi_render_ticks),
            ..Default::default()
        };
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            out.stream_ring_bytes = out.stream_ring_bytes.wrapping_add((*p).ring.len as u64);
            match (*p).state {
                StreamState::Open => {
                    out.open_streams += 1;
                    out.stream_available_bytes = out
                        .stream_available_bytes
                        .wrapping_add((*p).available as u64);
                }
                StreamState::Closed => out.closed_streams += 1,
                StreamState::Empty => {}
            }
        }
        unlock();
        let Some(_output) = OutputGuard::acquire(false) else {
            return out;
        };
        for i in 0..MAX_AUDIO_BACKENDS {
            let p = backend(i);
            if !(*p).registered {
                continue;
            }
            out.registered_backends += 1;
            if (*p).active && !(*p).closing {
                out.active_backends += 1;
            }
            if (*p).closing {
                continue;
            }
            let c = (*p).config;
            if let Some(status_fn) = c.status {
                let mut status = BackendStatus {
                    active: u32::from((*p).active),
                    ..Default::default()
                };
                if status_fn(c.context, &mut status) == 0 {
                    out.backend_status_writes =
                        out.backend_status_writes.wrapping_add(status.writes);
                    out.backend_status_underruns =
                        out.backend_status_underruns.wrapping_add(status.underruns);
                    out.backend_status_errors =
                        out.backend_status_errors.wrapping_add(status.errors);
                    out.backend_status_refills =
                        out.backend_status_refills.wrapping_add(status.refills);
                    out.backend_status_silence_refills = out
                        .backend_status_silence_refills
                        .wrapping_add(status.silence_refills);
                    out.backend_status_buffer_bytes = out
                        .backend_status_buffer_bytes
                        .wrapping_add(status.buffer_bytes);
                    out.backend_status_queued_buffers = out
                        .backend_status_queued_buffers
                        .wrapping_add(status.queued_buffers);
                    out.backend_status_last_buffer_bytes = out
                        .backend_status_last_buffer_bytes
                        .wrapping_add(status.last_buffer_bytes);
                    if status.total_write_ticks > out.backend_write_total_ticks {
                        out.backend_write_total_ticks = status.total_write_ticks;
                    }
                    if status.max_write_ticks > out.backend_write_max_ticks {
                        out.backend_write_max_ticks = status.max_write_ticks;
                    }
                    if status.last_write_ticks > out.backend_write_last_ticks {
                        out.backend_write_last_ticks = status.last_write_ticks;
                    }
                    out.backend_status_refill_total_ticks = out
                        .backend_status_refill_total_ticks
                        .wrapping_add(status.total_refill_ticks);
                    if status.max_refill_ticks > out.backend_status_refill_max_ticks {
                        out.backend_status_refill_max_ticks = status.max_refill_ticks;
                    }
                    if status.last_refill_ticks > out.backend_status_refill_last_ticks {
                        out.backend_status_refill_last_ticks = status.last_refill_ticks;
                    }
                }
            }
        }
        for i in 0..MAX_SYNTH_ENGINES {
            if (*synth(i)).registered {
                out.registered_synths += 1;
            }
        }
        out
    }
}

unsafe fn audio_log(text: *const u8, len: usize) {
    unsafe { crate::boot::log_route::r4os_rust_log_write(0, text, len) }
}

// Private operation modules share the sole state and original typed owners.
mod outputs;
mod progress;
mod streams;
mod synths;
use outputs::{
    active_output_present, idle_active_pcm_owned, register_mixer_backend, stop_active_pcm,
    write_active_pcm, write_active_pcm_owned,
};
pub(crate) use outputs::{
    audio_backend_active, audio_backend_family_status, audio_backend_has_output,
    audio_backend_registered, audio_output_info, audio_select_output,
    register_external_audio_backend_z, register_external_audio_outputs, register_mixer_backend_z,
    set_audio_backend_closing, unregister_audio_backend_by_name, unregister_audio_backend_z,
};
use progress::schedule_progress;
pub(crate) use progress::start_progress_worker;
pub(crate) use streams::{
    close_stream, close_stream_for_owner, close_streams_for_owner, open_stream,
    open_stream_for_owner, set_volume, set_volume_for_owner, write_stream, write_stream_for_owner,
};
use streams::{pump_available_locked, stop_pcm_if_no_streams};
pub(crate) use synths::{
    apply_configured_sid_model, configure_sid_model, midi_close, midi_open_synth, midi_send,
    opl3_render_block, opl3_reset, opl3_stop, opl3_write_register,
    register_external_synth_engine_z, sid_acquire, sid_init, sid_load_data, sid_model_name_z,
    sid_play_frame, sid_release, sid_stop, sid_write_register,
};
