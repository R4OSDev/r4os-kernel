//! Original R4D audio registry and callback admission. A copied slot carries
//! identity across foreign code; the real audio core still owns device cleanup.
use crate::{
    audio::{backend_contract::Limits, core as audio, types::BackendRegistration},
    boot::log,
    contract as a,
    driver::api_admission as admission,
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{MaybeUninit, offset_of, size_of},
    ptr,
};
mod capture;
pub(crate) use capture::{
    capture_io, close_for_owner as close_capture_for_owner, register_capture2,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Record {
    used: bool,
    owner: u32,
    name: [u8; 32],
    name_len: usize,
    descriptor: *const a::AudioBackend2,
    input: *const a::AudioCaptureBackend2,
    input_epoch: u64,
    input_closing: bool,
    capture: [capture::Lease; 4],
}
impl Record {
    const EMPTY: Self = Self {
        used: false,
        owner: 0,
        name: [0; 32],
        name_len: 0,
        descriptor: ptr::null(),
        input: ptr::null(),
        input_epoch: 0,
        input_closing: false,
        capture: [capture::Lease::EMPTY; 4],
    };
}
struct Storage(UnsafeCell<[Record; 8]>);
// SAFETY: the existing lifecycle/audio-core admission serializes enrolment,
// callback use and destruction; no registry borrow spans a foreign callback.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new([Record::EMPTY; 8]));
struct Callback(admission::CallbackToken);
impl Callback {
    fn enter(owner: u32) -> Option<Self> {
        let mut token = MaybeUninit::uninit();
        if unsafe { admission::r4os_rust_driver_api_storage_enter(owner, token.as_mut_ptr()) } {
            Some(Self(unsafe { token.assume_init() }))
        } else {
            None
        }
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        unsafe { admission::r4os_rust_driver_api_storage_leave(&self.0) }
    }
}
fn puts(bytes: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(bytes.as_ptr(), bytes.len()) }
}

/// # Safety
/// The R4D supplies its resident epoch callback for its own registered backend.
/// The Core output permit authenticates the exact context and retains the
/// registration through every subsequent synchronous callback.
pub(crate) unsafe extern "C" fn register_output_epoch(
    name: *const u8,
    callback: a::AudioOutputEpochFn,
) -> i32 {
    let owner = admission::r4os_rust_driver_api_owner_scalar(0);
    let bytes = unsafe { text(name) };
    let found = (0..8).find(|&i| {
        let r = unsafe { ptr::read(ptr::addr_of!((*STATE.0.get())[i])) };
        r.used && r.owner == owner && bytes == &r.name[..r.name_len]
    });
    let Some(index) = found else {
        return a::service_api_result_invalid;
    };
    let context = unsafe { ptr::addr_of_mut!((*STATE.0.get())[index]).cast::<c_void>() };
    unsafe {
        audio::register_external_audio_epoch(
            bytes,
            context,
            crate::audio::types::OutputEpochBinding {
                callback,
                query: output_epoch2,
            },
        )
    }
}

