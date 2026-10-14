//! Original R4D storage backend registry, callback ledger and prepared cleanup.
//! The exact block token stays retained through real shutdown and unregister.
use crate::{
    boot::log,
    contract as a,
    driver::{api_admission as admission, api_validation as validation},
};
use core::{cell::UnsafeCell, ffi::c_void, mem::MaybeUninit, ptr};
#[derive(Clone, Copy)]
struct Record {
    used: bool,
    owner: u32,
    block_index: usize,
    name: [u8; 32],
    name_len: usize,
    descriptor: *const a::StorageBackend,
}
impl Record {
    const EMPTY: Self = Self {
        used: false,
        owner: 0,
        block_index: 0,
        name: [0; 32],
        name_len: 0,
        descriptor: ptr::null(),
    };
}
struct Storage(UnsafeCell<[Record; 8]>);
// SAFETY: original driver lifecycle and block retirement admission retain
// these stable slots. Only copied records span I/O or foreign callbacks.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new([Record::EMPTY; 8]));
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Token {
    pub index: usize,
    pub generation: u64,
    pub active: bool,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Entry {
    pub backend_index: usize,
    pub token: Token,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Plan {
    pub entries: [Entry; 8],
    pub count: usize,
}
#[repr(C)]
pub struct Cleanup {
    pub removed: u32,
    pub remaining: u32,
}
struct Callback(admission::CallbackToken);
impl Callback {
    fn enter(owner: u32) -> Option<Self> {
        let mut t = MaybeUninit::uninit();
        if unsafe { admission::r4os_rust_driver_api_storage_enter(owner, t.as_mut_ptr()) } {
            Some(Self(unsafe { t.assume_init() }))
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
unsafe extern "C" {
    fn r4os_zig_block_find(name: *const u8, len: usize) -> bool;
    fn r4os_zig_block_register(
        name: *const u8,
        len: usize,
        owner: u32,
        d: *const a::StorageBackend,
        context: *mut c_void,
    ) -> isize;
    fn r4os_zig_block_unregister(index: usize) -> bool;
    fn r4os_zig_block_recovery(index: usize, finish: bool, success: bool);
    fn r4os_zig_block_prepare(index: usize, out: *mut Token) -> bool;
    fn r4os_zig_block_token(token: *mut Token, commit: bool) -> bool;
    fn r4os_zig_block_registration_log(name: *const u8, len: usize, index: usize, source: u32);
}
fn puts(text: &[u8]) {
    unsafe { log::r4os_rust_bootlog_write(text.as_ptr(), text.len()) }
}
unsafe fn name<'a>(text: *const u8) -> &'a [u8] {
    let mut n = 0;
    while n < 512 && unsafe { *text.add(n) } != 0 {
        n += 1
    }
    unsafe { core::slice::from_raw_parts(text, n) }
}
unsafe fn equal(text: *const u8, r: Record) -> bool {
    for i in 0..r.name_len {
        if !unsafe { *text.add(i) }.eq_ignore_ascii_case(&r.name[i]) {
            return false;
        }
    }
    (unsafe { *text.add(r.name_len) }) == 0
}
fn record_at(i: usize) -> Record {
    unsafe { (*STATE.0.get())[i] }
}
unsafe fn record_raw(raw: *mut c_void) -> Option<Record> {
    if raw.is_null() {
        None
    } else {
        Some(unsafe { ptr::read(raw.cast()) })
    }
}
unsafe fn find(text: *const u8) -> Option<(usize, Record)> {
    (0..8).find_map(|i| {
        let r = record_at(i);
        (r.used && unsafe { equal(text, r) }).then_some((i, r))
    })
}
/// # Safety
/// Resident versioned descriptor and zero-terminated name remain valid under
/// driver lifecycle admission through confirmed block-device retirement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_register(
    text: *const u8,
    d: *const a::StorageBackend,
) -> i32 {
    if !unsafe { validation::r4os_rust_driver_storage_valid(d) } {
        return -1;
    }
    let input = unsafe { name(text) };
    if unsafe { r4os_zig_block_find(input.as_ptr(), input.len()) } {
        puts(b"[R4D][ERROR] storage backend duplicate name ");
        puts(input);
        puts(b"\r\n");
        return -2;
    }
    let Some(i) = (0..8).find(|&i| !record_at(i).used) else {
        puts(b"[R4D][ERROR] storage backend table full\r\n");
        return -3;
    };
    let mut r = Record::EMPTY;
    r.used = true;
    r.owner = admission::r4os_rust_driver_api_owner_scalar(0);
    r.descriptor = d;
    while r.name_len < 31 && unsafe { *text.add(r.name_len) } != 0 {
        r.name[r.name_len] = unsafe { *text.add(r.name_len) };
        r.name_len += 1;
    }
    let raw = unsafe { ptr::addr_of_mut!((*STATE.0.get())[i]) };
    unsafe { ptr::write(raw, r) };
    // Core registration receives stable name and callback context addresses.
    let index = unsafe {
        r4os_zig_block_register(
            ptr::addr_of!((*raw).name).cast(),
            r.name_len,
            r.owner,
            d,
            raw.cast(),
        )
    };
    if index < 0 {
        unsafe { ptr::write(raw, Record::EMPTY) };
        puts(b"[R4D][ERROR] storage block table full\r\n");
        return -4;
    }
    unsafe { (*raw).block_index = index as usize };
    unsafe {
        r4os_zig_block_registration_log(r.name.as_ptr(), r.name_len, index as usize, (*d).source)
    };
    i32::try_from(index).expect("Original bounded block index")
}
/// # Safety
/// Readable name and resident descriptor retain their existing backend identity
/// until block unregister succeeds; failed unregister keeps the exact record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_unregister(text: *const u8) -> i32 {
    let Some((i, r)) = (unsafe { find(text) }) else {
        return -1;
    };
    if !unsafe { r4os_zig_block_unregister(r.block_index) } {
        return -2;
    }
    if let Some(shutdown) = unsafe { (*r.descriptor).shutdown } {
        unsafe { shutdown((*r.descriptor).context) };
    }
    unsafe { (*STATE.0.get())[i] = Record::EMPTY };
    0
}
/// # Safety
/// The readable name identifies a retained backend under existing admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_recovery(
    text: *const u8,
    finish: bool,
    ok: i32,
) -> i32 {
    let Some((_, r)) = (unsafe { find(text) }) else {
        return -1;
    };
    unsafe { r4os_zig_block_recovery(r.block_index, finish, ok != 0) };
    0
}
/// # Safety
/// Output has the exact resident status C layout and no block/driver admission
/// may retire its descriptor while the original callback ledger is enrolled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_status(
    index: usize,
    out: *mut a::StorageBackendStatus,
) -> bool {
    unsafe { ptr::write(out, a::StorageBackendStatus::default()) };
    for i in 0..8 {
        let r = record_at(i);
        if !r.used || r.block_index != index {
            continue;
        }
        let Some(status) = (unsafe { (*r.descriptor).status }) else {
            return false;
        };
        let Some(_callback) = Callback::enter(r.owner) else {
            return false;
        };
        return unsafe { status((*r.descriptor).context, out) } == 0;
    }
    false
}
/// # Safety
/// The private resident plan is exclusive under driver lifecycle admission;
/// block tokens are retained until the exact commit or cancel acknowledgement.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_prepare(owner: u32, plan: *mut Plan) -> bool {
    unsafe { ptr::write(plan, Plan::default()) };
    for i in 0..8 {
        let r = record_at(i);
        if !r.used || r.owner != owner {
            continue;
        }
        let mut token = MaybeUninit::uninit();
        if !unsafe { r4os_zig_block_prepare(r.block_index, token.as_mut_ptr()) } {
            unsafe { r4os_rust_driver_storage_cancel_plan(plan) };
            return false;
        }
        let count = unsafe { (*plan).count };
        unsafe {
            (*plan).entries[count] = Entry {
                backend_index: i,
                token: token.assume_init(),
            };
            (*plan).count += 1;
        }
    }
    true
}
/// # Safety
/// The exact exclusive resident plan retains every prepared block token.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_cancel_plan(plan: *mut Plan) {
    while unsafe { (*plan).count } != 0 {
        unsafe { (*plan).count -= 1 };
        let i = unsafe { (*plan).count };
        let t = unsafe { ptr::addr_of_mut!((*plan).entries[i].token) };
        unsafe { r4os_zig_block_token(t, false) };
    }
}
/// # Safety
/// Plan and output are exclusive resident C records. No failed physical
/// finalizer or block unregister acknowledgement releases the retained slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_commit(plan: *mut Plan, out: *mut Cleanup) {
    let mut result = Cleanup {
        removed: 0,
        remaining: 0,
    };
    let count = unsafe { (*plan).count };
    for i in 0..count {
        let index = unsafe { (*plan).entries[i].backend_index };
        let r = record_at(index);
        let token = unsafe { ptr::addr_of_mut!((*plan).entries[i].token) };
        if !r.used {
            unsafe { r4os_zig_block_token(token, false) };
            result.remaining = result.remaining.saturating_add(1);
            continue;
        }
        if let Some(shutdown) = unsafe { (*r.descriptor).shutdown } {
            let code = unsafe { shutdown((*r.descriptor).context) };
            if code != 0 {
                puts(b"[R4D] storage finalizer failed owner=");
                log::r4os_rust_bootlog_dec(u64::from(r.owner));
                puts(b" block=");
                log::r4os_rust_bootlog_dec(r.block_index as u64);
                puts(b" code=");
                log::r4os_rust_bootlog_dec(i64::from(code).unsigned_abs());
                puts(b"\r\n");
                unsafe { r4os_zig_block_token(token, false) };
                result.remaining = result.remaining.saturating_add(1);
                continue;
            }
        }
        if !unsafe { r4os_zig_block_token(token, true) } {
            unsafe { r4os_zig_block_token(token, false) };
            result.remaining = result.remaining.saturating_add(1);
            continue;
        }
        unsafe { (*STATE.0.get())[index] = Record::EMPTY };
        result.removed = result.removed.saturating_add(1);
    }
    unsafe {
        (*plan).count = 0;
        ptr::write(out, result)
    };
}
/// # Safety
/// Stable callback context, resident I/O buffer and descriptor survive the
/// original block request and its callback ledger through completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_io(
    raw: *mut c_void,
    write: bool,
    lba: u64,
    sectors: u16,
    bytes: *mut u8,
    len: u32,
) -> bool {
    let Some(r) = (unsafe { record_raw(raw) }) else {
        return false;
    };
    if write {
        let Some(f) = (unsafe { (*r.descriptor).write }) else {
            return false;
        };
        let Some(_callback) = Callback::enter(r.owner) else {
            return false;
        };
        (unsafe { f((*r.descriptor).context, lba, u32::from(sectors), bytes, len) }) == 0
    } else {
        let Some(f) = (unsafe { (*r.descriptor).read }) else {
            return false;
        };
        let Some(_callback) = Callback::enter(r.owner) else {
            return false;
        };
        (unsafe { f((*r.descriptor).context, lba, u32::from(sectors), bytes, len) }) == 0
    }
}
/// # Safety
/// The stable block callback context retains its descriptor through completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_flush(raw: *mut c_void) -> bool {
    let Some(r) = (unsafe { record_raw(raw) }) else {
        return false;
    };
    let Some(f) = (unsafe { (*r.descriptor).flush }) else {
        return true;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return false;
    };
    (unsafe { f((*r.descriptor).context) }) == 0
}
/// # Safety
/// Resident request, exact completion callback and descriptor survive actual
/// asynchronous submission; the block owner retains buffers until device ack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_async(
    raw: *mut c_void,
    request: *const a::StorageRequest,
) -> i32 {
    let Some(r) = (unsafe { record_raw(raw) }) else {
        return -1;
    };
    let Some(f) = (unsafe { validation::r4os_rust_driver_storage_submit(r.descriptor) }) else {
        return -1;
    };
    let Some(_callback) = Callback::enter(r.owner) else {
        return -1;
    };
    unsafe { f((*r.descriptor).context, request) }
}
/// # Safety
/// Retained callback context survives cancel/reset and their device acknowledgements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_driver_storage_control(
    raw: *mut c_void,
    reset: bool,
    handle: u64,
    reason: u32,
) -> i32 {
    let Some(r) = (unsafe { record_raw(raw) }) else {
        return -1;
    };
    if reset {
        let Some(f) = (unsafe { validation::r4os_rust_driver_storage_reset(r.descriptor) }) else {
            return -1;
        };
        let Some(_callback) = Callback::enter(r.owner) else {
            return -1;
        };
        unsafe { f((*r.descriptor).context, reason) }
    } else {
        let Some(f) = (unsafe { validation::r4os_rust_driver_storage_cancel(r.descriptor) }) else {
            return -1;
        };
        let Some(_callback) = Callback::enter(r.owner) else {
            return -1;
        };
        unsafe { f((*r.descriptor).context, handle, reason) }
    }
}
