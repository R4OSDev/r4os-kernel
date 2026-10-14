//! Original R4D audio registry and callback admission. A copied slot carries
//! identity across foreign code; the real audio core still owns device cleanup.
use crate::{boot::log, contract as a, driver::api_admission as admission};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{MaybeUninit, offset_of, size_of},
    ptr,
};
#[derive(Clone, Copy)]
struct Record {
    used: bool,
    owner: u32,
    name: [u8; 32],
    name_len: usize,
    descriptor: *const a::AudioBackend,
}
impl Record {
    const EMPTY: Self = Self {
        used: false,
        owner: 0,
        name: [0; 32],
        name_len: 0,
        descriptor: ptr::null(),
    };
}
struct Storage(UnsafeCell<[Record; 8]>);
// SAFETY: the existing lifecycle/audio-core admission serializes enrolment,
// callback use and destruction; no registry borrow spans a foreign callback.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new([Record::EMPTY; 8]));
#[repr(C)]
struct Outputs {
    base: a::AudioBackend,
    outputs: a::AudioOutputExtension,
}
#[repr(C)]
struct Idle {
    base: Outputs,
    idle: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
}
const _: () = {
    assert!(size_of::<a::AudioBackend>() == 72);
    assert!(offset_of!(Outputs, outputs) == 72 && size_of::<Outputs>() == 96);
    assert!(offset_of!(Idle, idle) == 96 && size_of::<Idle>() == 104);
};
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
unsafe fn text<'a>(name: *const u8) -> &'a [u8] {
    let mut n = 0;
    while n < 512 && unsafe { *name.add(n) } != 0 {
        n += 1;
    }
    unsafe { core::slice::from_raw_parts(name, n) }
}
fn has_outputs(version: u32) -> bool {
    version == a::audio_backend_outputs_version || version == a::audio_backend_idle_version
}
unsafe fn valid(d: *const a::AudioBackend) -> bool {
    let version = unsafe { (*d).version };
    if version != a::audio_backend_version && !has_outputs(version) {
        puts(b"[R4D][ERROR] audio backend version mismatch\r\n");
        return false;
    }
    let size = unsafe { (*d).size };
    if size < 72 {
        puts(b"[R4D][ERROR] audio backend descriptor too small\r\n");
        return false;
    }
    if has_outputs(version) && size < 96 || version == a::audio_backend_idle_version && size < 104 {
        return false;
    }
    let value = unsafe { ptr::read(d) };
    if value.write_pcm.is_none() {
        puts(b"[R4D][ERROR] audio backend missing write_pcm\r\n");
        return false;
    }
    if value.formats & (a::audio_backend_format_s16le | a::audio_backend_format_u8) == 0 {
        puts(b"[R4D][ERROR] audio backend unsupported format mask\r\n");
        return false;
    }
    if value.max_channels == 0 || value.max_channels > 8 {
        puts(b"[R4D][ERROR] audio backend invalid channel count\r\n");
        return false;
    }
    if value.min_rate != 0 && value.max_rate != 0 && value.min_rate > value.max_rate {
        puts(b"[R4D][ERROR] audio backend invalid rate range\r\n");
        return false;
    }
    true
}
unsafe extern "C" {
    fn r4os_zig_audio_register(
        name: *const u8,
        descriptor: *const a::AudioBackend,
        context: *mut c_void,
        write: unsafe extern "C" fn(*mut c_void, *const u8, u32, u32, u16, u16) -> i32,
        stop: unsafe extern "C" fn(*mut c_void) -> i32,
        status: unsafe extern "C" fn(*mut c_void, *mut a::AudioBackendStatus) -> i32,
    ) -> i32;
    fn r4os_zig_audio_register_outputs(
        name: *const u8,
        len: usize,
        query: a::AudioOutputQueryFn,
        select: a::AudioOutputSelectFn,
        active: a::AudioOutputActiveFn,
        idle: Option<unsafe extern "C" fn(*mut c_void) -> i32>,
    ) -> bool;
    fn r4os_zig_audio_unregister(name: *const u8, len: usize, terminated: bool) -> i32;
    fn r4os_zig_audio_closing(name: *const u8, len: usize, closing: bool) -> bool;
}
/// # Safety
/// The resident descriptor keeps the original versioned prefix and callback
/// lifetime under the existing R4D/audio admission through confirmed teardown.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_audio_register(
    name: *const u8,
    descriptor: *const a::AudioBackend,
) -> i32 {
    if !unsafe { valid(descriptor) } {
        return -1;
    }
    let slot = (0..8).find(|&i| !unsafe { (*STATE.0.get())[i].used });
    let Some(index) = slot else {
        puts(b"[R4D][ERROR] audio backend table full\r\n");
        return -2;
    };
    let mut record = Record::EMPTY;
    record.used = true;
    record.owner = admission::r4os_rust_driver_api_owner_scalar(0);
    record.descriptor = descriptor;
    while record.name_len < 31 && unsafe { *name.add(record.name_len) } != 0 {
        record.name[record.name_len] = unsafe { *name.add(record.name_len) };
        record.name_len += 1;
    }
    let context = unsafe { ptr::addr_of_mut!((*STATE.0.get())[index]).cast::<c_void>() };
    unsafe { ptr::write(context.cast::<Record>(), record) };
    let result = unsafe { r4os_zig_audio_register(name, descriptor, context, write, stop, status) };
    if result != 0 {
        unsafe { ptr::write(context.cast(), Record::EMPTY) };
        return -3;
    }
    let version = unsafe { (*descriptor).version };
    if has_outputs(version)
        && !unsafe {
            r4os_zig_audio_register_outputs(
                record.name.as_ptr(),
                record.name_len,
                output_query,
                output_select,
                output_active,
                if version == a::audio_backend_idle_version {
                    Some(idle)
                } else {
                    None
                },
            )
        }
    {
        if unsafe { r4os_zig_audio_unregister(name, 0, true) } == 0 {
            unsafe { ptr::write(context.cast(), Record::EMPTY) };
        }
        return -3;
    }
    puts(b"[R4D] register audio backend ");
    puts(&record.name[..record.name_len]);
    puts(b"\r\n");
    0
}
/// # Safety
/// The zero-terminated name is readable under the original audio-core removal
/// admission; failure retains the exact callback record and its owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_audio_unregister(name: *const u8) -> i32 {
    let result = unsafe { r4os_zig_audio_unregister(name, 0, true) };
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
        if r.used
            && r.owner == owner
            && !unsafe { r4os_zig_audio_closing(r.name.as_ptr(), r.name_len, closing) }
        {
            return false;
        }
    }
    true
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
        let d = unsafe { ptr::read(r.descriptor) };
        if let Some(shutdown) = d.shutdown
            && unsafe { shutdown(d.context) } != 0
        {
            result.failed = true;
            continue;
        }
        if unsafe { r4os_zig_audio_unregister(r.name.as_ptr(), r.name_len, false) } != 0 {
            result.failed = true;
            continue;
        }
        unsafe { (*STATE.0.get())[i] = Record::EMPTY };
        result.removed += 1;
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
unsafe extern "C" fn write(
    raw: *mut c_void,
    data: *const u8,
    len: u32,
    rate: u32,
    channels: u16,
    format: u16,
) -> i32 {
    let Some(r) = (unsafe { record(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    let d = unsafe { ptr::read(r.descriptor) };
    unsafe { d.write_pcm.unwrap()(d.context, data, len, rate, channels, format) }
}
unsafe extern "C" fn stop(raw: *mut c_void) -> i32 {
    let Some(r) = (unsafe { record(raw) }) else {
        return -1;
    };
    let d = unsafe { ptr::read(r.descriptor) };
    let Some(stop) = d.stop else {
        return 0;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    unsafe { stop(d.context) }
}
unsafe extern "C" fn idle(raw: *mut c_void) -> i32 {
    let Some(r) = (unsafe { record(raw) }) else {
        return -1;
    };
    let d = unsafe { ptr::read(r.descriptor) };
    if d.version != a::audio_backend_idle_version || d.size < 104 {
        return -1;
    }
    let Some(idle) = (unsafe { (*r.descriptor.cast::<Idle>()).idle }) else {
        return unsafe { stop(raw) };
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    unsafe { idle(d.context) }
}
unsafe extern "C" fn status(raw: *mut c_void, out: *mut a::AudioBackendStatus) -> i32 {
    let Some(r) = (unsafe { record(raw) }) else {
        return -1;
    };
    let d = unsafe { ptr::read(r.descriptor) };
    let Some(status) = d.status else {
        unsafe {
            ptr::write(
                out,
                a::AudioBackendStatus {
                    active: 1,
                    ..a::AudioBackendStatus::default()
                },
            )
        };
        return 0;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    unsafe { status(d.context, out) }
}
unsafe fn output_record(raw: u64) -> Option<(Record, a::AudioBackend, a::AudioOutputExtension)> {
    let r = unsafe { record(raw as *mut c_void) }?;
    let d = unsafe { ptr::read(r.descriptor) };
    if !has_outputs(d.version) {
        return None;
    }
    let outputs = unsafe { ptr::read(ptr::addr_of!((*r.descriptor.cast::<Outputs>()).outputs)) };
    Some((r, d, outputs))
}
unsafe extern "C" fn output_query(raw: u64, index: u32, out: *mut a::AudioOutputInfo) -> i32 {
    let Some((r, d, o)) = (unsafe { output_record(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    unsafe { (o.query)(d.context as u64, index, out) }
}
unsafe extern "C" fn output_select(raw: u64, index: u32) -> i32 {
    let Some((r, d, o)) = (unsafe { output_record(raw) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    unsafe { (o.select)(d.context as u64, index) }
}
unsafe extern "C" fn output_active(raw: u64) -> i32 {
    let Some((_r, d, o)) = (unsafe { output_record(raw) }) else {
        return -1;
    };
    unsafe { (o.active)(d.context as u64) }
}