unsafe fn text<'a>(name: *const u8) -> &'a [u8] {
    let mut n = 0;
    while n < 512 && unsafe { *name.add(n) } != 0 {
        n += 1;
    }
    unsafe { core::slice::from_raw_parts(name, n) }
}
/// # Safety
/// The zero-terminated name is readable under the original audio-core removal
/// admission; failure retains the exact callback record and its owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_audio_unregister(name: *const u8) -> i32 {
    let bytes = unsafe { text(name) };
    for i in 0..8 {
        let r = unsafe { (*STATE.0.get())[i] };
        if r.used && bytes.eq_ignore_ascii_case(&r.name[..r.name_len]) {
            let rc = unsafe { capture::retire(i) };
            if rc != 0 {
                return rc;
            }
            break;
        }
    }
    let result = unsafe { audio::unregister_audio_backend_z(name) };
    if result != 0 {
        return result;
    }
    let input = unsafe { text(name) };
    for i in 0..8 {
        let record = unsafe { (*STATE.0.get())[i] };
        if record.used && input.eq_ignore_ascii_case(&record.name[..record.name_len]) {
            unsafe { (*STATE.0.get())[i] = Record::EMPTY };
            break;
        }
    }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_driver_audio_closing(owner: u32, closing: bool) -> bool {
    for i in 0..8 {
        let r = unsafe { (*STATE.0.get())[i] };
        if r.used && r.owner == owner && !unsafe { capture::closing(i, closing) } {
            return false;
        }
        if r.used
            && r.owner == owner
            && !unsafe { audio::set_audio_backend_closing(&r.name[..r.name_len], closing) }
        {
            return false;
        }
    }
    unsafe { audio::set_synth_driver_closing(owner, closing) }
}
#[repr(C)]
pub struct Cleanup {
    pub removed: u32,
    pub failed: bool,
}
/// # Safety
/// Output is an aligned resident private C record. Original terminal admission
/// persists through real device shutdown and audio-core unregister acknowledgements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_audio_cleanup(owner: u32, out: *mut Cleanup) {
    let mut result = Cleanup {
        removed: 0,
        failed: false,
    };
    for i in 0..8 {
        let r = unsafe { (*STATE.0.get())[i] };
        if !r.used || r.owner != owner {
            continue;
        }
        if unsafe { capture::retire(i) } != 0 {
            result.failed = true;
            continue;
        }
        let d = unsafe { &*r.descriptor };
        let (shutdown, context) = (d.shutdown, d.context);
        if let Some(shutdown) = shutdown
            && unsafe { shutdown(context) } != 0
        {
            result.failed = true;
            continue;
        }
        if unsafe { audio::unregister_audio_backend_by_name(&r.name[..r.name_len]) } != 0 {
            result.failed = true;
            continue;
        }
        unsafe { (*STATE.0.get())[i] = Record::EMPTY };
        result.removed += 1;
    }
    if !unsafe { audio::unregister_synth_driver(owner) } {
        result.failed = true;
    }
    unsafe { ptr::write(out, result) };
}
unsafe fn record(raw: *mut c_void) -> Option<Record> {
    if raw.is_null() {
        None
    } else {
        let r = unsafe { ptr::read(raw.cast::<Record>()) };
        r.used.then_some(r)
    }
}
/// New typed registration shares the existing exact driver/audio owner and
/// registry. Descriptor/callback storage is immutable and retained until real
/// unregister or terminal cleanup succeeds, including partial failure.
pub(crate) unsafe extern "C" fn register2(
    name: *const u8,
    descriptor: *const a::AudioBackend2,
) -> i32 {
    let Some(owner) = admission::backend_registration_owner() else {
        return -1;
    };
    if name.is_null() || descriptor.is_null() {
        return -1;
    }
    // Distinct header is checked before touching the full descriptor.
    if unsafe { (*descriptor).magic } != a::audio_backend2_magic
        || unsafe { (*descriptor).version } != a::audio_backend2_version
        || unsafe { (*descriptor).size } < size_of::<a::AudioBackend2>() as u32
    {
        return -1;
    }
    let d = unsafe { ptr::read(descriptor) };
    let outputs = [
        d.output_query.is_some(),
        d.output_select.is_some(),
        d.output_active.is_some(),
    ];
    let progress = [
        d.write_tracked.is_some(),
        d.playback.is_some(),
        d.flow.is_some(),
    ];
    if d.flags != 0
        || d.reserved != 0
        || d.write_pcm.is_none()
        || d.stop.is_none()
        || d.status.is_none()
        || d.formats == 0
        || d.formats & !(a::audio_backend_format_s16le | a::audio_backend_format_u8) != 0
        || d.max_channels == 0
        || d.max_channels > 8
        || d.min_rate != 0 && d.max_rate != 0 && d.min_rate > d.max_rate
        || outputs.iter().any(|&v| v) != outputs.iter().all(|&v| v)
        || progress.iter().any(|&v| v) != progress.iter().all(|&v| v)
        || d.idle.is_some() && !outputs[0]
    {
        return -1;
    }
    let Some(index) = (0..8).find(|&i| !unsafe { (*STATE.0.get())[i].used }) else {
        return -2;
    };
    let mut record = Record {
        used: true,
        owner,
        descriptor,
        ..Record::EMPTY
    };
    while record.name_len < 31 && unsafe { *name.add(record.name_len) } != 0 {
        record.name[record.name_len] = unsafe { *name.add(record.name_len) };
        record.name_len += 1;
    }
    if record.name_len == 0 || unsafe { *name.add(record.name_len) } != 0 {
        return -1;
    }
    let context = unsafe { ptr::addr_of_mut!((*STATE.0.get())[index]).cast::<c_void>() };
    unsafe { ptr::write(context.cast::<Record>(), record) };
    if unsafe {
        audio::register_external_audio_backend_z(
            name,
            BackendRegistration {
                context,
                write: Some(write2),
                stop: Some(stop2),
                status: Some(status2),
                limits_present: true,
                limits: Limits {
                    formats: d.formats,
                    min_rate: d.min_rate,
                    max_rate: d.max_rate,
                    max_channels: d.max_channels,
                },
            },
        )
    } != 0
    {
        unsafe { ptr::write(context.cast(), Record::EMPTY) };
        return -3;
    }
    let extension_ok = !outputs[0]
        || unsafe {
            audio::register_external_audio_outputs(
                &record.name[..record.name_len],
                a::AudioOutputExtension {
                    query: output_query2,
                    select: output_select2,
                    active: output_active2,
                },
                if d.idle.is_some() { Some(idle2) } else { None },
            )
        };
    let progress_ok = extension_ok
        && (!progress[0]
            || unsafe {
                audio::register_external_audio_playback(
                    &record.name[..record.name_len],
                    tracked_write2,
                    playback2,
                    flow2,
                )
            });
    if !progress_ok {
        if unsafe { audio::unregister_audio_backend_z(name) } == 0 {
            unsafe { ptr::write(context.cast(), Record::EMPTY) };
        }
        return -3;
    }
    puts(b"[R4D] register audio backend2 ");
    puts(&record.name[..record.name_len]);
    puts(b"\r\n");
    0
}

