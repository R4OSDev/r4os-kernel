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
pub use crate::storage::block_execution::Retirement as Token;
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
    if unsafe { block_find(input.as_ptr(), input.len()) } {
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
        block_register(
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
    unsafe { block_registration_log(r.name.as_ptr(), r.name_len, index as usize, (*d).source) };
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
    if !unsafe { block_unregister(r.block_index) } {
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
    unsafe { block_recovery(r.block_index, finish, ok != 0) };
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
        if !unsafe { block_prepare(r.block_index, token.as_mut_ptr()) } {
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
        unsafe { block_token(t, false) };
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
            unsafe { block_token(token, false) };
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
                unsafe { block_token(token, false) };
                result.remaining = result.remaining.saturating_add(1);
                continue;
            }
        }
        if !unsafe { block_token(token, true) } {
            unsafe { block_token(token, false) };
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

unsafe fn block_unregister(index: usize) -> bool {
    unsafe { crate::storage::block_execution::r4os_rust_block_unregister(index) }
}

unsafe fn block_find(name: *const u8, len: usize) -> bool {
    (unsafe { crate::storage::block_registry::r4os_rust_block_registry_find(name, len) })
        != usize::MAX
}

unsafe fn block_prepare(index: usize, out: *mut Token) -> bool {
    unsafe { crate::storage::block_execution::r4os_rust_block_retirement_prepare(index, out) }
}

unsafe fn block_token(token: *mut Token, commit: bool) -> bool {
    unsafe {
        if commit {
            crate::storage::block_execution::r4os_rust_block_retirement_commit(token)
        } else {
            crate::storage::block_execution::r4os_rust_block_retirement_cancel(token)
        }
    }
}

unsafe fn block_recovery(index: usize, finish: bool, success: bool) {
    let _ = unsafe {
        crate::storage::block_execution::r4os_rust_block_device_operation(
            index,
            if finish {
                if success { 2 } else { 3 }
            } else {
                1
            },
            0,
            0,
            0,
            0,
        )
    };
}

unsafe fn block_register(
    name: *const u8,
    len: usize,
    owner: u32,
    d: *const a::StorageBackend,
    context: *mut c_void,
) -> isize {
    use crate::storage::{block_device as b, block_registry::Text};
    let asynchronous = unsafe { validation::r4os_rust_driver_storage_submit(d) }.is_some();
    let controller = unsafe { ptr::addr_of!((*d).controller).cast::<u8>() };
    let mut controller_len = 0;
    while controller_len < 32 && unsafe { controller.add(controller_len).read() } != 0 {
        controller_len += 1
    }
    let mut config = b::Config::new(Text { data: name, len }, block_read);
    config.parallel_callbacks = admission::callbacks_parallel();
    unsafe {
        config.driver = b::text(b"R4D");
        let bus = ptr::addr_of!((*d).bus).read();
        config.bus = if (1..=6).contains(&bus) { bus as u8 } else { 0 };
        config.controller = if controller_len == 0 {
            b::text(b"R4D")
        } else {
            Text {
                data: controller,
                len: controller_len,
            }
        };
        config.sector_size = ptr::addr_of!((*d).sector_size).read();
        config.sector_count = ptr::addr_of!((*d).sector_count).read();
        config.max_sectors = ptr::addr_of!((*d).max_sectors_per_request).read();
        config.queue_depth = if asynchronous {
            ptr::addr_of!((*d).queue_depth).read()
        } else {
            1
        };
        config.timeout = ptr::addr_of!((*d).timeout_ticks).read();
        let flags = ptr::addr_of!((*d).flags).read();
        config.removable = flags & a::storage_backend_flag_removable != 0;
        config.writable = flags & a::storage_backend_flag_writable != 0;
        let source = ptr::addr_of!((*d).source).read();
        config.source = match source {
            1 => 1,
            2 => 2,
            _ => 0,
        };
        config.owner = owner;
        config.context = context;
        config.write = if ptr::addr_of!((*d).write).read().is_some() {
            Some(block_write)
        } else {
            None
        };
        config.flush = if ptr::addr_of!((*d).flush).read().is_some() {
            Some(block_flush)
        } else {
            None
        };
        config.submit = if asynchronous {
            Some(block_submit)
        } else {
            None
        };
        config.cancel = if validation::r4os_rust_driver_storage_cancel(d).is_some() {
            Some(block_cancel)
        } else {
            None
        };
        config.reset = if validation::r4os_rust_driver_storage_reset(d).is_some() {
            Some(block_reset)
        } else {
            None
        };
    }
    unsafe { b::register(config) }.map_or(-1, |index| isize::try_from(index).expect("block slot"))
}

unsafe extern "C" fn block_read(
    ctx: *mut c_void,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
) -> bool {
    unsafe {
        r4os_rust_driver_storage_io(
            ctx,
            false,
            lba,
            sectors,
            out,
            u32::try_from(len).expect("block read bytes"),
        )
    }
}
unsafe extern "C" fn block_write(
    ctx: *mut c_void,
    lba: u64,
    sectors: u16,
    data: *const u8,
    len: usize,
) -> bool {
    unsafe {
        r4os_rust_driver_storage_io(
            ctx,
            true,
            lba,
            sectors,
            data.cast_mut(),
            u32::try_from(len).expect("block write bytes"),
        )
    }
}
unsafe extern "C" fn block_flush(ctx: *mut c_void) -> bool {
    unsafe { r4os_rust_driver_storage_flush(ctx) }
}
unsafe extern "C" fn block_submit(
    ctx: *mut c_void,
    input: *const crate::storage::block_device::AsyncRequest,
) -> i32 {
    let r = unsafe { input.read() };
    let buffer = match r.kind {
        1 => r.buffer as u64,
        2 => r.const_buffer as u64,
        0 | 3 => 0,
        _ => unreachable!(),
    };
    let public = a::StorageRequest {
        version: a::storage_request_version,
        size: core::mem::size_of::<a::StorageRequest>() as u32,
        flags: 0,
        handle: r.handle,
        operation: r.kind.into(),
        lba: r.lba,
        sectors: r.sectors.into(),
        buffer_bytes: u32::try_from(r.buffer_len).expect("block async bytes"),
        buffer_addr: buffer,
        complete: r.complete,
    };
    if r.observation.is_null() {
        return unsafe { r4os_rust_driver_storage_async(ctx, &public) };
    }
    let subscription = unsafe { &*r.observation };
    let mut observed = a::StorageObservedRequest {
        base: public,
        observation_version: a::storage_observation_version,
        reserved0: 0,
        cookie: subscription.cookie,
        observe: Some(subscription.observe),
    };
    observed.base.size = core::mem::size_of::<a::StorageObservedRequest>() as u32;
    observed.base.flags = a::storage_request_flag_observation;
    unsafe { r4os_rust_driver_storage_async(ctx, &observed.base) }
}
unsafe extern "C" fn block_cancel(ctx: *mut c_void, handle: u64, reason: u32) -> i32 {
    unsafe { r4os_rust_driver_storage_control(ctx, false, handle, reason) }
}
unsafe extern "C" fn block_reset(ctx: *mut c_void, reason: u32) -> i32 {
    unsafe { r4os_rust_driver_storage_control(ctx, true, 0, reason) }
}

unsafe fn block_registration_log(name: *const u8, len: usize, index: usize, source: u32) {
    use crate::boot::log as l;
    unsafe {
        l::text(b"[R4D] register storage backend ");
        l::text(core::slice::from_raw_parts(name, len));
        l::text(b" block=");
        l::r4os_rust_bootlog_dec(index as u64);
        l::text(b" source=");
        l::text(match source {
            1 => b"preload",
            2 => b"disk",
            _ => b"built-in",
        });
        l::text(b"\r\n");
    }
}
