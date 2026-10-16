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
    memory::{heap, phys},
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
const MAX_STREAMS: usize = 8;
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
unsafe extern "C" {
    fn r4os_zig_audio_log(text: *const u8, len: usize);
    fn r4os_zig_audio_legacy(
        op: u32,
        code: usize,
        data: *const u8,
        len: usize,
        rate: u32,
        channels: u16,
        format: u16,
        a: u8,
        b: u8,
        c: u8,
        d: u8,
    ) -> i32;
}
pub(super) fn log(s: &[u8]) {
    unsafe { r4os_zig_audio_log(s.as_ptr(), s.len()) }
}
fn log_dec(mut value: u64) {
    let mut buf = [0; 20];
    let mut n = 0;
    if value == 0 {
        log(b"0");
        return;
    }
    while value != 0 {
        buf[n] = b'0' + (value % 10) as u8;
        n += 1;
        value /= 10;
    }
    buf[..n].reverse();
    log(&buf[..n]);
}
fn signed(value: i32) {
    if value < 0 {
        log(b"-");
        log_dec((-value) as u64);
    } else {
        log_dec(value as u64);
    }
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
pub(crate) unsafe fn start_progress_worker() -> bool {
    unsafe {
        if g!(progress_worker_started) {
            return true;
        }
        let name = b"audio-progress";
        let request = task_api::CreateRequest {
            name: name.as_ptr(),
            name_len: name.len(),
            state: 1,
            role: 1,
            needs_fpu: true,
            smp_eligible: false,
            smp_r4x: false,
            owner_kind: 0,
            owner_context: ptr::null_mut(),
            entry: Some(progress_worker),
        };
        let mut failure = 0;
        if task_api::r4os_rust_task_create(&request, false, &mut failure).is_null() {
            return false;
        }
        g!(progress_worker_started) = true;
        true
    }
}
// Retain original checked multiply/add overflow and deadline rounding behavior.
#[allow(clippy::manual_div_ceil)]
fn schedule_progress(pending: &AtomicU64, milliseconds: u64) {
    let ticks = 1u64.max((milliseconds * timer::frequency() as u64 + 999) / 1000);
    let due = timer::r4os_rust_timer_control(7, ticks, 0);
    let mut previous = pending.load(Ordering::Acquire);
    while due < previous {
        match pending.compare_exchange_weak(previous, due, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => {
                unsafe {
                    sync_objects::r4os_rust_sync_event(PROGRESS.0.get(), 0, 0);
                }
                return;
            }
            Err(actual) => previous = actual,
        }
    }
}
unsafe extern "C" fn progress_worker() {
    loop {
        let now = timer::tick_count();
        if unsafe { run_progress_if_due(now) } {
            continue;
        }
        let due = next_progress_tick();
        unsafe {
            sync_objects::r4os_rust_sync_event(
                PROGRESS.0.get(),
                2,
                if due == u64::MAX {
                    u64::MAX
                } else {
                    due.saturating_sub(now)
                },
            );
        }
    }
}
fn next_progress_tick() -> u64 {
    MIX_DUE
        .load(Ordering::Acquire)
        .min(BACKEND_DUE.load(Ordering::Acquire))
}
fn claim_progress(pending: &AtomicU64, now: u64) -> bool {
    let due = pending.load(Ordering::Acquire);
    due != u64::MAX
        && now >= due
        && pending
            .compare_exchange(due, u64::MAX, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
}
unsafe fn run_progress_if_due(now: u64) -> bool {
    unsafe {
        if now < next_progress_tick() {
            return false;
        }
        if claim_progress(&MIX_DUE, now) {
            relock();
            let result = pump_available_locked(true);
            unlock();
            if result == BUSY {
                schedule_progress(&MIX_DUE, 2);
            }
        }
        if claim_progress(&BACKEND_DUE, now) {
            service_backend_progress();
        }
        true
    }
}
unsafe fn service_backend_progress() {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            schedule_progress(&BACKEND_DUE, 2);
            return;
        };
        let Some(slot) = g!(active_audio_slot) else {
            return;
        };
        let p = backend(slot);
        if (*p).closing {
            return;
        }
        let config = (*p).config;
        if let Some(status) = config.status {
            let mut value = BackendStatus::default();
            if status(config.context, &mut value) == BUSY {
                schedule_progress(&BACKEND_DUE, 2);
            }
        }
    }
}
pub(crate) unsafe fn configure_sid_model(model: &[u8]) {
    unsafe {
        g!(sid_model_option) = Span {
            pointer: model.as_ptr(),
            len: model.len(),
        };
        apply_configured_sid_model();
    }
}
pub(crate) unsafe fn apply_configured_sid_model() -> i32 {
    unsafe {
        if let Some(p) = sid_engine() {
            let config = (*p).config;
            if let Some(set_model) = config.sid_model {
                g!(sid_last_result) = set_model(config.context, sid_model_id());
                return g!(sid_last_result);
            }
        }
        NO_ENDPOINT
    }
}
pub(crate) unsafe fn sid_model_name_z() -> *const u8 {
    if unsafe { name_eq(g!(sid_model_option).bytes(), b"6581") } {
        c"6581".as_ptr().cast()
    } else {
        c"8580".as_ptr().cast()
    }
}
pub(crate) unsafe fn sid_protocol_status() -> SidProtocolStatus {
    unsafe {
        SidProtocolStatus {
            source: protocol_source(b"audio.sid"),
            model: g!(sid_r4p_model),
            register: g!(sid_r4p_register),
            io: g!(sid_r4p_io),
            missing_required: g!(sid_missing_required),
            dispatch_fail: g!(sid_dispatch_failures),
            last_result: g!(sid_last_result),
            last_kind: g!(sid_last_kind),
            last_voice: g!(sid_last_voice),
        }
    }
}
pub(crate) unsafe fn open_stream(rate: u32, channels: u16, format: u16) -> i32 {
    unsafe { open_stream_for_owner(Owner::KERNEL, rate, channels, format) }
}
pub(crate) unsafe fn open_stream_for_owner(
    owner: Owner,
    rate: u32,
    channels: u16,
    format: u16,
) -> i32 {
    unsafe {
        if !owner.valid() || !valid_stream_format(rate, channels, format) {
            return INVALID;
        }
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        if !active_output_present() {
            return NO_ENDPOINT;
        }
        let Some(slot) = free_stream_slot() else {
            return a::service_api_result_full;
        };
        let p = stream(slot);
        let ring = if (*p).ring.len == RING_BYTES {
            (*p).ring
        } else {
            let pointer = heap::r4os_rust_heap_alloc(RING_BYTES, 16);
            if pointer.is_null() {
                return -3;
            }
            Ring {
                pointer,
                len: RING_BYTES,
            }
        };
        let id = g!(next_stream_id);
        g!(next_stream_id) = id.wrapping_add(1);
        if g!(next_stream_id) == 0 {
            g!(next_stream_id) = 1;
        }
        p.write(Stream {
            state: StreamState::Open,
            id,
            owner,
            rate,
            channels,
            format,
            ring,
            ..Stream::EMPTY
        });
        boot(b"[AUDIO] stream open id=");
        boot_dec(id as u64);
        boot(b"\r\n");
        id.try_into().unwrap()
    }
}
pub(crate) unsafe fn write_stream(id: u32, data: *const u8, byte_count: u32) -> i32 {
    unsafe { write_stream_for_owner(Owner::KERNEL, id, data, byte_count) }
}
pub(crate) unsafe fn write_stream_for_owner(
    owner: Owner,
    id: u32,
    data: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe {
        let write_start = timer::tick_count();
        if !owner.valid() {
            return INVALID;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        let Some(p) = stream_by_owner(owner, id) else {
            return a::service_api_result_not_found;
        };
        if byte_count == 0 {
            return 0;
        }
        if (*p).transaction {
            return BUSY;
        }
        if !active_output_present() {
            return NO_ENDPOINT;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return BUSY;
        }
        (*p).transaction = true;
        let _transaction = Transaction { stream: p, token };
        let _timing = WriteTiming(write_start);
        let Some(frame_bytes) = pcm::source_frame_bytes((*p).channels, (*p).format) else {
            return INVALID;
        };
        let requested_bytes = byte_count as usize;
        let requested_frames = requested_bytes / frame_bytes;
        if requested_frames == 0 {
            return INVALID;
        }
        let mut fitting_frames =
            (((*p).ring.len - (*p).available) / pcm::TARGET_FRAME_BYTES) as u64 * (*p).rate as u64
                / pcm::TARGET_RATE as u64;
        if fitting_frames == 0 {
            let result = pump_available_locked(true);
            if result < 0 {
                return result;
            }
            fitting_frames = (((*p).ring.len - (*p).available) / pcm::TARGET_FRAME_BYTES) as u64
                * (*p).rate as u64
                / pcm::TARGET_RATE as u64;
        }
        let accepted_frames = requested_frames.min(fitting_frames.try_into().unwrap());
        if accepted_frames == 0 {
            return BUSY;
        }
        let accepted_bytes = accepted_frames * frame_bytes;
        let input = slice::from_raw_parts(data, accepted_bytes);
        let capacity = (*p).ring.len - (*p).available;
        let mut resampler = (*p).resampler;
        let rate = (*p).rate;
        let channels = (*p).channels;
        let format = (*p).format;
        unlock();
        let mut converted = core::mem::MaybeUninit::<[u8; RING_BYTES]>::uninit();
        resampler.begin_chunk(rate, channels, format);
        // Initialize the bounded Rust stack span only for conversion. Native direct spans retain
        // caller input; neither path holds a Rust view into resident stream metadata.
        let output = if let Some(direct) = pcm::take_direct_chunk(
            &mut resampler,
            input,
            rate,
            channels,
            format,
            capacity,
            ptr::null_mut(),
        ) {
            direct
        } else {
            let scratch = &mut converted.write([0u8; RING_BYTES])[..capacity];
            let produced = pcm::convert_streaming_to_stereo_s16(
                &mut resampler,
                input,
                rate,
                channels,
                format,
                scratch,
                ptr::null_mut(),
            );
            &scratch[..produced]
        };
        relock();
        if !resampler.chunk_done {
            return BUSY;
        }
        let was_empty = (*p).available == 0;
        assert_eq!(
            mixer::ring_write(
                (*p).ring.bytes_mut(),
                &mut (*p).write_pos,
                &mut (*p).available,
                output
            ),
            output.len()
        );
        (*p).resampler = resampler;
        if was_empty {
            (*p).deferred_once = false;
        }
        g!(stream_available_high_water) =
            g!(stream_available_high_water).max((*p).available as u64);
        (*p).total_written = (*p).total_written.wrapping_add(accepted_bytes as u64);
        g!(total_stream_writes) = g!(total_stream_writes).wrapping_add(1);
        if accepted_bytes < requested_bytes {
            g!(stream_write_truncations) = g!(stream_write_truncations).wrapping_add(1);
        }
        if pump_available_locked(false) == BUSY {
            schedule_progress(&MIX_DUE, 2);
        }
        accepted_bytes.try_into().unwrap()
    }
}
pub(crate) unsafe fn close_stream(id: u32) -> i32 {
    unsafe { close_stream_for_owner(Owner::KERNEL, id) }
}
pub(crate) unsafe fn close_stream_for_owner(owner: Owner, id: u32) -> i32 {
    unsafe {
        if !owner.valid() {
            return INVALID;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        let Some(p) = stream_by_owner(owner, id) else {
            return a::service_api_result_not_found;
        };
        if (*p).transaction || (*p).read_reserved {
            return BUSY;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return BUSY;
        }
        (*p).transaction = true;
        let _transaction = Transaction { stream: p, token };
        let finish = finish_stream_pcm_locked(p);
        if finish < 0 {
            return finish;
        }
        let pump = pump_available_locked(true);
        if pump < 0 {
            return pump;
        }
        let stopped = stop_closing_streams_locked(1);
        if stopped < 0 {
            return stopped;
        }
        if !release_stream(p) {
            return BUSY;
        }
        boot(b"[AUDIO] stream close id=");
        boot_dec(id as u64);
        boot(b"\r\n");
        0
    }
}
pub(crate) unsafe fn close_streams_for_owner(owner: Owner) -> bool {
    unsafe {
        if !owner.valid() || (owner.instance_id == 0 && owner.generation == 0) {
            return false;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return false;
        };
        let mut reserved = [false; MAX_STREAMS];
        for (i, reserve) in reserved.iter_mut().enumerate() {
            let p = stream(i);
            if (*p).state != StreamState::Open || !mixer::same_owner((*p).owner, owner) {
                continue;
            }
            if (*p).transaction || (*p).read_reserved {
                return false;
            }
            *reserve = true;
        }
        let token = task_context::enter();
        if !token.admitted() {
            return false;
        }
        for (i, value) in reserved.iter().enumerate() {
            if *value {
                (*stream(i)).transaction = true;
            }
        }
        let _transactions = Transactions { reserved, token };
        let mut matching = 0;
        let mut owner_queued = false;
        for (i, value) in reserved.iter().enumerate() {
            if !*value {
                continue;
            }
            let p = stream(i);
            if finish_stream_pcm_locked(p) < 0 {
                return false;
            }
            matching += 1;
            owner_queued |= (*p).available != 0;
        }
        if matching == 0 {
            return true;
        }
        if owner_queued && active_output_present() && pump_available_locked(true) < 0 {
            return false;
        }
        if stop_closing_streams_locked(matching) < 0 {
            return false;
        }
        for (i, value) in reserved.iter().enumerate() {
            let p = stream(i);
            if *value && (*p).read_reserved {
                return false;
            }
            if !*value && (*p).state == StreamState::Open && mixer::same_owner((*p).owner, owner) {
                return false;
            }
        }
        let mut released = true;
        for (i, value) in reserved.iter().enumerate() {
            if !*value {
                continue;
            }
            let p = stream(i);
            g!(stream_dropped_bytes) = g!(stream_dropped_bytes).wrapping_add((*p).available as u64);
            (*p).available = 0;
            if !release_stream(p) {
                released = false;
            }
        }
        released
    }
}
pub(crate) unsafe fn close_all_streams() -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            return false;
        };
        if !lock() {
            return false;
        }
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            if (*p).transaction || (*p).read_reserved {
                unlock();
                return false;
            }
        }
        for i in 0..MAX_STREAMS {
            (*stream(i)).transaction = true;
        }
        unlock();
        let stopped = stop_active_pcm_owned();
        relock();
        for i in 0..MAX_STREAMS {
            (*stream(i)).transaction = false;
        }
        if stopped < 0 {
            unlock();
            return false;
        }
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            if (*p).state != StreamState::Open {
                continue;
            }
            g!(stream_dropped_bytes) = g!(stream_dropped_bytes)
                .wrapping_add(((*p).available + pcm::pending_tail_bytes(&(*p).resampler)) as u64);
            release_stream(p);
        }
        unlock();
        true
    }
}
pub(crate) unsafe fn set_volume(id: u32, volume: u32) -> i32 {
    unsafe { set_volume_for_owner(Owner::KERNEL, id, volume) }
}
pub(crate) unsafe fn set_volume_for_owner(owner: Owner, id: u32, volume: u32) -> i32 {
    unsafe {
        if !owner.valid() {
            return INVALID;
        }
        let Some(_lock) = StreamLock::acquire() else {
            return BUSY;
        };
        let Some(p) = stream_by_owner(owner, id) else {
            return a::service_api_result_not_found;
        };
        (*p).volume = volume;
        0
    }
}
pub(crate) unsafe fn sid_acquire() -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -1 };
        let c = (*p).config;
        let Some(acquire) = c.sid_acquire else {
            return -3;
        };
        if g!(sid_acquired) {
            return -2;
        }
        let result = acquire(c.context);
        g!(sid_last_result) = result;
        if result > 0 {
            g!(sid_acquired) = true;
            g!(sid_pcm_pending_len) = 0;
        }
        result
    }
}
pub(crate) unsafe fn sid_write_register(handle: u32, register: u8, value: u8) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(write) = c.sid_write else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        if register >= 25 {
            return -2;
        }
        g!(sid_last_register) = register;
        g!(sid_last_value) = value;
        g!(sid_write_count) += 1;
        g!(sid_last_result) = write(c.context, handle, register as u32, value as u32);
        g!(sid_last_result)
    }
}
pub(crate) unsafe fn sid_release(handle: u32) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(release) = c.sid_release else {
            return -3;
        };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        g!(sid_last_result) = release(c.context, handle);
        g!(sid_acquired) = false;
        g!(sid_pcm_pending_len) = 0;
        g!(sid_last_result)
    }
}
pub(crate) unsafe fn sid_load_data(
    handle: u32,
    load_addr: u16,
    data: *const u8,
    byte_count: u32,
) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(load) = c.sid_load else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        if byte_count == 0 || byte_count > 65_536 {
            return -2;
        }
        g!(sid_last_result) = load(c.context, handle, load_addr as u32, data, byte_count);
        if g!(sid_last_result) == 0 {
            byte_count.try_into().unwrap()
        } else {
            g!(sid_last_result)
        }
    }
}
pub(crate) unsafe fn sid_init(handle: u32, init_addr: u16, song: u16) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(init) = c.sid_init else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        g!(sid_last_result) = init(c.context, handle, init_addr as u32, song as u32);
        g!(sid_last_result)
    }
}
pub(crate) unsafe fn sid_play_frame(handle: u32, play_addr: u16, frame_hz: u16) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(play) = c.sid_play else { return -3 };
        let Some(render) = c.sid_pcm else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        if g!(sid_pcm_pending_len) != 0 {
            return flush_pending_sid_pcm();
        }
        g!(sid_last_result) = play(c.context, handle, play_addr as u32, frame_hz as u32);
        if g!(sid_last_result) != 0 {
            return g!(sid_last_result);
        }
        let rendered = render(
            c.context,
            handle,
            ptr::addr_of_mut!(g!(sid_pcm_pending)).cast(),
            SID_RENDER_BYTES as u32,
        );
        g!(sid_last_result) = rendered;
        if rendered <= 0
            || rendered > SID_RENDER_BYTES as i32
            || !(rendered as u32).is_multiple_of(pcm::TARGET_FRAME_BYTES as u32)
        {
            g!(sid_pcm_fail) = g!(sid_pcm_fail).wrapping_add(1);
            g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
            return if rendered < 0 { rendered } else { INVALID };
        }
        g!(sid_pcm_pending_len) = rendered as u32;
        g!(sid_pcm_writes) = g!(sid_pcm_writes).wrapping_add(1);
        (*p).renders = (*p).renders.wrapping_add(1);
        flush_pending_sid_pcm()
    }
}
pub(crate) unsafe fn sid_stop(handle: u32) -> i32 {
    unsafe {
        let Some(p) = sid_engine() else { return -3 };
        let c = (*p).config;
        let Some(stop) = c.stop else { return -3 };
        if handle != 1 || !g!(sid_acquired) {
            return -1;
        }
        g!(sid_last_result) = stop(c.context);
        if g!(sid_last_result) == 0 {
            (*p).stops = (*p).stops.wrapping_add(1);
        }
        g!(sid_pcm_pending_len) = 0;
        stop_active_pcm();
        g!(sid_last_result)
    }
}
unsafe fn flush_pending_sid_pcm() -> i32 {
    unsafe {
        let len = g!(sid_pcm_pending_len) as usize;
        if len == 0 {
            return 0;
        }
        let data = slice::from_raw_parts(ptr::addr_of!(g!(sid_pcm_pending)).cast(), len);
        let result = write_active_pcm(data, DEFAULT_RATE, DEFAULT_CHANNELS, FORMAT_S16LE)
            .unwrap_or(NO_ENDPOINT);
        g!(sid_last_result) = result;
        if result == 0 {
            g!(sid_pcm_pending_len) = 0;
            g!(sid_pcm_ok) = g!(sid_pcm_ok).wrapping_add(1);
            g!(total_backend_ok) = g!(total_backend_ok).wrapping_add(1);
        } else if result != BUSY {
            g!(sid_pcm_fail) = g!(sid_pcm_fail).wrapping_add(1);
            g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
        }
        result
    }
}
pub(crate) unsafe fn midi_open_synth(backend_z: *const u8) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(requested) = copy_z(backend_z, &mut buf) else {
            return -1;
        };
        if g!(midi_acquired) {
            return -3;
        }
        let selected = if requested.is_empty() {
            let Some(slot) = find_renderable_synth() else {
                return -4;
            };
            slot
        } else {
            let Some(slot) = find_synth(requested) else {
                return -2;
            };
            if !synth_can_render_midi(slot) {
                return -4;
            }
            slot
        };
        (*synth(selected)).pcm_pending_len = 0;
        g!(active_midi_synth_slot) = Some(selected);
        g!(midi_acquired) = true;
        1
    }
}
pub(crate) unsafe fn midi_send(handle: u32, channel: u8, status: u8, data1: u8, data2: u8) -> i32 {
    unsafe {
        if handle != 1 || !g!(midi_acquired) {
            return -1;
        }
        if channel > 15 {
            return -2;
        }
        if status == 0 {
            let encoded = data1 as u32 | ((data2 as u32) << 8);
            let requested = if encoded == 0 {
                SYNTH_RENDER_MAX_FRAMES
            } else {
                encoded
            };
            if requested > SYNTH_RENDER_MAX_FRAMES {
                return INVALID;
            }
            return render_active_midi(requested);
        }
        let Some(event) = classify_midi_event(channel, status, data1, data2) else {
            return -3;
        };
        g!(midi_last_channel) = channel;
        g!(midi_last_status) = event.normalized_status;
        g!(midi_last_data1) = data1;
        g!(midi_last_data2) = data2;
        g!(midi_last_event) = event.event;
        g!(midi_send_count) += 1;
        send_active_midi(channel, event.normalized_status, data1, data2)
    }
}
pub(crate) unsafe fn midi_close(handle: u32) -> i32 {
    unsafe {
        if handle != 1 || !g!(midi_acquired) {
            return -1;
        }
        let stopped = stop_active_midi();
        if let Some(slot) = g!(active_midi_synth_slot) {
            (*synth(slot)).pcm_pending_len = 0;
        }
        g!(active_midi_synth_slot) = None;
        g!(midi_acquired) = false;
        stop_pcm_if_no_streams();
        stopped
    }
}
pub(crate) unsafe fn midi_protocol_status() -> MidiProtocolStatus {
    unsafe {
        MidiProtocolStatus {
            source: protocol_source(b"audio.midi"),
            classify: g!(midi_r4p_classify),
            missing_required: g!(midi_missing_required),
            dispatch_fail: g!(midi_dispatch_failures),
            last_result: g!(midi_last_result),
            last_event: g!(midi_last_event),
        }
    }
}
pub(crate) unsafe fn opl3_protocol_status() -> Opl3ProtocolStatus {
    unsafe {
        Opl3ProtocolStatus {
            source: protocol_source(b"audio.opl3"),
            registers: g!(opl3_r4p_register),
            midi: g!(opl3_r4p_midi),
            missing_required: g!(opl3_missing_required),
            dispatch_fail: g!(opl3_dispatch_failures),
            last_result: g!(opl3_last_result),
            last_kind: g!(opl3_last_write_kind),
            last_action: g!(opl3_last_action),
        }
    }
}
pub(crate) unsafe fn opl3_reset() -> i32 {
    unsafe {
        let mut op = a::AudioOpl3Op::default();
        if has_protocol(b"audio.opl3") {
            dispatch_opl3(a::audio_opl3_op_reset, &mut op);
        }
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        let c = (*synth(slot)).config;
        let Some(reset) = c.opl3_reset else { return -3 };
        reset(c.context)
    }
}
pub(crate) unsafe fn opl3_write_register(bank: u8, reg: u8, value: u8) -> i32 {
    unsafe {
        if classify_opl3_register(bank, reg, value).is_none() {
            return -1;
        }
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        let c = (*synth(slot)).config;
        let Some(write) = c.opl3_write else { return -3 };
        write(c.context, bank, reg, value)
    }
}
pub(crate) unsafe fn opl3_run_register_demo() {
    unsafe {
        opl3_reset();
        for (bank, reg, value) in [
            (1, 0x05, 0x01),
            (0, 0x01, 0x20),
            (0, 0x20, 0x21),
            (0, 0x23, 0x01),
            (0, 0x40, 0x18),
            (0, 0x43, 0x00),
            (0, 0x60, 0xf3),
            (0, 0x63, 0xf3),
            (0, 0x80, 0x77),
            (0, 0x83, 0x77),
            (0, 0xe0, 0),
            (0, 0xe3, 0),
            (0, 0xa0, 0x98),
            (0, 0xb0, 0x31),
            (0, 0xc0, 0x31),
        ] {
            opl3_write_register(bank, reg, value);
        }
    }
}
pub(crate) unsafe fn opl3_render_block() -> i32 {
    unsafe {
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        render_synth_pcm(slot, SYNTH_RENDER_MAX_FRAMES)
    }
}
pub(crate) unsafe fn opl3_stop() -> i32 {
    unsafe {
        let Some(slot) = find_synth(b"OPL3") else {
            return -2;
        };
        let p = synth(slot);
        let c = (*p).config;
        if let Some(stop) = c.stop {
            let result = stop(c.context);
            if result >= 0 {
                (*p).stops += 1;
            }
            return result;
        }
        0
    }
}
pub(crate) unsafe fn alloc_dma_buffer(bytes: u32, _alignment: u32) -> u64 {
    unsafe {
        if bytes == 0 || bytes > 4096 {
            return 0;
        }
        let mut address = 0;
        if phys::r4os_rust_phys_alloc(&mut address) {
            address
        } else {
            0
        }
    }
}
pub(crate) fn free_dma_buffer(address: u64, _bytes: u32) {
    if address != 0 {
        phys::r4os_rust_phys_free(address);
    }
}
fn role(name: &[u8]) -> Bytes {
    Bytes {
        data: name.as_ptr(),
        len: name.len(),
    }
}
unsafe fn protocol_source(name: &[u8]) -> Span {
    unsafe {
        let s = r4p::r4os_rust_r4p_source(role(name), true);
        Span {
            pointer: s.data,
            len: s.len,
        }
    }
}
unsafe fn has_protocol(name: &[u8]) -> bool {
    unsafe { r4p::r4os_rust_r4p_has_active(role(name)) }
}
unsafe fn classify_midi_event(
    channel: u8,
    status: u8,
    data1: u8,
    data2: u8,
) -> Option<a::AudioMidiOp> {
    unsafe {
        if has_protocol(b"audio.midi") {
            let mut op = a::AudioMidiOp {
                channel,
                status,
                data1,
                data2,
                ..Default::default()
            };
            if dispatch_midi(a::audio_midi_op_classify_event, &mut op)
                && op.result == a::audio_midi_result_ok
            {
                g!(midi_r4p_classify) = g!(midi_r4p_classify).wrapping_add(1);
                g!(midi_last_event) = op.event;
                return Some(op);
            }
        }
        g!(midi_missing_required) = g!(midi_missing_required).wrapping_add(1);
        g!(midi_last_result) = -5;
        g!(midi_last_event) = a::audio_midi_event_ignore;
        None
    }
}
unsafe fn dispatch_protocol<T>(name: &[u8], opcode: u32, op: *mut T) -> i32 {
    unsafe {
        let input = a::ProtocolBuffer {
            data: op.cast(),
            len: size_of::<T>() as u32,
            capacity: size_of::<T>() as u32,
            flags: 0,
            reserved: 0,
        };
        let mut output = a::ProtocolBuffer::default();
        r4p::r4os_rust_r4p_dispatch(role(name), opcode, &input, &mut output)
    }
}
unsafe fn dispatch_midi(opcode: u32, op: *mut a::AudioMidiOp) -> bool {
    unsafe {
        let result = dispatch_protocol(b"audio.midi", opcode, op);
        g!(midi_last_result) = result;
        if result != a::audio_midi_result_ok {
            g!(midi_dispatch_failures) = g!(midi_dispatch_failures).wrapping_add(1);
            return false;
        }
        true
    }
}
unsafe fn dispatch_opl3(opcode: u32, op: *mut a::AudioOpl3Op) -> bool {
    unsafe {
        let result = dispatch_protocol(b"audio.opl3", opcode, op);
        g!(opl3_last_result) = result;
        if result != a::audio_opl3_result_ok {
            g!(opl3_dispatch_failures) = g!(opl3_dispatch_failures).wrapping_add(1);
            return false;
        }
        true
    }
}
unsafe fn classify_opl3_register(bank: u8, reg: u8, value: u8) -> Option<a::AudioOpl3Op> {
    unsafe {
        if has_protocol(b"audio.opl3") {
            let mut op = a::AudioOpl3Op {
                bank,
                register: reg,
                value,
                ..Default::default()
            };
            if dispatch_opl3(a::audio_opl3_op_write_register, &mut op)
                && op.result == a::audio_opl3_result_ok
            {
                g!(opl3_r4p_register) = g!(opl3_r4p_register).wrapping_add(1);
                g!(opl3_last_write_kind) = op.write_kind;
                return Some(op);
            }
        }
        g!(opl3_missing_required) = g!(opl3_missing_required).wrapping_add(1);
        g!(opl3_last_result) = -5;
        g!(opl3_last_write_kind) = a::audio_opl3_write_other;
        None
    }
}
unsafe fn render_active_midi(requested: u32) -> i32 {
    unsafe {
        let Some(slot) = g!(active_midi_synth_slot) else {
            return NO_ENDPOINT;
        };
        g!(midi_render_ticks) = g!(midi_render_ticks).wrapping_add(1);
        render_synth_pcm(slot, requested)
    }
}
unsafe fn send_active_midi(channel: u8, status: u8, data1: u8, data2: u8) -> i32 {
    unsafe {
        let Some(slot) = g!(active_midi_synth_slot) else {
            return NO_ENDPOINT;
        };
        let p = synth(slot);
        let c = (*p).config;
        if let Some(send) = c.send {
            let result = send(c.context, channel, status, data1, data2);
            if result >= 0 {
                (*p).sends = (*p).sends.wrapping_add(1);
            }
            return result;
        }
        if c.send_legacy != 0 {
            r4os_zig_audio_legacy(
                2,
                c.send_legacy,
                ptr::null(),
                0,
                0,
                0,
                0,
                channel,
                status,
                data1,
                data2,
            );
            (*p).sends = (*p).sends.wrapping_add(1);
            return 0;
        }
        NO_ENDPOINT
    }
}
unsafe fn stop_active_midi() -> i32 {
    unsafe {
        let Some(slot) = g!(active_midi_synth_slot) else {
            return 0;
        };
        let p = synth(slot);
        let c = (*p).config;
        if let Some(stop) = c.stop {
            let result = stop(c.context);
            if result >= 0 {
                (*p).stops = (*p).stops.wrapping_add(1);
            }
            return result;
        }
        if c.stop_legacy != 0 {
            r4os_zig_audio_legacy(3, c.stop_legacy, ptr::null(), 0, 0, 0, 0, 0, 0, 0, 0);
            (*p).stops = (*p).stops.wrapping_add(1);
        }
        0
    }
}
unsafe fn render_synth_pcm(slot: usize, requested: u32) -> i32 {
    unsafe {
        if requested == 0 || requested > SYNTH_RENDER_MAX_FRAMES {
            return INVALID;
        }
        let p = synth(slot);
        if (*p).pcm_pending_len != 0 {
            return flush_pending_synth_pcm(p);
        }
        let c = (*p).config;
        let Some(render) = c.render_pcm else {
            return NO_ENDPOINT;
        };
        let capacity = requested * pcm::TARGET_FRAME_BYTES as u32;
        let rendered = render(
            c.context,
            ptr::addr_of_mut!((*p).pcm_pending).cast(),
            capacity,
            DEFAULT_RATE,
            DEFAULT_CHANNELS,
            FORMAT_S16LE,
        );
        if rendered < 0 {
            return rendered;
        }
        if rendered == 0 {
            return 0;
        }
        if rendered != capacity as i32 {
            return INVALID;
        }
        (*p).pcm_pending_len = rendered as u32;
        (*p).renders = (*p).renders.wrapping_add(1);
        flush_pending_synth_pcm(p)
    }
}
unsafe fn flush_pending_synth_pcm(p: *mut SynthEngine) -> i32 {
    unsafe {
        let len = (*p).pcm_pending_len as usize;
        if len == 0 {
            return 0;
        }
        let data = slice::from_raw_parts(ptr::addr_of!((*p).pcm_pending).cast(), len);
        let result = write_active_pcm(data, DEFAULT_RATE, DEFAULT_CHANNELS, FORMAT_S16LE)
            .unwrap_or(NO_ENDPOINT);
        if result == 0 {
            (*p).pcm_pending_len = 0;
            g!(total_backend_ok) = g!(total_backend_ok).wrapping_add(1);
        } else if result != BUSY {
            g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
        }
        result
    }
}
pub(crate) unsafe fn register_audio_backend_z(name: *const u8, pointer: *const c_void) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        if register_audio_backend_internal(
            name,
            BackendRegistration {
                pointer,
                ..Default::default()
            },
        ) {
            0
        } else {
            -2
        }
    }
}
pub(crate) unsafe fn register_mixer_backend_z(name: *const u8, pointer: *const c_void) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        store_mixer(name, pointer);
        boot(b"[AUDIO] mixer backend ");
        boot(name);
        boot(b" [OK]\r\n");
        0
    }
}
pub(crate) unsafe fn register_synth_engine_z(name: *const u8, pointer: *const c_void) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        if register_synth_engine(name, pointer) {
            0
        } else {
            -2
        }
    }
}
pub(crate) unsafe fn register_audio_backend(name: &[u8], pointer: *const c_void) {
    unsafe {
        register_audio_backend_internal(
            name,
            BackendRegistration {
                pointer,
                ..Default::default()
            },
        );
    }
}
pub(crate) unsafe fn register_external_audio_backend_z(
    name: *const u8,
    config: BackendRegistration,
) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        if register_audio_backend_internal(name, config) {
            0
        } else {
            -2
        }
    }
}
pub(crate) unsafe fn register_external_audio_outputs(
    name: &[u8],
    outputs: a::AudioOutputExtension,
    idle: Option<StopPcmCtxFn>,
) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let p = backend(slot);
        if (*p).closing {
            return false;
        }
        (*p).outputs = Some(outputs);
        (*p).idle = idle;
        g!(initial_output_pending) = true;
        true
    }
}
unsafe fn output_record(slot: usize, index: u32, out: *mut a::AudioOutputInfo) -> i32 {
    unsafe {
        let p = backend(slot);
        out.write(a::AudioOutputInfo::default());
        let c = (*p).config;
        if !(*p).registered || (*p).closing || (c.write_legacy == 0 && c.write.is_none()) {
            return 0;
        }
        if let Some(outputs) = (*p).outputs {
            let result = (outputs.query)(c.context as u64, index, out);
            if result != 1 {
                return result;
            }
            if (*out).version != 1
                || (*out).size != size_of::<a::AudioOutputInfo>() as u32
                || !(*out).id.contains(&0)
                || !(*out).name.contains(&0)
            {
                return -1;
            }
            (*out).flags &= !a::audio_output_flag_active;
            if g!(active_audio_slot) == Some(slot)
                && (outputs.active)(c.context as u64) == i32::try_from(index).unwrap()
            {
                (*out).flags |= a::audio_output_flag_active;
            }
            return 1;
        }
        if index != 0 {
            return 0;
        }
        let len = (*p).name_len;
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*p).name).cast::<u8>(),
            ptr::addr_of_mut!((*out).id).cast(),
            len,
        );
        ptr::copy_nonoverlapping(
            ptr::addr_of!((*p).name).cast::<u8>(),
            ptr::addr_of_mut!((*out).name).cast(),
            len,
        );
        (*out).availability = a::audio_output_available;
        if let Some(status) = c.status {
            let mut value = BackendStatus::default();
            if status(c.context, &mut value) < 0 || value.active == 0 {
                (*out).availability = a::audio_output_unavailable;
            }
        }
        if g!(active_audio_slot) == Some(slot) {
            (*out).flags = a::audio_output_flag_active;
        }
        (*out).preferred_rate = DEFAULT_RATE;
        (*out).channels = DEFAULT_CHANNELS;
        (*out).format = FORMAT_S16LE;
        1
    }
}
pub(crate) unsafe fn audio_output_info(index: u32, out: *mut a::AudioOutputInfo) -> i32 {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let mut ordinal = 0;
        for slot in 0..MAX_AUDIO_BACKENDS {
            for local in 0..32 {
                let result = output_record(slot, local, out);
                if result < 0 {
                    return result;
                }
                if result == 0 {
                    break;
                }
                if ordinal == index {
                    return 1;
                }
                ordinal += 1;
            }
        }
        out.write(a::AudioOutputInfo::default());
        0
    }
}
unsafe fn select_output(slot: usize, local: u32) -> i32 {
    unsafe {
        let p = backend(slot);
        if let Some(outputs) = (*p).outputs {
            let result = (outputs.select)((*p).config.context as u64, local);
            if result < 0 {
                return result;
            }
        }
        if g!(active_audio_slot) != Some(slot) {
            let stopped = stop_active_pcm_owned();
            if stopped < 0 {
                return stopped;
            }
            set_active_backend(slot);
        }
        0
    }
}
pub(crate) unsafe fn audio_select_output(id: &[u8]) -> i32 {
    unsafe {
        if id.is_empty() || id.len() >= 64 {
            return INVALID;
        }
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let mut record = a::AudioOutputInfo::default();
        for slot in 0..MAX_AUDIO_BACKENDS {
            for local in 0..32 {
                let result = output_record(slot, local, &mut record);
                if result < 0 {
                    return result;
                }
                if result == 0 {
                    break;
                }
                let Some(len) = record.id.iter().position(|&b| b == 0) else {
                    return -1;
                };
                if id != &record.id[..len] {
                    continue;
                }
                if record.availability != a::audio_output_available {
                    return NO_ENDPOINT;
                }
                let selected = select_output(slot, local);
                if selected == 0 {
                    g!(initial_output_pending) = false;
                }
                return selected;
            }
        }
        NO_ENDPOINT
    }
}
// Keep the original complete-snapshot selection and success publication scope.
#[allow(clippy::collapsible_if)]
unsafe fn resolve_initial_output() {
    unsafe {
        if !g!(initial_output_pending) {
            return;
        }
        let mut best_slot = None;
        let mut best_local = 0;
        let mut best_score = 0;
        let mut record = a::AudioOutputInfo::default();
        for slot in 0..MAX_AUDIO_BACKENDS {
            for local in 0..32 {
                let result = output_record(slot, local, &mut record);
                if result < 0 {
                    return;
                }
                if result == 0 {
                    break;
                }
                if record.availability != a::audio_output_available {
                    continue;
                }
                let p = backend(slot);
                let score = if record.kind == a::audio_output_kind_hdmi {
                    100
                } else if let Some(outputs) = (*p).outputs {
                    if (outputs.active)((*p).config.context as u64) == local as i32 {
                        20
                    } else {
                        1
                    }
                } else {
                    1
                };
                if score <= best_score {
                    continue;
                }
                best_score = score;
                best_slot = Some(slot);
                best_local = local;
            }
        }
        if let Some(slot) = best_slot {
            if select_output(slot, best_local) == 0 {
                g!(initial_output_pending) = false;
            }
        }
    }
}
pub(crate) unsafe fn register_external_synth_engine_z(
    name: *const u8,
    config: SynthRegistration,
) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        if register_synth_internal(name, config) {
            0
        } else {
            -2
        }
    }
}
pub(crate) unsafe fn unregister_audio_backend_by_name(name: &[u8]) -> i32 {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return BUSY;
        };
        let Some(slot) = find_backend(name) else {
            return -1;
        };
        let was_active = g!(active_audio_slot) == Some(slot);
        if was_active {
            let result = stop_active_pcm_owned();
            if result < 0 {
                return result;
            }
        }
        backend(slot).write(AudioBackend::EMPTY);
        if was_active {
            g!(active_audio_slot) = None;
            OUTPUT_PRESENT.store(0, Ordering::Release);
            activate_first_backend();
        }
        boot(b"[AUDIO] audio backend unregistered ");
        boot(name);
        boot(b" [OK]\r\n");
        0
    }
}
pub(crate) unsafe fn unregister_audio_backend_z(name: *const u8) -> i32 {
    unsafe {
        let mut buf = [0; MAX_NAME];
        let Some(name) = copy_z(name, &mut buf) else {
            return -1;
        };
        unregister_audio_backend_by_name(name)
    }
}
pub(crate) unsafe fn register_synth_engine(name: &[u8], pointer: *const c_void) -> bool {
    unsafe {
        register_synth_internal(
            name,
            SynthRegistration {
                pointer,
                migration: name_eq(name, b"SID"),
                ..Default::default()
            },
        )
    }
}
pub(crate) unsafe fn register_native_synth_engine(
    name: &[u8],
    send: usize,
    render: usize,
    stop: usize,
) {
    unsafe {
        register_synth_internal(
            name,
            SynthRegistration {
                flags: if send != 0 { 1 } else { 0 },
                send_legacy: send,
                render_legacy: render,
                stop_legacy: stop,
                ..Default::default()
            },
        );
    }
}
unsafe fn register_synth_internal(name: &[u8], config: SynthRegistration) -> bool {
    unsafe {
        let Some(slot) = find_synth(name).or_else(|| free_synth_slot()) else {
            boot(b"[AUDIO][WARN] synth engine registry full\r\n");
            return false;
        };
        let p = synth(slot);
        let sends = (*p).sends;
        let renders = (*p).renders;
        let stops = (*p).stops;
        p.write(SynthEngine::EMPTY);
        (*p).registered = true;
        (*p).config = config;
        (*p).sends = sends;
        (*p).renders = renders;
        (*p).stops = stops;
        (*p).name_len = if name.len() < MAX_NAME {
            name.len()
        } else {
            MAX_NAME - 1
        };
        let len = (*p).name_len;
        if len > 0 {
            ptr::copy_nonoverlapping(name.as_ptr(), ptr::addr_of_mut!((*p).name).cast(), len);
        }
        boot(b"[AUDIO] synth engine registered ");
        boot(slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), len));
        boot(if config.external {
            b" external"
        } else if config.migration {
            b" migration"
        } else if config.send_legacy != 0 || config.render_legacy != 0 {
            b" native"
        } else {
            b" generic"
        });
        boot(b" [OK]\r\n");
        true
    }
}
pub(crate) unsafe fn register_native_audio_backend(name: &[u8], write: usize, stop: usize) {
    unsafe {
        register_audio_backend_internal(
            name,
            BackendRegistration {
                write_legacy: write,
                stop_legacy: stop,
                ..Default::default()
            },
        );
    }
}
pub(crate) unsafe fn select_audio_backend(name: &[u8]) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            boot(b"[AUDIO][WARN] audio backend not found ");
            boot(name);
            boot(b"\r\n");
            return false;
        };
        let p = backend(slot);
        if (*p).closing {
            return false;
        }
        let c = (*p).config;
        if c.write_legacy == 0 && c.write.is_none() {
            boot(b"[AUDIO][WARN] audio backend has no PCM output ");
            boot(name);
            boot(b"\r\n");
            return false;
        }
        if g!(active_audio_slot) == Some(slot) {
            return true;
        }
        if stop_active_pcm_owned() < 0 {
            return false;
        }
        set_active_backend(slot);
        true
    }
}
pub(crate) unsafe fn unregister_audio_backend(name: &[u8]) -> bool {
    unsafe { unregister_audio_backend_by_name(name) == 0 }
}
pub(crate) unsafe fn set_audio_backend_closing(name: &[u8], closing: bool) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let p = backend(slot);
        (*p).closing = closing;
        if g!(active_audio_slot) == Some(slot) {
            let c = (*p).config;
            OUTPUT_PRESENT.store(
                u8::from(!closing && (c.write_legacy != 0 || c.write.is_some())),
                Ordering::Release,
            );
        }
        true
    }
}
pub(crate) unsafe fn audio_backend_registered(name: &[u8]) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        find_backend(name).is_some()
    }
}
pub(crate) unsafe fn audio_backend_family_status(family: &[u8]) -> BackendFamilyStatus {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return BackendFamilyStatus::default();
        };
        let mut result = BackendFamilyStatus::default();
        for slot in 0..MAX_AUDIO_BACKENDS {
            let p = backend(slot);
            if !(*p).registered {
                continue;
            }
            let name = slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len);
            if name.len() < family.len() || !name[..family.len()].eq_ignore_ascii_case(family) {
                continue;
            }
            if name.len() != family.len() && name[family.len()] != b'-' {
                continue;
            }
            result.registered = true;
            if (*p).closing {
                continue;
            }
            result.active |= g!(active_audio_slot) == Some(slot) && (*p).active;
            let c = (*p).config;
            result.has_output |= c.write_legacy != 0 || c.write.is_some();
        }
        result
    }
}
pub(crate) unsafe fn audio_backend_active(name: &[u8]) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let p = backend(slot);
        g!(active_audio_slot) == Some(slot) && (*p).active && !(*p).closing
    }
}
pub(crate) unsafe fn audio_backend_has_output(name: &[u8]) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name) else {
            return false;
        };
        let c = (*backend(slot)).config;
        c.write_legacy != 0 || c.write.is_some()
    }
}
unsafe fn register_mixer_backend(name: &[u8], pointer: *const c_void) {
    unsafe {
        store_mixer(name, pointer);
        boot(b"[AUDIO] mixer backend ");
        boot(name);
        boot(b" [OK]\r\n");
    }
}
unsafe fn store_mixer(name: &[u8], pointer: *const c_void) {
    unsafe {
        let p = ptr::addr_of_mut!(g!(mixer_backend));
        p.write(NamedBackend {
            active: true,
            pointer,
            ..NamedBackend::EMPTY
        });
        (*p).name_len = if name.len() < MAX_NAME {
            name.len()
        } else {
            MAX_NAME - 1
        };
        if (*p).name_len > 0 {
            ptr::copy_nonoverlapping(
                name.as_ptr(),
                ptr::addr_of_mut!((*p).name).cast(),
                (*p).name_len,
            );
        }
    }
}
unsafe fn register_audio_backend_internal(name: &[u8], config: BackendRegistration) -> bool {
    unsafe {
        let Some(_output) = OutputGuard::acquire(true) else {
            return false;
        };
        let Some(slot) = find_backend(name).or_else(|| free_backend_slot()) else {
            boot(b"[AUDIO][WARN] audio backend registry full\r\n");
            return false;
        };
        let was_active = g!(active_audio_slot) == Some(slot);
        let p = backend(slot);
        if (*p).closing {
            return false;
        }
        p.write(AudioBackend::EMPTY);
        (*p).registered = true;
        (*p).config = config;
        (*p).name_len = if name.len() < MAX_NAME {
            name.len()
        } else {
            MAX_NAME - 1
        };
        let len = (*p).name_len;
        if len > 0 {
            ptr::copy_nonoverlapping(name.as_ptr(), ptr::addr_of_mut!((*p).name).cast(), len);
        }
        boot(b"[AUDIO] audio backend registered ");
        boot(slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), len));
        boot(if config.write_legacy != 0 {
            b" native"
        } else if config.write.is_some() {
            b" r4d"
        } else {
            b" generic"
        });
        boot(b" [OK]\r\n");
        if was_active
            || (g!(active_audio_slot).is_none()
                && (config.write_legacy != 0 || config.write.is_some()))
        {
            set_active_backend(slot);
        }
        true
    }
}
unsafe fn set_active_backend(slot: usize) {
    unsafe {
        for i in 0..MAX_AUDIO_BACKENDS {
            (*backend(i)).active = false;
        }
        let p = backend(slot);
        (*p).active = true;
        g!(active_audio_slot) = Some(slot);
        let c = (*p).config;
        OUTPUT_PRESENT.store(
            u8::from(!(*p).closing && (c.write_legacy != 0 || c.write.is_some())),
            Ordering::Release,
        );
        boot(b"[AUDIO] active audio backend ");
        boot(slice::from_raw_parts(
            ptr::addr_of!((*p).name).cast(),
            (*p).name_len,
        ));
        boot(b" [OK]\r\n");
    }
}
unsafe fn write_active_pcm(data: &[u8], rate: u32, channels: u16, format: u16) -> Option<i32> {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            return Some(BUSY);
        };
        write_active_pcm_owned(data, rate, channels, format)
    }
}
// Keep the original legacy/contextual/no-endpoint callback branches explicit.
#[allow(clippy::question_mark)]
unsafe fn write_active_pcm_owned(
    data: &[u8],
    rate: u32,
    channels: u16,
    format: u16,
) -> Option<i32> {
    unsafe {
        resolve_initial_output();
        let slot = g!(active_audio_slot)?;
        let p = backend(slot);
        if (*p).closing {
            return Some(BUSY);
        }
        let c = (*p).config;
        if c.limits_present && !c.limits.accepts(rate, channels, format) {
            return Some(INVALID);
        }
        let result = if c.write_legacy != 0 {
            if r4os_zig_audio_legacy(
                0,
                c.write_legacy,
                data.as_ptr(),
                data.len(),
                rate,
                channels,
                format,
                0,
                0,
                0,
                0,
            ) != 0
            {
                0
            } else {
                -1
            }
        } else if let Some(write) = c.write {
            write(
                c.context,
                data.as_ptr(),
                data.len().try_into().unwrap(),
                rate,
                channels,
                format,
            )
        } else {
            return None;
        };
        if result == 0 {
            schedule_progress(&BACKEND_DUE, 10);
        }
        Some(result)
    }
}
unsafe fn stop_active_pcm() {
    unsafe {
        stop_active_pcm_result();
    }
}
unsafe fn stop_active_pcm_result() -> i32 {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            return BUSY;
        };
        idle_active_pcm_owned()
    }
}
unsafe fn idle_active_pcm_owned() -> i32 {
    unsafe {
        let Some(slot) = g!(active_audio_slot) else {
            return 0;
        };
        let p = backend(slot);
        let c = (*p).config;
        if c.stop_legacy != 0 {
            r4os_zig_audio_legacy(1, c.stop_legacy, ptr::null(), 0, 0, 0, 0, 0, 0, 0, 0);
            return 0;
        }
        backend_contract::finish_pcm(EndMode::Idle, (*p).closing, c.context, c.stop, (*p).idle)
    }
}
unsafe fn stop_active_pcm_owned() -> i32 {
    unsafe {
        let Some(slot) = g!(active_audio_slot) else {
            return 0;
        };
        let p = backend(slot);
        let c = (*p).config;
        if c.stop_legacy != 0 {
            r4os_zig_audio_legacy(1, c.stop_legacy, ptr::null(), 0, 0, 0, 0, 0, 0, 0, 0);
            return 0;
        }
        backend_contract::finish_pcm(EndMode::Stop, (*p).closing, c.context, c.stop, (*p).idle)
    }
}
unsafe fn activate_first_backend() {
    unsafe {
        for i in 0..MAX_AUDIO_BACKENDS {
            let p = backend(i);
            let c = (*p).config;
            if !(*p).registered || (*p).closing || (c.write_legacy == 0 && c.write.is_none()) {
                continue;
            }
            set_active_backend(i);
            return;
        }
        boot(b"[AUDIO][WARN] no active audio backend\r\n");
    }
}
unsafe fn find_backend(name: &[u8]) -> Option<usize> {
    unsafe {
        for i in 0..MAX_AUDIO_BACKENDS {
            let p = backend(i);
            if (*p).registered
                && name_eq(
                    slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len),
                    name,
                )
            {
                return Some(i);
            }
        }
        None
    }
}
unsafe fn find_synth(name: &[u8]) -> Option<usize> {
    unsafe {
        for i in 0..MAX_SYNTH_ENGINES {
            let p = synth(i);
            if (*p).registered
                && name_eq(
                    slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len),
                    name,
                )
            {
                return Some(i);
            }
        }
        None
    }
}
unsafe fn sid_engine() -> Option<*mut SynthEngine> {
    unsafe { find_synth(b"SID").map(|i| synth(i)) }
}
unsafe fn sid_model_id() -> u32 {
    unsafe {
        if name_eq(g!(sid_model_option).bytes(), b"6581") {
            a::audio_sid_model_6581 as u32
        } else {
            a::audio_sid_model_8580 as u32
        }
    }
}
// Keep the original OPL3 then MIDI then registry fallback selection order.
#[allow(clippy::collapsible_if)]
unsafe fn find_renderable_synth() -> Option<usize> {
    unsafe {
        if let Some(slot) = find_synth(b"OPL3") {
            if synth_can_render_midi(slot) {
                return Some(slot);
            }
        }
        if let Some(slot) = find_synth(b"MIDI") {
            if synth_can_render_midi(slot) {
                return Some(slot);
            }
        }
        for i in 0..MAX_SYNTH_ENGINES {
            if (*synth(i)).registered && synth_can_render_midi(i) {
                return Some(i);
            }
        }
        None
    }
}
unsafe fn synth_can_render_midi(slot: usize) -> bool {
    unsafe {
        let c = (*synth(slot)).config;
        c.flags & 1 != 0 && (c.send_legacy != 0 || c.send.is_some()) && c.render_pcm.is_some()
    }
}
unsafe fn free_backend_slot() -> Option<usize> {
    unsafe { (0..MAX_AUDIO_BACKENDS).find(|&i| !(*backend(i)).registered) }
}
unsafe fn free_synth_slot() -> Option<usize> {
    unsafe { (0..MAX_SYNTH_ENGINES).find(|&i| !(*synth(i)).registered) }
}
fn valid_stream_format(rate: u32, channels: u16, format: u16) -> bool {
    rate != 0
        && rate <= 192000
        && channels != 0
        && channels <= 2
        && (format == FORMAT_S16LE || format == FORMAT_U8)
}
fn active_output_present() -> bool {
    OUTPUT_PRESENT.load(Ordering::Acquire) != 0
}
unsafe fn free_stream_slot() -> Option<usize> {
    unsafe {
        (0..MAX_STREAMS).find(|&i| {
            let state = (*stream(i)).state;
            state == StreamState::Empty || state == StreamState::Closed
        })
    }
}
unsafe fn stream_by_owner(owner: Owner, id: u32) -> Option<*mut Stream> {
    unsafe {
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            if (*p).state == StreamState::Open
                && (*p).id == id
                && mixer::same_owner((*p).owner, owner)
            {
                return Some(p);
            }
        }
        None
    }
}
unsafe fn open_stream_count() -> u32 {
    unsafe {
        (0..MAX_STREAMS)
            .filter(|&i| (*stream(i)).state == StreamState::Open)
            .count() as u32
    }
}
unsafe fn release_stream(p: *mut Stream) -> bool {
    unsafe {
        if (*p).read_reserved {
            return false;
        }
        let id = (*p).id;
        let ring = (*p).ring;
        p.write(Stream {
            state: StreamState::Closed,
            id,
            ring,
            ..Stream::EMPTY
        });
        true
    }
}
unsafe fn stop_closing_streams_locked(closing_count: u32) -> i32 {
    unsafe {
        if open_stream_count() != closing_count || g!(sid_acquired) || g!(midi_acquired) {
            return 0;
        }
        unlock();
        let Some(_output) = OutputGuard::acquire(false) else {
            relock();
            return BUSY;
        };
        relock();
        if open_stream_count() != closing_count || g!(sid_acquired) || g!(midi_acquired) {
            return 0;
        }
        unlock();
        let result = idle_active_pcm_owned();
        relock();
        result
    }
}
unsafe fn finish_stream_pcm_locked(p: *mut Stream) -> i32 {
    unsafe {
        while (*p).resampler.previous_valid {
            if !(*p).resampler.chunk_done {
                return BUSY;
            }
            let free_bytes = (*p).ring.len - (*p).available;
            let mut scratch = [0; MIX_QUANTUM_BYTES];
            let mut resampler = (*p).resampler;
            unlock();
            let produced = pcm::finish_streaming_to_stereo_s16(
                &mut resampler,
                &mut scratch[..free_bytes.min(MIX_QUANTUM_BYTES)],
                ptr::null_mut(),
            );
            relock();
            (*p).resampler = resampler;
            if mixer::ring_write(
                (*p).ring.bytes_mut(),
                &mut (*p).write_pos,
                &mut (*p).available,
                &scratch[..produced],
            ) != produced
            {
                return BUSY;
            }
            if !(*p).resampler.previous_valid {
                return 0;
            }
            let pumped = pump_available_locked(true);
            if pumped < 0 {
                return pumped;
            }
        }
        0
    }
}
unsafe fn pump_available_locked(force: bool) -> i32 {
    unsafe {
        unlock();
        let Some(output) = OutputGuard::acquire(false) else {
            relock();
            return BUSY;
        };
        let result = pump_available_output_owned(force);
        drop(output);
        relock();
        result
    }
}
unsafe fn pump_available_output_owned(force: bool) -> i32 {
    unsafe {
        relock();
        let _lock = StreamLock;
        let max_passes = MAX_STREAMS * (RING_BYTES / MIX_QUANTUM_BYTES + 2);
        for _ in 0..max_passes {
            let mut selected = [false; MAX_STREAMS];
            let mut open_count = 0;
            let mut ready_count = 0;
            let mut single_ready = 0;
            let mut chunk_bytes = MIX_QUANTUM_BYTES;
            for i in 0..MAX_STREAMS {
                let p = stream(i);
                if (*p).state != StreamState::Open {
                    continue;
                }
                open_count += 1;
                if (*p).available < pcm::TARGET_FRAME_BYTES {
                    continue;
                }
                ready_count += 1;
                single_ready = i;
            }
            if ready_count == 0 {
                return 0;
            }
            if ready_count == 1 {
                let p = stream(single_ready);
                if open_count > 1 && !force && !(*p).deferred_once {
                    (*p).deferred_once = true;
                    schedule_progress(&MIX_DUE, 2);
                    return 0;
                }
                selected[single_ready] = true;
                chunk_bytes = chunk_bytes.min((*p).available);
            } else {
                for (i, value) in selected.iter_mut().enumerate() {
                    let p = stream(i);
                    if (*p).state != StreamState::Open || (*p).available < pcm::TARGET_FRAME_BYTES {
                        continue;
                    }
                    *value = true;
                    chunk_bytes = chunk_bytes.min((*p).available);
                }
            }
            chunk_bytes -= chunk_bytes % pcm::TARGET_FRAME_BYTES;
            if chunk_bytes == 0 {
                return 0;
            }
            let mut sources = [mixer::Source::EMPTY; MAX_STREAMS];
            let mut source_count = 0;
            for (i, value) in selected.iter().enumerate() {
                if !*value {
                    continue;
                }
                let p = stream(i);
                (*p).read_reserved = true;
                sources[source_count] = mixer::Source {
                    ring: (*p).ring.pointer,
                    len: (*p).ring.len,
                    read_pos: (*p).read_pos,
                    volume: (*p).volume,
                };
                source_count += 1;
            }
            unlock();
            let scratch = ptr::addr_of_mut!(g!(mix_scratch)).cast::<u8>();
            mixer::mix(
                &sources[..source_count],
                slice::from_raw_parts_mut(scratch, chunk_bytes),
                ptr::null_mut(),
            );
            let start = timer::tick_count();
            let result = write_active_pcm_owned(
                slice::from_raw_parts(scratch, chunk_bytes),
                pcm::TARGET_RATE,
                pcm::TARGET_CHANNELS,
                pcm::FORMAT_S16LE,
            )
            .unwrap_or(NO_ENDPOINT);
            relock();
            for (i, value) in selected.iter().enumerate() {
                if *value {
                    (*stream(i)).read_reserved = false;
                }
            }
            g!(backend_write_calls) = g!(backend_write_calls).wrapping_add(1);
            record_tick_stat(
                ptr::addr_of_mut!(g!(backend_write_total_ticks)),
                ptr::addr_of_mut!(g!(backend_write_max_ticks)),
                ptr::addr_of_mut!(g!(backend_write_last_ticks)),
                start,
            );
            if result != 0 {
                if result != BUSY {
                    g!(total_backend_fail) = g!(total_backend_fail).wrapping_add(1);
                }
                return result;
            }
            g!(total_backend_ok) = g!(total_backend_ok).wrapping_add(1);
            for (i, value) in selected.iter().enumerate() {
                if !*value {
                    continue;
                }
                let p = stream(i);
                mixer::ring_consume(
                    (*p).ring.len,
                    &mut (*p).read_pos,
                    &mut (*p).available,
                    chunk_bytes,
                );
                (*p).deferred_once = false;
            }
        }
        BUSY
    }
}
unsafe fn stop_pcm_if_no_streams() {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            return;
        };
        relock();
        let empty = open_stream_count() == 0;
        unlock();
        if empty {
            idle_active_pcm_owned();
        }
    }
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
fn yes(value: bool) -> &'static [u8] {
    if value { b"yes" } else { b"no" }
}
unsafe fn backend_name<'a>(p: *const AudioBackend) -> &'a [u8] {
    unsafe { slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len) }
}
unsafe fn synth_name<'a>(p: *const SynthEngine) -> &'a [u8] {
    unsafe { slice::from_raw_parts(ptr::addr_of!((*p).name).cast(), (*p).name_len) }
}
pub(crate) unsafe fn dump_status() {
    unsafe {
        let Some(_output) = OutputGuard::acquire(false) else {
            return;
        };
        log(b"Audio:\r\n");
        log(b"  Backend: ");
        if let Some(slot) = g!(active_audio_slot) {
            log(backend_name(backend(slot)));
        } else {
            log(b"none");
        }
        log(b"\r\n");
        log(b"  Mixer: ");
        if g!(mixer_backend).active {
            let p = ptr::addr_of!(g!(mixer_backend));
            log(slice::from_raw_parts(
                ptr::addr_of!((*p).name).cast(),
                (*p).name_len,
            ));
        } else {
            log(b"none");
        }
        log(b"\r\n");
        log(b"  Writes: total=");
        log_dec(g!(total_stream_writes));
        log(b" backend_ok=");
        log_dec(g!(total_backend_ok));
        log(b" backend_fail=");
        log_dec(g!(total_backend_fail));
        log(b"\r\n");
        dump_audio_backends();
        dump_synth_engines();
        dump_sid_status();
        dump_midi_status();
        relock();
        let _lock = StreamLock;
        let mut open_count = 0;
        for i in 0..MAX_STREAMS {
            let p = stream(i);
            if (*p).state != StreamState::Open {
                continue;
            }
            open_count += 1;
            log(b"  Stream ");
            log_dec((*p).id as u64);
            log(b": rate=");
            log_dec((*p).rate as u64);
            log(b" ch=");
            log_dec((*p).channels as u64);
            log(b" fmt=");
            log_dec((*p).format as u64);
            log(b" bytes=");
            log_dec((*p).total_written);
            log(b"\r\n");
        }
        if open_count == 0 {
            log(b"  Streams: none open\r\n");
        }
    }
}
unsafe fn dump_sid_status() {
    unsafe {
        log(b"  SID: available=");
        let engine = sid_engine();
        log(yes(engine.is_some()));
        log(b" external=");
        log(yes(engine.is_some_and(|p| (*p).config.external)));
        log(b" acquired=");
        log(yes(g!(sid_acquired)));
        log(b" model=");
        let z = sid_model_name_z();
        log(slice::from_raw_parts(z, 4));
        log(b" writes=");
        log_dec(g!(sid_write_count));
        log(b" last_reg=");
        log_dec(g!(sid_last_register) as u64);
        log(b" last_val=");
        log_dec(g!(sid_last_value) as u64);
        log(b" last=");
        signed(g!(sid_last_result));
        log(b" pcm=");
        log_dec(g!(sid_pcm_writes));
        log(b" ok=");
        log_dec(g!(sid_pcm_ok));
        log(b" fail=");
        log_dec(g!(sid_pcm_fail));
        if let Some(p) = engine {
            let c = (*p).config;
            if let Some(status_fn) = c.status {
                let mut status = SynthStatus::default();
                if status_fn(c.context, &mut status) == 0 {
                    log(b" r4d_renders=");
                    log_dec(status.renders);
                    log(b" r4d_stops=");
                    log_dec(status.stops);
                    log(b" r4d_last=");
                    signed(status.last_result);
                }
            }
        }
        log(b"\r\n");
    }
}
unsafe fn dump_midi_status() {
    unsafe {
        log(b"  MIDI: available=");
        log(yes(
            find_synth(b"MIDI").is_some() || find_synth(b"OPL3").is_some()
        ));
        log(b" acquired=");
        log(yes(g!(midi_acquired)));
        log(b" sends=");
        log_dec(g!(midi_send_count));
        log(b" last_ch=");
        log_dec(g!(midi_last_channel) as u64);
        log(b" last_status=");
        log_dec(g!(midi_last_status) as u64);
        log(b" data=");
        log_dec(g!(midi_last_data1) as u64);
        log(b"/");
        log_dec(g!(midi_last_data2) as u64);
        log(b" render_ticks=");
        log_dec(g!(midi_render_ticks));
        log(b" source=");
        log(protocol_source(b"audio.midi").bytes());
        log(b" r4p=");
        log_dec(g!(midi_r4p_classify));
        log(b"/");
        log_dec(g!(midi_dispatch_failures));
        log(b" required_missing=");
        log_dec(g!(midi_missing_required));
        log(b" event=");
        log_dec(g!(midi_last_event) as u64);
        log(b"\r\n");
        log(b"  OPL3 engine: external=");
        log(yes(find_synth(b"OPL3").is_some()));
        log(b" source=");
        log(protocol_source(b"audio.opl3").bytes());
        log(b" r4p=");
        log_dec(g!(opl3_r4p_register));
        log(b"/");
        log_dec(g!(opl3_r4p_midi));
        log(b"/");
        log_dec(g!(opl3_dispatch_failures));
        log(b" required_missing=");
        log_dec(g!(opl3_missing_required));
        log(b" kind=");
        log_dec(g!(opl3_last_write_kind) as u64);
        log(b" action=");
        log_dec(g!(opl3_last_action) as u64);
        log(b"\r\n");
    }
}
unsafe fn dump_audio_backends() {
    unsafe {
        log(b"  Backends:");
        let mut count = 0;
        for i in 0..MAX_AUDIO_BACKENDS {
            let p = backend(i);
            if !(*p).registered {
                continue;
            }
            count += 1;
            log(b" ");
            if (*p).active {
                log(b"*");
            }
            log(backend_name(p));
            let c = (*p).config;
            log(if c.write_legacy != 0 {
                b"(native)"
            } else if c.write.is_some() {
                b"(r4d)"
            } else {
                b"(generic)"
            });
        }
        if count == 0 {
            log(b" none");
        }
        log(b"\r\n");
        dump_audio_backend_statuses();
    }
}
unsafe fn dump_audio_backend_statuses() {
    unsafe {
        for i in 0..MAX_AUDIO_BACKENDS {
            let p = backend(i);
            if !(*p).registered || (*p).closing {
                continue;
            }
            let c = (*p).config;
            let Some(status_fn) = c.status else { continue };
            let mut status = BackendStatus {
                active: u32::from((*p).active),
                ..Default::default()
            };
            let result = status_fn(c.context, &mut status);
            log(b"  Backend ");
            log(backend_name(p));
            log(b" status: ");
            if result != 0 {
                log(b"rc=");
                signed(result);
                log(b"\r\n");
                continue;
            }
            log(b"active=");
            log(yes(status.active != 0));
            log(b" writes=");
            log_dec(status.writes);
            log(b" underruns=");
            log_dec(status.underruns);
            log(b" errors=");
            log_dec(status.errors);
            log(b" last=");
            signed(status.last_result);
            log(b"\r\n");
        }
    }
}
unsafe fn dump_synth_engines() {
    unsafe {
        log(b"  Synths:");
        let mut count = 0;
        for i in 0..MAX_SYNTH_ENGINES {
            let p = synth(i);
            if !(*p).registered {
                continue;
            }
            count += 1;
            log(b" ");
            if g!(active_midi_synth_slot) == Some(i) {
                log(b"*");
            }
            log(synth_name(p));
            log(b"(");
            let c = (*p).config;
            if c.external {
                log(b"r4d:");
            }
            if c.migration {
                log(b"migration:");
            }
            if c.send_legacy != 0 || c.send.is_some() {
                log(b"midi");
            }
            if c.render_legacy != 0 || c.render.is_some() {
                log(b"+pcm");
            }
            if c.send_legacy == 0 && c.render_legacy == 0 && c.send.is_none() && c.render.is_none()
            {
                log(b"generic");
            }
            log(b")");
        }
        if count == 0 {
            log(b" none");
        }
        log(b"\r\n");
        dump_synth_counters();
    }
}
unsafe fn dump_synth_counters() {
    unsafe {
        for i in 0..MAX_SYNTH_ENGINES {
            let p = synth(i);
            if !(*p).registered || ((*p).sends == 0 && (*p).renders == 0 && (*p).stops == 0) {
                continue;
            }
            log(b"  Synth ");
            log(synth_name(p));
            log(b": sends=");
            log_dec((*p).sends);
            log(b" renders=");
            log_dec((*p).renders);
            log(b" stops=");
            log_dec((*p).stops);
            log(b"\r\n");
        }
    }
}