// The sole typed descriptor stays borrowed from the original exact driver
// registry until confirmed removal. Retain the original immutable-header
// guard; no old pointer layout or generation discriminator is interpreted.
unsafe fn record2(raw: *mut c_void) -> Option<(Record, *const a::AudioBackend2)> {
    let r = unsafe { record(raw) }?;
    if r.descriptor.is_null() || unsafe { (*r.descriptor).magic } != a::audio_backend2_magic {
        return None;
    }
    Some((r, r.descriptor))
}
unsafe extern "C" fn write2(
    raw: *mut c_void,
    data: *const u8,
    len: u32,
    rate: u32,
    channels: u16,
    format: u16,
) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    unsafe { d.write_pcm.unwrap()(d.context, data, len, rate, channels, format) }
}
unsafe extern "C" fn stop2(raw: *mut c_void) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    unsafe { d.stop.unwrap()(d.context) }
}
unsafe extern "C" fn idle2(raw: *mut c_void) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let call = d.idle.or(d.stop).unwrap();
    unsafe { call(d.context) }
}
unsafe extern "C" fn status2(raw: *mut c_void, out: *mut a::AudioBackendStatus) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let mut snapshot = a::AudioBackendStatus2::default();
    let result = unsafe { d.status.unwrap()(d.context, &mut snapshot) };
    if result != 0 {
        return result;
    }
    if snapshot.reserved != 0 {
        return -1;
    }
    unsafe {
        out.write(snapshot.expand_legacy());
    }
    0
}
unsafe extern "C" fn tracked_write2(
    raw: *mut c_void,
    data: *const u8,
    len: u32,
    rate: u32,
    channels: u16,
    format: u16,
    out: *mut a::AudioBackendWriteReceipt,
) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let Some(call) = d.write_tracked else {
        return -1;
    };
    unsafe { call(d.context, data, len, rate, channels, format, out) }
}
unsafe extern "C" fn playback2(raw: *mut c_void, out: *mut a::AudioBackendPlayback) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let Some(call) = d.playback else { return -1 };
    unsafe { call(d.context, out) }
}
unsafe extern "C" fn flow2(raw: *mut c_void, input: *const a::AudioBackendFlow) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let Some(call) = d.flow else { return -1 };
    unsafe { call(d.context, input) }
}
unsafe extern "C" fn output_query2(raw: u64, index: u32, out: *mut a::AudioOutputInfo) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw as *mut c_void) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let Some(call) = d.output_query else {
        return -1;
    };
    unsafe { call(d.context as u64, index, out) }
}
unsafe extern "C" fn output_select2(raw: u64, index: u32) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw as *mut c_void) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let Some(call) = d.output_select else {
        return -1;
    };
    unsafe { call(d.context as u64, index) }
}
unsafe extern "C" fn output_active2(raw: u64) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw as *mut c_void) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { &*d };
    let Some(call) = d.output_active else {
        return -1;
    };
    unsafe { call(d.context as u64) }
}
unsafe extern "C" fn output_epoch2(
    raw: *mut c_void,
    callback: a::AudioOutputEpochFn,
    out: *mut a::AudioOutputEpoch,
) -> i32 {
    let Some((r, d)) = (unsafe { record2(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return a::service_api_result_busy;
    };
    unsafe { callback((*d).context as u64, out) }
}
const _: () = {
    assert!(size_of::<Record>() == 400);
    assert!(offset_of!(Record, descriptor) == 48);
    assert!(offset_of!(Record, input) == 56);
    assert!(offset_of!(Record, capture) == 80);
};
