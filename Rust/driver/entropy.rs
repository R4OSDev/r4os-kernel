//! One exact registered R4D source. No device protocol, pool or source policy.
//! DriverApi's sleepable owner serializes mutable state. Passive snapshots use
//! atomic words, so readers never race a plain-data seqlock or wait for DMA.
use super::{api_admission as admission, resource_state::Binding, resources};
use crate::{
    contract as c,
    platform::{monotonic, timer},
    sched::task_context::Admission,
};
use core::{
    cell::UnsafeCell,
    mem, ptr,
    sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
};

#[derive(Clone, Copy)]
struct Source {
    owner: u32,
    binding: Binding,
    backend: c::EntropyBackend,
    info: c::EntropySourceInfo,
    last_request: u64,
    busy: bool,
    detached: bool,
}
struct State {
    next: u64,
    source: Option<Source>,
}
struct Storage(UnsafeCell<State>);
// All mutation requires the exact globally serialized DriverApi owner. No
// reference into this storage is retained across a module callback or wait.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    next: 1,
    source: None,
}));
static LIVE_OWNER: AtomicU32 = AtomicU32::new(0);
static LIVE_SLOT: AtomicUsize = AtomicUsize::new(usize::MAX);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static CACHE: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];

fn publish(value: c::EntropySourceInfo) {
    let sequence = SEQUENCE.load(Ordering::SeqCst);
    assert!(
        sequence & 1 == 0 && sequence < u64::MAX - 1,
        "entropy snapshot sequence"
    );
    SEQUENCE.store(sequence + 1, Ordering::SeqCst);
    // Every byte is initialized; the fixed record has no hidden padding.
    let words: [u64; 16] = unsafe { mem::transmute(value) };
    for (word, value) in CACHE.iter().zip(words) {
        word.store(value, Ordering::SeqCst);
    }
    SEQUENCE.store(sequence + 2, Ordering::SeqCst);
}
pub(crate) fn has_owner(owner: u32) -> bool {
    owner != 0 && LIVE_OWNER.load(Ordering::Acquire) == owner
}
pub(crate) fn has_module(slot: usize) -> bool {
    LIVE_OWNER.load(Ordering::Acquire) != 0 && LIVE_SLOT.load(Ordering::Acquire) == slot
}
fn exact(source: Source, owner: u32, closing: bool) -> bool {
    source.owner == owner
        && resources::owned_binding(owner, closing).is_some_and(|b| {
            b.epoch == source.binding.epoch
                && b.slot == source.binding.slot
                && b.module_generation == source.binding.module_generation
        })
}
pub(crate) fn close(owner: u32) {
    let Some(mut source) = (unsafe { (*STATE.0.get()).source }) else {
        return;
    };
    if source.owner != owner {
        return;
    }
    assert!(
        exact(source, owner, true) && !source.busy,
        "entropy close owner"
    );
    if source.info.state == c::entropy_state_ready {
        source.info.state = c::entropy_state_closed;
    }
    unsafe {
        (*STATE.0.get()).source = Some(source);
    }
    publish(source.info);
}
pub(crate) fn prepare_close(owner: u32) -> bool {
    let Some(source) = (unsafe { (*STATE.0.get()).source }) else {
        return true;
    };
    source.owner != owner || exact(source, owner, true) && !source.busy
}
pub(crate) fn retained(owner: u32) {
    let Some(mut source) = (unsafe { (*STATE.0.get()).source }) else {
        return;
    };
    if source.owner != owner {
        return;
    }
    assert!(
        exact(source, owner, true) && !source.busy,
        "entropy retained owner"
    );
    source.info.state = c::entropy_state_retained;
    unsafe {
        (*STATE.0.get()).source = Some(source);
    }
    publish(source.info);
}
pub(crate) fn finish(owner: u32) -> bool {
    let Some(mut source) = (unsafe { (*STATE.0.get()).source }) else {
        return true;
    };
    if source.owner != owner {
        return true;
    }
    if !exact(source, owner, true) || source.busy || !source.detached {
        return false;
    }
    source.info.state = c::entropy_state_removed;
    source.info.flags &= !c::entropy_source_flag_registered;
    source.info.flags |= c::entropy_source_flag_quiesced;
    publish(source.info);
    unsafe {
        (*STATE.0.get()).source = None;
    }
    LIVE_OWNER.store(0, Ordering::Release);
    LIVE_SLOT.store(usize::MAX, Ordering::Release);
    true
}
pub(crate) fn ready_to_finish(owner: u32) -> bool {
    let Some(source) = (unsafe { (*STATE.0.get()).source }) else {
        return true;
    };
    source.owner != owner || exact(source, owner, true) && !source.busy && source.detached
}

