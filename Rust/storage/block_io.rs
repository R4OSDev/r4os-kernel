//! Original logical I/O splitting, bounce/direct ownership and batch drains.
//! Only resident raw pointers/snapshots cross the native execution owner;
//! no Rust buffer/registry reference or no-sleep guard crosses a wait/backend.
use crate::{
    memory::heap,
    sched::{
        scheduler,
        task_context::{Admission, Unwind},
    },
    storage::{
        access_state::Reference, block_accounting as accounting, block_registry, block_requests,
        block_workers, split,
    },
};
use core::{ffi::c_void, mem::MaybeUninit, ptr};
#[repr(C)]
#[derive(Clone, Copy)]
pub struct View {
    pub reference: Reference,
    pub sector_size: u32,
    pub sector_count: u64,
    pub max_sectors: u16,
    pub queue_depth: u16,
    pub timeout: u64,
    pub writable: bool,
    pub has_writer: bool,
    pub has_submit: bool,
    pub has_flush: bool,
    pub stats: *mut accounting::Stats,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Transfer {
    pub sectors_completed: u16,
    pub error: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Write {
    pub lba: u64,
    pub sectors: u16,
    pub data: *const u8,
    pub len: usize,
    pub attempted: bool,
    pub ok: bool,
}
#[repr(C)]
pub struct Input {
    pub kind: u8,
    pub lba: u64,
    pub sectors: u16,
    pub buffer: *mut u8,
    pub data: *const u8,
    pub len: usize,
    pub ownership: u8,
    pub(crate) observation: *const crate::storage::block_observation::Subscription,
}

struct Pin(block_registry::Pin);
impl Pin {
    fn acquire(index: usize) -> Option<Self> {
        let mut out = MaybeUninit::uninit();
        if !unsafe { block_registry::r4os_rust_block_registry_pin(index, out.as_mut_ptr()) } {
            return None;
        }
        Some(Self(unsafe { out.assume_init() }))
    }
    fn exact(reference: Reference) -> Option<Self> {
        let mut out = MaybeUninit::uninit();
        if !unsafe { block_registry::pin_exact(reference, out.as_mut_ptr()) } {
            return None;
        }
        Some(Self(unsafe { out.assume_init() }))
    }
    fn device(&self) -> *mut c_void {
        self.0.device
    }
    fn view(&self) -> View {
        let mut out = MaybeUninit::uninit();
        unsafe {
            block_io_view(self.device(), out.as_mut_ptr());
            out.assume_init()
        }
    }
}
impl Drop for Pin {
    fn drop(&mut self) {
        unsafe {
            block_registry::r4os_rust_block_registry_unpin(&mut self.0);
        }
    }
}
struct Bounce {
    pointer: *mut u8,
    len: usize,
    release: bool,
}
impl Drop for Bounce {
    fn drop(&mut self) {
        if self.release && !self.pointer.is_null() {
            unsafe {
                heap::r4os_rust_heap_free(self.pointer, self.len);
            }
        }
    }
}
fn stats(view: View, operation: u8, field: u8, value: u64) -> u64 {
    unsafe { accounting::r4os_rust_block_stats_change(view.stats, operation, field, value) }
}
fn last_error(view: View) -> u8 {
    let mut out = MaybeUninit::uninit();
    unsafe {
        accounting::r4os_rust_block_stats_snapshot(view.stats, out.as_mut_ptr());
        out.assume_init().last_error
    }
}
fn failure(view: View, kind: u8, error: u8) {
    stats(
        view,
        4,
        match kind {
            1 => 21,
            2 => 24,
            3 => 26,
            _ => unreachable!(),
        },
        1,
    );
    stats(view, 0, 42, u64::from(error));
    if error == 7 {
        stats(view, 4, 3, 1);
    }
}
fn timeout(view: View) -> u64 {
    if view.timeout == 0 {
        u64::MAX
    } else {
        view.timeout
    }
}
fn validate(view: View, lba: u64, sectors: u16, bytes: usize) -> u8 {
    block_requests::r4os_rust_block_validate(
        view.sector_size,
        view.sector_count,
        lba,
        sectors,
        bytes,
    )
}
// Bound synchronous copy/CPU work while respecting a smaller backend limit.
// A single unusually large sector remains indivisible; ordinary sector sizes
// fit this 64-KB boundary. DMA waits retain the existing worker/completion path.
fn synchronous_limit(view: View) -> u16 {
    // Preserve split_init's original rejection of an invalid zero sector size.
    if view.sector_size == 0 {
        return 0;
    }
    let bytes = (64 * 1024 / view.sector_size as usize).clamp(1, u16::MAX as usize) as u16;
    if view.max_sectors == 0 {
        bytes
    } else {
        bytes.min(view.max_sectors)
    }
}
fn ownership(worker: bool, trusted: bool) -> u8 {
    if !worker {
        0
    } else if trusted {
        2
    } else {
        1
    }
}
fn buffers(view: View, kind: u8, bytes: usize) {
    match kind {
        2 => {
            stats(view, 1, 29, 1);
            stats(view, 1, 30, bytes as u64);
            accounting::r4os_rust_block_runtime_change(1, 16, 1);
            accounting::r4os_rust_block_runtime_change(1, 17, bytes as u64);
        }
        1 => {
            stats(view, 1, 31, 1);
            stats(view, 1, 32, bytes as u64);
            accounting::r4os_rust_block_runtime_change(1, 18, 1);
            accounting::r4os_rust_block_runtime_change(1, 19, bytes as u64);
        }
        0 => {}
        _ => unreachable!(),
    }
}
fn copies(view: View, bytes: usize) {
    stats(view, 1, 33, bytes as u64);
    accounting::r4os_rust_block_runtime_change(1, 20, bytes as u64);
}
unsafe fn enqueue(device: *mut c_void, input: Input) -> Option<u64> {
    let mut id = 0;
    if unsafe { crate::storage::block_execution::r4os_rust_block_enqueue(device, &input, &mut id) }
    {
        Some(id)
    } else {
        None
    }
}
unsafe fn wait(device: *mut c_void, view: View, id: u64) -> block_requests::ResultRecord {
    let mut out = MaybeUninit::uninit();
    unsafe {
        crate::storage::block_execution::r4os_rust_block_wait(
            device,
            id,
            timeout(view),
            out.as_mut_ptr(),
        );
        out.assume_init()
    }
}
unsafe fn read_chunk(
    device: *mut c_void,
    view: View,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    trusted: bool,
) -> u8 {
    unsafe {
        read_chunk_observed(
            device,
            view,
            lba,
            sectors,
            out,
            trusted,
            ReadObservation::NONE,
        )
    }
}
struct ReadObservation {
    subscription: *const crate::storage::block_observation::Subscription,
    receipt: *mut crate::storage::block_observation::ReadReceipt,
}
impl ReadObservation {
    const NONE: Self = Self {
        subscription: ptr::null(),
        receipt: ptr::null_mut(),
    };
}
#[inline(always)]
unsafe fn read_chunk_observed(
    device: *mut c_void,
    view: View,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    trusted: bool,
    observation: ReadObservation,
) -> u8 {
    let bytes = usize::from(sectors) * view.sector_size as usize;
    let worker = block_workers::r4os_rust_block_workers_ready();
    let pointer = if worker && !trusted {
        heap::r4os_rust_heap_alloc(bytes, 16)
    } else {
        ptr::null_mut()
    };
    if worker && !trusted && pointer.is_null() {
        failure(view, 1, 1);
        return 1;
    }
    let mut bounce = Bounce {
        pointer,
        len: bytes,
        release: true,
    };
    let owner = ownership(worker, trusted);
    buffers(view, owner, bytes);
    let buffer = if pointer.is_null() { out } else { pointer };
    let Some(id) = (unsafe {
        enqueue(
            device,
            Input {
                kind: 1,
                lba,
                sectors,
                buffer,
                data: ptr::null(),
                len: bytes,
                ownership: owner,
                observation: observation.subscription,
            },
        )
    }) else {
        let error = last_error(view);
        failure(view, 1, error);
        return error;
    };
    unsafe {
        crate::storage::block_execution::r4os_rust_block_schedule(device, worker);
    }
    let result = if observation.receipt.is_null() {
        unsafe { wait(device, view, id) }
    } else {
        unsafe {
            crate::storage::block_execution::wait_receipted(
                device,
                id,
                timeout(view),
                observation.receipt,
            )
        }
    };
    if result.buffer_detached {
        bounce.release = false;
    }
    if !result.ok {
        return result.err;
    }
    if !pointer.is_null() {
        unsafe {
            ptr::copy_nonoverlapping(pointer, out, bytes);
        }
        copies(view, bytes);
    }
    0
}
unsafe fn write_chunk(
    device: *mut c_void,
    view: View,
    lba: u64,
    sectors: u16,
    data: *const u8,
    trusted: bool,
) -> u8 {
    let bytes = usize::from(sectors) * view.sector_size as usize;
    let worker = block_workers::r4os_rust_block_workers_ready();
    let pointer = if worker && !trusted {
        heap::r4os_rust_heap_alloc(bytes, 16)
    } else {
        ptr::null_mut()
    };
    if worker && !trusted && pointer.is_null() {
        failure(view, 2, 1);
        return 1;
    }
    let mut bounce = Bounce {
        pointer,
        len: bytes,
        release: true,
    };
    let owner = ownership(worker, trusted);
    buffers(view, owner, bytes);
    if !pointer.is_null() {
        unsafe {
            ptr::copy_nonoverlapping(data, pointer, bytes);
        }
        copies(view, bytes);
    }
    let buffer = if pointer.is_null() {
        data
    } else {
        pointer.cast_const()
    };
    let Some(id) = (unsafe {
        enqueue(
            device,
            Input {
                kind: 2,
                lba,
                sectors,
                buffer: ptr::null_mut(),
                data: buffer,
                len: bytes,
                ownership: owner,
                observation: ptr::null(),
            },
        )
    }) else {
        let error = last_error(view);
        failure(view, 2, error);
        return error;
    };
    unsafe {
        crate::storage::block_execution::r4os_rust_block_schedule(device, worker);
    }
    let result = unsafe { wait(device, view, id) };
    if result.buffer_detached {
        bounce.release = false;
    }
    if result.ok { 0 } else { result.err }
}
/// # Safety
/// Caller owns len writable bytes (or proves a resident direct lease) through
/// the synchronous call. Exact device pin/unwind lasts through every wait and
/// physical completion. No pageable caller pointer reaches the runtime worker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_read(
    index: usize,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
    trusted: bool,
) -> bool {
    unsafe { read_pinned(Pin::acquire(index), lba, sectors, out, len, trusted) }
}
pub(crate) unsafe fn read_exact(
    reference: Reference,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
    trusted: bool,
) -> bool {
    unsafe { read_pinned(Pin::exact(reference), lba, sectors, out, len, trusted) }
}
#[inline(always)]
unsafe fn read_pinned(
    pin: Option<Pin>,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
    trusted: bool,
) -> bool {
    let Some(_unwind) = Admission::acquire(false) else {
        return false;
    };
    let Some(pin) = pin else {
        return false;
    };
    let view = pin.view();
    let error = validate(view, lba, sectors, len);
    if error != 0 {
        failure(view, 1, error);
        return false;
    }
    let mut iterator = MaybeUninit::<split::Iterator>::uninit();
    if !unsafe {
        split::r4os_rust_storage_split_init(
            sectors,
            synchronous_limit(view),
            view.sector_size as usize,
            iterator.as_mut_ptr(),
        )
    } {
        failure(view, 1, 2);
        return false;
    }
    let mut iterator = unsafe { iterator.assume_init() };
    loop {
        let mut chunk = MaybeUninit::<split::Chunk>::uninit();
        if !unsafe { split::r4os_rust_storage_split_next(&mut iterator, chunk.as_mut_ptr()) } {
            break;
        }
        let chunk = unsafe { chunk.assume_init() };
        if unsafe {
            read_chunk(
                pin.device(),
                view,
                lba + u64::from(chunk.sector_offset),
                chunk.sectors,
                out.add(chunk.byte_offset),
                trusted,
            )
        } != 0
        {
            return false;
        }
        // Chunk resources and physical completion are fully acknowledged.
        scheduler::safe_reschedule();
    }
    true
}
/// # Safety
/// Caller owns len readable bytes through this call/direct physical drain;
/// output is exclusive C transfer storage and reports the same committed prefix.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_write(
    index: usize,
    lba: u64,
    sectors: u16,
    data: *const u8,
    len: usize,
    trusted: bool,
    out: *mut Transfer,
) {
    unsafe {
        out.write(Transfer::default());
    }
    let Some(_unwind) = Admission::acquire(false) else {
        unsafe {
            (*out).error = 1;
        }
        return;
    };
    let Some(pin) = Pin::acquire(index) else {
        unsafe {
            (*out).error = 2;
        }
        return;
    };
    let view = pin.view();
    if (!view.has_writer && !view.has_submit) || !view.writable {
        failure(view, 2, 6);
        unsafe {
            (*out).error = 6;
        }
        return;
    }
    let error = validate(view, lba, sectors, len);
    if error != 0 {
        failure(view, 2, error);
        unsafe {
            (*out).error = error;
        }
        return;
    }
    let mut iterator = MaybeUninit::<split::Iterator>::uninit();
    if !unsafe {
        split::r4os_rust_storage_split_init(
            sectors,
            synchronous_limit(view),
            view.sector_size as usize,
            iterator.as_mut_ptr(),
        )
    } {
        failure(view, 2, 2);
        unsafe {
            (*out).error = 2;
        }
        return;
    }
    let mut iterator = unsafe { iterator.assume_init() };
    loop {
        let mut chunk = MaybeUninit::<split::Chunk>::uninit();
        if !unsafe { split::r4os_rust_storage_split_next(&mut iterator, chunk.as_mut_ptr()) } {
            break;
        }
        let chunk = unsafe { chunk.assume_init() };
        let error = unsafe {
            write_chunk(
                pin.device(),
                view,
                lba + u64::from(chunk.sector_offset),
                chunk.sectors,
                data.add(chunk.byte_offset),
                trusted,
            )
        };
        if error != 0 {
            unsafe {
                (*out).error = error;
            }
            return;
        }
        unsafe {
            (*out).sectors_completed += chunk.sectors;
        }
        // Publish the durable prefix before permitting another task to run.
        scheduler::safe_reschedule();
    }
}
/// # Safety
/// Caller owns exactly count C jobs and every resident data lease through all
/// admitted sibling drains. Count above4 is rejected before buffer admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_block_write_batch(
    index: usize,
    expected: *const Reference,
    jobs: *mut Write,
    count: usize,
    parallel: bool,
) -> usize {
    for i in 0..count {
        unsafe {
            (*jobs.add(i)).ok = false;
            (*jobs.add(i)).attempted = false;
        }
    }
    if count == 0 || count > 4 {
        return 0;
    }
    let Some(_unwind) = Admission::acquire(false) else {
        return 0;
    };
    let expected = unsafe { expected.read() };
    if index != expected.slot as usize {
        return 0;
    }
    let Some(pin) = Pin::exact(expected) else {
        return 0;
    };
    let view = pin.view();
    if view.reference.slot != expected.slot
        || view.reference.generation != expected.generation
        || !view.writable
        || (!view.has_writer && !view.has_submit)
    {
        return 0;
    }
    let limit = if view.max_sectors == 0 {
        u16::MAX
    } else {
        view.max_sectors
    };
    for i in 0..count {
        let job = unsafe { jobs.add(i).read() };
        if job.sectors > limit || validate(view, job.lba, job.sectors, job.len) != 0 {
            return 0;
        }
        for j in 0..i {
            let before = unsafe { jobs.add(j).read() };
            if job.lba < before.lba + u64::from(before.sectors)
                && before.lba < job.lba + u64::from(job.sectors)
            {
                return 0;
            }
        }
    }
    let worker = block_workers::r4os_rust_block_workers_ready();
    let width = if parallel && worker && view.has_submit {
        usize::from(view.queue_depth).clamp(1, 4)
    } else {
        1
    };
    let mut pending_max = 0;
    let mut first = 0;
    while first < count {
        let mut ids = [0; 4];
        let mut admitted = 0;
        let wave_count = width.min(count - first);
        while admitted < wave_count {
            let job = unsafe { jobs.add(first + admitted).read() };
            let bytes = usize::from(job.sectors) * view.sector_size as usize;
            let Some(id) = (unsafe {
                enqueue(
                    pin.device(),
                    Input {
                        kind: 2,
                        lba: job.lba,
                        sectors: job.sectors,
                        buffer: ptr::null_mut(),
                        data: job.data,
                        len: bytes,
                        ownership: if worker { 2 } else { 0 },
                        observation: ptr::null(),
                    },
                )
            }) else {
                break;
            };
            ids[admitted] = id;
            unsafe {
                (*jobs.add(first + admitted)).attempted = true;
            }
            // The original batch counts its explicitly admitted direct jobs
            // even for inline boot execution; preserve that accounting.
            buffers(view, 2, bytes);
            unsafe {
                crate::storage::block_execution::r4os_rust_block_schedule(pin.device(), worker);
            }
            admitted += 1;
        }
        pending_max = pending_max.max(admitted);
        let mut all_ok = admitted == wave_count;
        for (offset, id) in ids[..admitted].iter().copied().enumerate() {
            let done = unsafe { wait(pin.device(), view, id) };
            unsafe {
                (*jobs.add(first + offset)).ok = done.ok;
            }
            all_ok &= done.ok;
        }
        if !all_ok {
            break;
        }
        first += admitted;
        // All admitted siblings in this wave have returned their real receipt.
        scheduler::safe_reschedule();
    }
    pending_max
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_block_flush(index: usize) -> bool {
    flush_pinned(Pin::acquire(index))
}
pub(crate) fn flush_exact(reference: Reference) -> bool {
    flush_pinned(Pin::exact(reference))
}
#[inline(always)]
fn flush_pinned(pin: Option<Pin>) -> bool {
    let Some(pin) = pin else {
        return false;
    };
    let view = pin.view();
    if !view.has_flush && !view.has_submit {
        return true;
    }
    let Some(id) = (unsafe {
        enqueue(
            pin.device(),
            Input {
                kind: 3,
                lba: 0,
                sectors: 0,
                buffer: ptr::null_mut(),
                data: ptr::null(),
                len: 0,
                ownership: 0,
                observation: ptr::null(),
            },
        )
    }) else {
        failure(view, 3, last_error(view));
        return false;
    };
    unsafe {
        crate::storage::block_execution::r4os_rust_block_schedule(
            pin.device(),
            block_workers::r4os_rust_block_workers_ready(),
        );
    }
    unsafe { wait(pin.device(), view, id) }.ok
}
const _: () = {
    assert!(core::mem::size_of::<Unwind>() == 16);
    assert!(core::mem::size_of::<Transfer>() == 4);
    assert!(core::mem::size_of::<Write>() == 40);
    assert!(core::mem::size_of::<View>() == 64);
    assert!(core::mem::size_of::<Input>() == 64);
};

unsafe fn block_io_view(device: *mut c_void, out: *mut View) {
    unsafe { out.write(crate::storage::block_device::io_view(device)) }
}

/// Exactly one resident read through the existing splitter/chunk/queue owner.
/// The subscription and receipt remain resident until real completion.
pub(crate) unsafe fn read_observed(
    reference: Reference,
    lba: u64,
    out: *mut u8,
    len: usize,
    observation: *const crate::storage::block_observation::Subscription,
    receipt: *mut crate::storage::block_observation::ReadReceipt,
) -> bool {
    let Some(pin) = Pin::exact(reference) else {
        return false;
    };
    let view = pin.view();
    if observation.is_null()
        || receipt.is_null()
        || len != view.sector_size as usize
        || validate(view, lba, 1, len) != 0
    {
        return false;
    }
    unsafe {
        read_chunk_observed(
            pin.device(),
            view,
            lba,
            1,
            out,
            true,
            ReadObservation {
                subscription: observation,
                receipt,
            },
        ) == 0
    }
}

/// One confirmed, bounded, resident speculative request through the original
/// device pin/queue/direct-buffer/physical-drain owner. Busy admission skips
/// without errors or fallback I/O. Normal queued/active controller work wins.
pub(crate) unsafe fn read_prefetch_exact(
    reference: Reference,
    lba: u64,
    sectors: u16,
    out: *mut u8,
    len: usize,
) -> Option<bool> {
    let _unwind = Admission::acquire(true)?;
    if !block_workers::r4os_rust_block_workers_ready() {
        return None;
    }
    let pin = Pin::exact(reference)?;
    let view = pin.view();
    if view.sector_size != 512
        || view.max_sectors < sectors
        || sectors == 0
        || sectors > 32
        || validate(view, lba, sectors, len) != 0
    {
        return None;
    }
    let bytes = usize::from(sectors) * 512;
    let owner = ownership(true, true);
    let id = unsafe {
        crate::storage::block_execution::enqueue_prefetch(
            pin.device(),
            Input {
                kind: 1,
                lba,
                sectors,
                buffer: out,
                data: ptr::null(),
                len: bytes,
                ownership: owner,
                observation: ptr::null(),
            },
        )
    }?;
    buffers(view, owner, bytes);
    unsafe {
        crate::storage::block_execution::r4os_rust_block_schedule(pin.device(), true);
    }
    let result = unsafe { wait(pin.device(), view, id) };
    assert!(
        !result.buffer_detached,
        "resident prefetch buffer physically drained"
    );
    unsafe {
        crate::storage::block_execution::finish_prefetch(pin.device(), id);
    }
    Some(result.ok)
}