/// # Safety
/// Exclusive aligned version/size output under the current R4D init owner.
pub(crate) unsafe extern "C" fn query(out: *mut c::DriverEntropyApi) -> i32 {
    let owner = admission::r4os_rust_driver_api_owner_scalar(2);
    if resources::owned_binding(owner, false).is_none() {
        return c::entropy_error_context;
    }
    if out.is_null()
        || !(out as usize).is_multiple_of(8)
        || unsafe { (*out).version != 1 || (*out).size < 32 }
    {
        return c::entropy_error_invalid;
    }
    unsafe {
        out.write(c::DriverEntropyApi {
            register_backend: register as *const () as usize as u64,
            unregister_backend: unregister as *const () as usize as u64,
            ..Default::default()
        });
    }
    0
}
unsafe extern "C" fn register(input: *const c::EntropyBackend, out: *mut u64) -> i32 {
    let owner = admission::r4os_rust_driver_api_owner_scalar(2);
    let Some(binding) = resources::owned_binding(owner, false) else {
        return c::entropy_error_context;
    };
    if input.is_null()
        || out.is_null()
        || !(input as usize).is_multiple_of(8)
        || !(out as usize).is_multiple_of(8)
    {
        return c::entropy_error_invalid;
    }
    if unsafe { (*input).version != 1 || (*input).size != 64 } {
        return c::entropy_error_invalid;
    }
    let backend = unsafe { input.read() };
    let allowed = c::entropy_source_flag_device | c::entropy_source_flag_virtual;
    if backend.read == 0
        || backend.max_request_bytes == 0
        || backend.max_request_bytes > c::entropy_max_request_bytes
        || backend.flags & !allowed != 0
        || backend.flags & c::entropy_source_flag_device == 0
        || backend.reserved0 != [0; 3]
    {
        return c::entropy_error_invalid;
    }
    if unsafe { (*STATE.0.get()).source.is_some() } {
        return c::entropy_error_busy;
    }
    let generation = unsafe { (*STATE.0.get()).next };
    if generation == u64::MAX {
        return c::entropy_error_retained;
    }
    let info = c::EntropySourceInfo {
        state: c::entropy_state_ready,
        flags: backend.flags | c::entropy_source_flag_registered,
        generation,
        owner_epoch: binding.epoch,
        owner,
        module_generation: binding.module_generation,
        pci_address: backend.pci_address,
        vendor_id: backend.vendor_id,
        device_id: backend.device_id,
        max_request_bytes: backend.max_request_bytes,
        ..Default::default()
    };
    unsafe {
        (*STATE.0.get()).next = generation + 1;
        (*STATE.0.get()).source = Some(Source {
            owner,
            binding,
            backend,
            info,
            last_request: 0,
            busy: false,
            detached: false,
        });
        out.write(generation);
    }
    LIVE_SLOT.store(binding.slot, Ordering::Release);
    LIVE_OWNER.store(owner, Ordering::Release);
    publish(info);
    0
}
unsafe extern "C" fn unregister(cookie: u64) -> i32 {
    let owner = admission::r4os_rust_driver_api_owner_scalar(2);
    let Some(mut source) = (unsafe { (*STATE.0.get()).source }) else {
        return c::entropy_error_stale;
    };
    if cookie == 0 || cookie != source.info.generation || !exact(source, owner, true) {
        return c::entropy_error_stale;
    }
    if source.busy {
        return c::entropy_error_busy;
    }
    source.detached = true;
    if source.info.state == c::entropy_state_ready {
        source.info.state = c::entropy_state_closed;
    }
    unsafe {
        (*STATE.0.get()).source = Some(source);
    }
    publish(source.info);
    0
}

/// # Safety
/// Exclusive writable aligned Contract record, valid through this short copy.
pub(crate) unsafe extern "C" fn info(out: *mut c::EntropySourceInfo) -> i32 {
    if out.is_null()
        || !(out as usize).is_multiple_of(8)
        || unsafe { (*out).version != 1 || (*out).size < 128 }
    {
        return c::entropy_error_invalid;
    }
    for _ in 0..8 {
        let sequence = SEQUENCE.load(Ordering::SeqCst);
        if sequence == 0 {
            unsafe {
                out.write(c::EntropySourceInfo::default());
            }
            return 0;
        }
        if sequence & 1 != 0 {
            continue;
        }
        let words = core::array::from_fn(|i| CACHE[i].load(Ordering::SeqCst));
        if sequence == SEQUENCE.load(Ordering::SeqCst) {
            unsafe {
                out.write(mem::transmute::<[u64; 16], c::EntropySourceInfo>(words));
            }
            return 0;
        }
    }
    c::entropy_error_busy
}
struct DriverOwner;
impl Drop for DriverOwner {
    fn drop(&mut self) {
        assert!(
            admission::r4os_rust_driver_api_leave(),
            "entropy driver owner"
        );
    }
}
fn context_allowed() -> bool {
    crate::platform::cpu_capabilities::flags() & 512 != 0
        && crate::memory::owner::fault_allowed()
        && super::irq_router::r4os_rust_irq_router_local(false) == 0
        && admission::r4os_rust_driver_api_storage_owner() == 0
        && admission::r4os_rust_driver_api_owner_scalar(2) == 0
        && admission::r4os_rust_driver_api_runtime_owner(true) == 0
}
/// # Safety
/// A writable trusted caller range of 1..4096 bytes is retained through the
/// complete wait and final copy. Failed validated ranges are cleared in full.
pub(crate) unsafe extern "C" fn fill(out: *mut u8, bytes: u32) -> i32 {
    if out.is_null()
        || bytes == 0
        || bytes > c::entropy_max_request_bytes
        || (out as usize).checked_add(bytes as usize).is_none()
    {
        return c::entropy_error_invalid;
    }
    if !context_allowed() {
        // No output admission yet. In particular, never fault a paged caller
        // range while an incompatible driver/IRQ/no-sleep scope owns execution.
        return c::entropy_error_context;
    }
    let Some(_task) = Admission::acquire(true) else {
        return c::entropy_error_context;
    };
    crate::memory::owner::prepare_stack();
    unsafe { staged_fill(out, bytes) }
}
#[inline(never)]
unsafe fn staged_fill(out: *mut u8, bytes: u32) -> i32 {
    // Initialized/touched before acquiring any driver owner; never a DMA target.
    let mut staging = [0u8; c::entropy_max_request_bytes as usize];
    let result = unsafe { read_source(staging.as_mut_ptr(), bytes) };
    // read_source has returned its driver owner before a possibly paged copy.
    let result = if result > 0 && crate::program::process::should_close() {
        c::entropy_error_cancelled
    } else {
        result
    };
    unsafe {
        if result == bytes as i32 {
            ptr::copy_nonoverlapping(staging.as_ptr(), out, bytes as usize);
        } else {
            ptr::write_bytes(out, 0, bytes as usize);
        }
        for byte in &mut staging {
            ptr::write_volatile(byte, 0);
        }
    }
    result
}
unsafe fn read_source(out: *mut u8, bytes: u32) -> i32 {
    let owner = LIVE_OWNER.load(Ordering::Acquire);
    if owner == 0 {
        return c::entropy_error_unavailable;
    }
    if !admission::r4os_rust_driver_api_enter(
        owner,
        u64::from(timer::frequency()).div_ceil(4).max(1),
    ) {
        return c::entropy_error_busy;
    }
    let _driver = DriverOwner;
    let Some(mut source) = (unsafe { (*STATE.0.get()).source }) else {
        return c::entropy_error_unavailable;
    };
    if !exact(source, owner, false) || source.detached {
        return c::entropy_error_stale;
    }
    if source.info.state != c::entropy_state_ready {
        return c::entropy_error_unavailable;
    }
    if source.busy {
        return c::entropy_error_busy;
    }
    if bytes > source.backend.max_request_bytes {
        return c::entropy_error_invalid;
    }
    let start = monotonic::capture();
    let Some(begin) = monotonic::resolve(start) else {
        return c::entropy_error_context;
    };
    if !matches!(start.kind, 1 | 2) || start.generation == 0 {
        return c::entropy_error_context;
    }
    source.busy = true;
    source.info.requests = source.info.requests.saturating_add(1);
    unsafe {
        (*STATE.0.get()).source = Some(source);
    }
    let mut receipt = c::EntropyDeviceReceipt {
        clock_source: u32::from(start.kind),
        clock_generation: start.generation,
        ..Default::default()
    };
    let callback: c::EntropyReadFn = unsafe { mem::transmute(source.backend.read as usize) };
    // Only copied scalars, raw pointers and the exact task/driver owners cross
    // the external call. The R4D owns every physical completion and stop.
    let returned = unsafe { callback(source.backend.context, out, bytes, &mut receipt) };
    let end = monotonic::capture();
    let timing = end.kind == start.kind
        && end.generation == start.generation
        && monotonic::resolve(end).is_some_and(|ns| {
            ns >= begin
                && ns - begin <= c::entropy_max_window_ns
                && receipt.begin_ns >= begin
                && receipt.end_ns <= ns
        })
        && receipt.clock_source == u32::from(start.kind)
        && receipt.clock_generation == start.generation;
    let completed = returned == bytes as i32
        && timing
        && super::entropy_policy::completed(&receipt, bytes, source.last_request)
        && exact(source, owner, false);
    let result = if completed || returned < 0 {
        returned
    } else {
        c::entropy_error_device
    };
    source.busy = false;
    source.last_request = source.last_request.max(receipt.request_generation);
    source.info.last_result = result;
    source.info.last_device_flags = receipt.flags;
    source.info.dma_submitted = source
        .info
        .dma_submitted
        .saturating_add(receipt.dma_submitted);
    source.info.dma_completed = source
        .info
        .dma_completed
        .saturating_add(receipt.dma_completed);
    source.info.short_completions = source
        .info
        .short_completions
        .saturating_add(receipt.short_completions);
    if completed {
        source.info.completed_requests = source.info.completed_requests.saturating_add(1);
        source.info.completed_bytes = source.info.completed_bytes.saturating_add(u64::from(bytes));
        source.info.flags |= c::entropy_source_flag_verified_receipt;
    } else {
        source.info.failed_requests = source.info.failed_requests.saturating_add(1);
        if receipt.flags & c::entropy_receipt_flag_retained != 0 {
            source.info.state = c::entropy_state_retained;
        } else if receipt.flags & c::entropy_receipt_flag_quiesced != 0 {
            source.info.state = c::entropy_state_closed;
            source.info.flags |= c::entropy_source_flag_quiesced;
        } else {
            // A negative return alone proves neither physical completion nor
            // quiescence. Keep its owner and refuse further device admission.
            source.info.state = c::entropy_state_retained;
        }
    }
    unsafe {
        (*STATE.0.get()).source = Some(source);
    }
    publish(source.info);
    result
}
