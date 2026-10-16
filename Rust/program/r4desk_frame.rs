//! Sole remote-frame storage, consumer epochs, immutable leases and dirty
//! history. Frame copies/heap work retain only the original try-enter unwind
//! guard. Short program ownership covers metadata and retirement publication.
use super::remote_frame_state::{self as model, CaptureRegistry, History, Owner, Rect};
use crate::{
    contract as a,
    display::ownership::{self, Resident},
    memory::heap,
    platform::timer,
    sched::{
        sync_locks,
        sync_types::{Text, UnwindGuard, WaitQueue, WaitRequest},
        sync_wait,
    },
    service::desktop_events,
};
use core::{
    cell::UnsafeCell,
    ffi::c_void,
    mem::{MaybeUninit, size_of},
    ptr,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;
const WAIT_SLICE: u64 = 250 * crate::arch::pit::DEFAULT_HZ as u64 / 1000;
#[derive(Clone, Copy)]
struct Snapshot {
    memory: *mut u8,
    pixels: *mut u32,
    capacity_pixels: usize,
    revision: u32,
    width: u32,
    height: u32,
    epoch: u64,
}
impl Snapshot {
    const EMPTY: Self = Self {
        memory: ptr::null_mut(),
        pixels: ptr::null_mut(),
        capacity_pixels: 0,
        revision: 0,
        width: 0,
        height: 0,
        epoch: 0,
    };
}
struct State {
    memory: *mut u8,
    pixels: *mut u32,
    capacity_pixels: usize,
    info: a::RemoteFrameInfo,
    ready: bool,
    epoch: u64,
    discard_pending: bool,
    reap_pending: bool,
    registry: CaptureRegistry,
    source: Owner,
    leased: [Snapshot; model::SNAPSHOT_CAPACITY],
    published_bytes: u64,
    snapshot_bytes: u64,
    snapshot_misses: u64,
    revision_counter: u32,
    history: History,
    snapshots: [Snapshot; 2],
    snapshot_active: usize,
}
static STATE: Resident<MaybeUninit<State>> = Resident(UnsafeCell::new(MaybeUninit::uninit()));
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static CONSUMERS: AtomicU32 = AtomicU32::new(0);
static PUBLISHED_REVISION: AtomicU32 = AtomicU32::new(0);
static EXECUTION: Resident<UnwindGuard> =
    Resident(UnsafeCell::new(ownership::execution(b"remote-frame")));
static WAITERS: Resident<WaitQueue> = Resident(UnsafeCell::new(WaitQueue::EMPTY));
unsafe fn data() -> *mut State {
    STATE.0.get().cast()
}
// Original large resident state is initialized fieldwise under program_state,
// never materialized on a64KB task stack. It has exactly one native owner.
unsafe fn initialize() {
    if INITIALIZED.load(Ordering::Acquire) {
        return;
    }
    unsafe {
        let p = data();
        ptr::addr_of_mut!((*p).memory).write(ptr::null_mut());
        ptr::addr_of_mut!((*p).pixels).write(ptr::null_mut());
        ptr::addr_of_mut!((*p).capacity_pixels).write(0);
        ptr::addr_of_mut!((*p).info).write(a::RemoteFrameInfo::default());
        ptr::addr_of_mut!((*p).ready).write(false);
        ptr::addr_of_mut!((*p).epoch).write(1);
        ptr::addr_of_mut!((*p).discard_pending).write(false);
        ptr::addr_of_mut!((*p).reap_pending).write(false);
        // Registry/history contain only integer/boolean C records; all-zero
        // is their valid empty storage, except the nonzero first lease id.
        ptr::addr_of_mut!((*p).registry).write_bytes(0, 1);
        (*p).registry.next_lease = 1;
        ptr::addr_of_mut!((*p).source).write(Owner::default());
        for i in 0..model::SNAPSHOT_CAPACITY {
            ptr::addr_of_mut!((*p).leased)
                .cast::<Snapshot>()
                .add(i)
                .write(Snapshot::EMPTY)
        }
        ptr::addr_of_mut!((*p).published_bytes).write(0);
        ptr::addr_of_mut!((*p).snapshot_bytes).write(0);
        ptr::addr_of_mut!((*p).snapshot_misses).write(0);
        ptr::addr_of_mut!((*p).revision_counter).write(0);
        ptr::addr_of_mut!((*p).history).write_bytes(0, 1);
        for i in 0..2 {
            ptr::addr_of_mut!((*p).snapshots)
                .cast::<Snapshot>()
                .add(i)
                .write(Snapshot::EMPTY)
        }
        ptr::addr_of_mut!((*p).snapshot_active).write(1);
    }
    INITIALIZED.store(true, Ordering::Release);
}
fn state<R>(f: impl FnOnce(*mut State) -> R) -> R {
    let _state = ownership::enter_state();
    unsafe {
        initialize();
        f(data())
    }
}
fn wake() {
    let _ = unsafe {
        sync_wait::r4os_rust_sync_queue(WAITERS.0.get(), 1, ptr::null_mut(), 0, ptr::null_mut())
    };
}
fn signal() {
    desktop_events::r4os_rust_desktop_signal()
}
struct FrameExecution;
impl FrameExecution {
    fn enter() -> Option<Self> {
        if !unsafe { sync_locks::r4os_rust_sync_guard(EXECUTION.0.get(), 1, 0) } {
            return None;
        }
        discard_pending();
        Some(Self)
    }
    fn complete(self, epoch: u64, revision: u32) -> bool {
        let current = state(|p| unsafe {
            let current = epoch == (*p).epoch && consumers() != 0 && !(*p).discard_pending;
            if current {
                PUBLISHED_REVISION.store(revision, Ordering::Release)
            }
            current
        });
        drop(self);
        current
    }
}
impl Drop for FrameExecution {
    fn drop(&mut self) {
        loop {
            let state_owner = ownership::enter_state();
            let pending = unsafe {
                let p = data();
                (*p).discard_pending || (*p).reap_pending
            };
            if pending {
                drop(state_owner);
                discard_pending();
                continue;
            }
            // Guard release and final retirement check remain under the same
            // program owner. A later last consumer can then drain itself.
            let _ = unsafe { sync_locks::r4os_rust_sync_guard(EXECUTION.0.get(), 3, 0) };
            drop(state_owner);
            return;
        }
    }
}
fn discard_pending() {
    let (pending, reap) = state(|p| unsafe {
        let pending = (*p).discard_pending;
        let reap = (*p).reap_pending;
        (*p).discard_pending = false;
        (*p).reap_pending = false;
        (pending, reap)
    });
    if pending {
        discard_storage()
    }
    if reap {
        reap_snapshots()
    }
}
fn drain_if_available() {
    if let Some(execution) = FrameExecution::enter() {
        drop(execution)
    }
}
unsafe fn retire_locked(p: *mut State) {
    unsafe {
        (*p).epoch = (*p).epoch.wrapping_add(1);
        if (*p).epoch == 0 {
            (*p).epoch = 1
        }
        (*p).discard_pending = true;
        (*p).reap_pending = true;
        PUBLISHED_REVISION.store(0, Ordering::Release)
    }
}
pub fn consumers() -> u32 {
    CONSUMERS.load(Ordering::Acquire)
}
pub fn acquire(owner: Owner) -> i32 {
    let result = state(|p| unsafe {
        let current = consumers();
        let next = (*p)
            .registry
            .acquire(owner)
            .map_err(|_| a::remote_frame_error_invalid)?;
        CONSUMERS.store(next, Ordering::Release);
        Ok::<_, i32>((current, next))
    });
    match result {
        Ok((current, next)) => {
            if current == 0 {
                signal()
            }
            next as i32
        }
        Err(e) => e,
    }
}
pub fn release(owner: Owner) -> i32 {
    let result = state(|p| unsafe {
        let next = (*p)
            .registry
            .release(owner)
            .map_err(|_| a::remote_frame_error_invalid)?;
        CONSUMERS.store(next, Ordering::Release);
        if next == 0 {
            retire_locked(p)
        }
        Ok::<_, i32>(next)
    });
    match result {
        Ok(next) => {
            if next == 0 {
                drain_if_available();
                wake();
                signal()
            }
            next as i32
        }
        Err(e) => e,
    }
}
pub fn publisher(owner: Owner) -> bool {
    state(|p| unsafe {
        if (*p).source.id == 0 {
            (*p).source = owner
        }
        (*p).source == owner
    })
}
pub fn source_reset(owner: Owner) -> i32 {
    state(|p| unsafe {
        (*p).source = owner;
        retire_locked(p)
    });
    drain_if_available();
    wake();
    signal();
    0
}
pub fn stopped(owner: Owner) {
    state(|p| unsafe {
        let before = (*p).registry.count;
        (*p).registry.stopped(owner, timer::event_nanoseconds());
        CONSUMERS.store((*p).registry.count, Ordering::Release);
        let publisher = (*p).source == owner;
        if publisher {
            (*p).source = Owner::default()
        }
        if publisher || before != 0 && (*p).registry.count == 0 {
            retire_locked(p)
        }
        (*p).reap_pending = true;
    });
    drain_if_available();
    wake();
    signal()
}
fn current_epoch() -> Option<u64> {
    state(|p| unsafe {
        if consumers() == 0 || (*p).discard_pending {
            None
        } else {
            Some((*p).epoch)
        }
    })
}
pub fn revision() -> u32 {
    PUBLISHED_REVISION.load(Ordering::Acquire)
}
fn bump_revision() -> u32 {
    unsafe {
        let p = data();
        let next = (*p).revision_counter.wrapping_add(1);
        let next = if next == 0 { 1 } else { next };
        (*p).revision_counter = next;
        next
    }
}
unsafe fn free(memory: *mut u8, capacity: usize) {
    if !memory.is_null() {
        let _ = unsafe { heap::r4os_rust_heap_free(memory, capacity * 4) };
    }
}
fn ensure_capacity(pixel_count: usize) -> bool {
    if pixel_count == 0 || pixel_count > MAX_FRAME_BYTES / 4 {
        return false;
    }
    unsafe {
        let p = data();
        if (*p).capacity_pixels >= pixel_count && !(*p).pixels.is_null() {
            return true;
        }
        if pixel_count > usize::MAX / 4 {
            return false;
        }
        let memory = heap::r4os_rust_heap_alloc(pixel_count * 4, 4);
        if memory.is_null() {
            return false;
        }
        free((*p).memory, (*p).capacity_pixels);
        (*p).memory = memory;
        (*p).pixels = memory.cast();
        (*p).capacity_pixels = pixel_count;
        (*p).ready = false;
        (*p).info = a::RemoteFrameInfo::default();
        (*p).history.reset();
        PUBLISHED_REVISION.store(0, Ordering::Release);
        true
    }
}
fn discard_storage() {
    unsafe {
        let p = data();
        free((*p).memory, (*p).capacity_pixels);
        for i in 0..2 {
            let snapshot = ptr::addr_of_mut!((*p).snapshots).cast::<Snapshot>().add(i);
            if consumers() == 0 {
                free((*snapshot).memory, (*snapshot).capacity_pixels);
                snapshot.write(Snapshot::EMPTY)
            } else {
                (*snapshot).revision = 0;
                (*snapshot).width = 0;
                (*snapshot).height = 0
            }
        }
        (*p).memory = ptr::null_mut();
        (*p).pixels = ptr::null_mut();
        (*p).capacity_pixels = 0;
        (*p).info = a::RemoteFrameInfo::default();
        (*p).ready = false;
        (*p).history.reset();
        if consumers() == 0 {
            (*p).snapshot_active = 1
        }
        PUBLISHED_REVISION.store(0, Ordering::Release);
    }
    reap_snapshots()
}
unsafe fn ensure_snapshot_capacity(snapshot: *mut Snapshot, pixel_count: usize) -> bool {
    unsafe {
        if pixel_count == 0 {
            return false;
        }
        if (*snapshot).capacity_pixels >= pixel_count && !(*snapshot).pixels.is_null() {
            return true;
        }
        if pixel_count > MAX_FRAME_BYTES / 4 {
            return false;
        }
        let memory = heap::r4os_rust_heap_alloc(pixel_count * 4, 4);
        if memory.is_null() {
            return false;
        }
        free((*snapshot).memory, (*snapshot).capacity_pixels);
        snapshot.write(Snapshot {
            memory,
            pixels: memory.cast(),
            capacity_pixels: pixel_count,
            ..Snapshot::EMPTY
        });
        true
    }
}
fn reap_snapshots() {
    for i in 0..model::SNAPSHOT_CAPACITY {
        let snapshot = unsafe {
            ptr::addr_of_mut!((*data()).leased)
                .cast::<Snapshot>()
                .add(i)
        };
        let release = state(|p| unsafe {
            (*p).registry.references[i] == 0
                && ((*p).registry.count == 0 || (*snapshot).epoch != (*p).epoch)
        });
        if release {
            unsafe {
                free((*snapshot).memory, (*snapshot).capacity_pixels);
                snapshot.write(Snapshot::EMPTY)
            }
        }
    }
}
fn unavailable() -> i32 {
    state(|p| unsafe { (*p).snapshot_misses = (*p).snapshot_misses.saturating_add(1) });
    a::remote_frame_error_unavailable
}
fn normalize(info: &a::RemoteFrameInfo, force_full: bool) -> Rect {
    let full = Rect {
        x: 0,
        y: 0,
        w: info.width,
        h: info.height,
    };
    if force_full || info.dirty_w == 0 || info.dirty_h == 0 {
        return full;
    }
    let left = i64::from(info.dirty_x).max(0);
    let top = i64::from(info.dirty_y).max(0);
    let right = (i64::from(info.dirty_x) + i64::from(info.dirty_w)).min(i64::from(info.width));
    let bottom = (i64::from(info.dirty_y) + i64::from(info.dirty_h)).min(i64::from(info.height));
    if right <= left || bottom <= top {
        full
    } else {
        Rect {
            x: left as u32,
            y: top as u32,
            w: (right - left) as u32,
            h: (bottom - top) as u32,
        }
    }
}
fn merge(a: Rect, b: Rect) -> Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (u64::from(a.x) + u64::from(a.w)).max(u64::from(b.x) + u64::from(b.w));
    let bottom = (u64::from(a.y) + u64::from(a.h)).max(u64::from(b.y) + u64::from(b.h));
    Rect {
        x: left,
        y: top,
        w: u32::try_from(right - u64::from(left)).expect("bounded damage width"),
        h: u32::try_from(bottom - u64::from(top)).expect("bounded damage height"),
    }
}
unsafe fn copy_rect(
    dest: *mut u32,
    source: *const u32,
    width: u32,
    source_stride: u32,
    rect: Rect,
) {
    for row in 0..rect.h {
        let y = (rect.y + row) as usize;
        let dst_offset = y * width as usize + rect.x as usize;
        let src_offset = y * source_stride as usize + rect.x as usize;
        unsafe {
            ptr::copy_nonoverlapping(
                source.add(src_offset),
                dest.add(dst_offset),
                rect.w as usize,
            )
        }
    }
}
fn published_info(
    info: a::RemoteFrameInfo,
    total_pixels: usize,
    revision: u32,
    rect: Rect,
) -> a::RemoteFrameInfo {
    a::RemoteFrameInfo {
        magic: a::remote_frame_magic,
        version: a::remote_frame_version,
        flags: a::remote_frame_flag_ready
            | a::remote_frame_flag_dirty_valid
            | a::remote_frame_flag_cursor_valid,
        format: a::remote_frame_format_xrgb32,
        width: info.width,
        height: info.height,
        stride_pixels: info.width,
        bytes_per_pixel: 4,
        revision,
        frame_pixels: total_pixels as u32,
        frame_bytes: (total_pixels * 4) as u32,
        dirty_x: i32::try_from(rect.x).expect("damage x"),
        dirty_y: i32::try_from(rect.y).expect("damage y"),
        dirty_w: rect.w,
        dirty_h: rect.h,
        cursor_x: info.cursor_x,
        cursor_y: info.cursor_y,
        cursor_flags: info.cursor_flags,
        ..Default::default()
    }
}
/// # Safety
/// Caller keeps complete info and all declared source pixels live throughout
/// this synchronous copy. No source pointer is published or kept afterward.
pub unsafe fn publish(
    info: *const a::RemoteFrameInfo,
    pixels: *const u32,
    pixel_count: u32,
) -> i32 {
    if info.is_null() {
        return a::remote_frame_error_invalid;
    }
    let info = unsafe { info.read() };
    if info.format != a::remote_frame_format_xrgb32 || info.bytes_per_pixel != 4 {
        return a::remote_frame_error_unsupported;
    }
    if info.width == 0 || info.height == 0 || info.stride_pixels < info.width {
        return a::remote_frame_error_invalid;
    }
    let total = u64::from(info.width) * u64::from(info.height);
    if total == 0 || total > (MAX_FRAME_BYTES / 4) as u64 {
        return a::remote_frame_error_invalid;
    }
    let source_pixels = u64::from(info.stride_pixels) * u64::from(info.height);
    if source_pixels > u64::from(pixel_count) || pixels.is_null() {
        return a::remote_frame_error_invalid;
    }
    if consumers() == 0 {
        return 0;
    }
    let Some(execution) = FrameExecution::enter() else {
        return a::remote_frame_error_unavailable;
    };
    let Some(epoch) = current_epoch() else {
        return 0;
    };
    if !ensure_capacity(total as usize) {
        return a::remote_frame_error_oom;
    }
    let copied = unsafe {
        let p = data();
        let geometry_changed =
            !(*p).ready || (*p).info.width != info.width || (*p).info.height != info.height;
        let rect = normalize(&info, geometry_changed);
        copy_rect((*p).pixels, pixels, info.width, info.stride_pixels, rect);
        let revision = bump_revision();
        (*p).info = published_info(info, total as usize, revision, rect);
        (*p).ready = true;
        (*p).history.record(revision, rect);
        (*p).published_bytes = (*p)
            .published_bytes
            .saturating_add(u64::from(rect.w) * u64::from(rect.h) * 4);
        if !execution.complete(epoch, revision) {
            return 0;
        }
        wake();
        u64::from(rect.w) * u64::from(rect.h)
    };
    copied.min(i32::MAX as u64) as i32
}
/// # Safety
/// Full info, declared pixel and region arrays remain immutable through copy.
pub unsafe fn publish_regions(
    info: *const a::RemoteFrameInfo,
    pixels: *const u32,
    pixel_count: u32,
    regions: *const a::DisplayDamageRect,
    region_count: u32,
) -> i32 {
    if info.is_null() || pixels.is_null() || regions.is_null() {
        return a::remote_frame_error_invalid;
    }
    let info = unsafe { info.read() };
    if info.format != a::remote_frame_format_xrgb32 || info.bytes_per_pixel != 4 {
        return a::remote_frame_error_unsupported;
    }
    if info.width == 0
        || info.height == 0
        || info.stride_pixels < info.width
        || region_count == 0
        || region_count > a::display_damage_max_regions
    {
        return a::remote_frame_error_invalid;
    }
    let total = u64::from(info.width) * u64::from(info.height);
    let source_pixels = u64::from(info.stride_pixels) * u64::from(info.height);
    if total == 0 || total > (MAX_FRAME_BYTES / 4) as u64 || source_pixels > u64::from(pixel_count)
    {
        return a::remote_frame_error_invalid;
    }
    let mut normalized = [Rect::default(); a::display_damage_max_regions as usize];
    let mut copied = 0;
    let mut bounds = Rect::default();
    for (i, slot) in normalized
        .iter_mut()
        .enumerate()
        .take(region_count as usize)
    {
        let source = unsafe { regions.add(i).read() };
        if source.x < 0 || source.y < 0 || source.w == 0 || source.h == 0 {
            return a::remote_frame_error_out_of_range;
        }
        let x = source.x as u32;
        let y = source.y as u32;
        if x >= info.width
            || y >= info.height
            || source.w > info.width - x
            || source.h > info.height - y
        {
            return a::remote_frame_error_out_of_range;
        }
        let region = Rect {
            x,
            y,
            w: source.w,
            h: source.h,
        };
        *slot = region;
        copied += u64::from(region.w) * u64::from(region.h);
        bounds = if i == 0 {
            region
        } else {
            merge(bounds, region)
        };
    }
    if consumers() == 0 {
        return 0;
    }
    let Some(execution) = FrameExecution::enter() else {
        return a::remote_frame_error_unavailable;
    };
    let Some(epoch) = current_epoch() else {
        return 0;
    };
    if !ensure_capacity(total as usize) {
        return a::remote_frame_error_oom;
    }
    unsafe {
        let p = data();
        let geometry_changed =
            !(*p).ready || (*p).info.width != info.width || (*p).info.height != info.height;
        if geometry_changed {
            let full = Rect {
                w: info.width,
                h: info.height,
                ..Default::default()
            };
            copy_rect((*p).pixels, pixels, info.width, info.stride_pixels, full);
            bounds = full;
            copied = total
        } else {
            for region in &normalized[..region_count as usize] {
                copy_rect((*p).pixels, pixels, info.width, info.stride_pixels, *region)
            }
        }
        let revision = bump_revision();
        (*p).info = published_info(info, total as usize, revision, bounds);
        (*p).ready = true;
        (*p).history.record(revision, bounds);
        (*p).published_bytes = (*p).published_bytes.saturating_add(copied * 4);
        if !execution.complete(epoch, revision) {
            return 0;
        }
        wake();
    }
    copied.min(i32::MAX as u64) as i32
}
unsafe fn capture_info(since: Option<u32>, out: *mut a::RemoteFrameInfo) -> bool {
    let Some(_execution) = FrameExecution::enter() else {
        return false;
    };
    unsafe {
        let p = data();
        if !(*p).ready {
            return false;
        }
        let mut info = (*p).info;
        if let Some(revision) = since
            && revision != info.revision
        {
            let rect = (*p)
                .history
                .union_since(revision, info.revision, info.width, info.height);
            info.dirty_x = i32::try_from(rect.x).expect("dirty x");
            info.dirty_y = i32::try_from(rect.y).expect("dirty y");
            info.dirty_w = rect.w;
            info.dirty_h = rect.h;
        }
        out.write(info);
        true
    }
}
/// # Safety
/// Complete frame-info output remains writable through this bounded snapshot.
pub unsafe fn info(out: *mut a::RemoteFrameInfo) -> i32 {
    if out.is_null() {
        return a::remote_frame_error_invalid;
    }
    if !unsafe { capture_info(None, out) } {
        unsafe { out.write(a::RemoteFrameInfo::default()) };
        a::remote_frame_error_unavailable
    } else {
        0
    }
}
/// # Safety
/// Optional info and declared writable pixel count remain valid throughout copy.
pub unsafe fn read(
    offset: u32,
    out: *mut u32,
    pixel_count: u32,
    out_info: *mut a::RemoteFrameInfo,
) -> i32 {
    if pixel_count != 0 && out.is_null() {
        return a::remote_frame_error_invalid;
    }
    if !out_info.is_null() {
        unsafe { out_info.write(a::RemoteFrameInfo::default()) }
    }
    let Some(_execution) = FrameExecution::enter() else {
        return a::remote_frame_error_unavailable;
    };
    unsafe {
        let p = data();
        if !(*p).ready || (*p).pixels.is_null() {
            return a::remote_frame_error_unavailable;
        }
        let info = (*p).info;
        if !out_info.is_null() {
            out_info.write(info)
        }
        let frame_pixels = info.frame_pixels as usize;
        let offset = offset as usize;
        if offset > frame_pixels {
            return a::remote_frame_error_out_of_range;
        }
        if pixel_count == 0 || offset == frame_pixels {
            return 0;
        }
        let count = (pixel_count as usize).min(frame_pixels - offset);
        ptr::copy_nonoverlapping((*p).pixels.add(offset), out, count);
        count as i32
    }
}
unsafe extern "C" fn still_wait(raw: *mut c_void) -> bool {
    let last = unsafe { raw.cast::<u32>().read() };
    let current = revision();
    current == 0 || current == last
}
/// # Safety
/// Caller retains a complete output while its original wait predicate sleeps.
pub unsafe fn wait(last: u32, timeout: u64, out: *mut a::RemoteFrameInfo) -> i32 {
    if out.is_null() {
        return a::remote_frame_error_invalid;
    }
    if revision() == 0 {
        unsafe { out.write(a::RemoteFrameInfo::default()) };
        return a::remote_frame_error_unavailable;
    }
    let mut waited = 0;
    loop {
        let current = revision();
        if current != 0 && current != last {
            if unsafe { capture_info(Some(last), out) } {
                return 1;
            }
            continue;
        }
        if waited >= timeout {
            break;
        }
        let slice = WAIT_SLICE.max(1).min(timeout - waited);
        let mut context = last;
        let request = WaitRequest {
            timeout: slice,
            reason: Text::literal(b"remote-frame"),
            predicate: Some(still_wait),
            context: ptr::from_mut(&mut context).cast(),
            release: None,
            release_context: ptr::null_mut(),
            flags: 0,
            consume_runtime: false,
        };
        let _ = unsafe { sync_wait::r4os_rust_sync_wait(WAITERS.0.get(), &request) };
        waited += slice;
    }
    if unsafe { capture_info(None, out) } {
        0
    } else {
        unsafe { out.write(a::RemoteFrameInfo::default()) };
        a::remote_frame_error_unavailable
    }
}
/// # Safety
/// Output is a complete public mapping record. Original owner-thread ABI keeps
/// the emitted immutable slot live through the next map/release operation.
pub unsafe fn map(out: *mut a::RemoteFrameMapInfo) -> i32 {
    if out.is_null() {
        return a::remote_frame_error_invalid;
    }
    unsafe { out.write(a::RemoteFrameMapInfo::default()) };
    let Some(_execution) = FrameExecution::enter() else {
        return a::remote_frame_error_unavailable;
    };
    if consumers() != 1 {
        return a::remote_frame_error_unavailable;
    }
    unsafe {
        let p = data();
        if !(*p).ready || (*p).pixels.is_null() || (*p).info.frame_pixels == 0 {
            return a::remote_frame_error_unavailable;
        }
        let info = (*p).info;
        let required = info.frame_pixels as usize;
        let index = 1 - (*p).snapshot_active;
        let snapshot = ptr::addr_of_mut!((*p).snapshots)
            .cast::<Snapshot>()
            .add(index);
        if !ensure_snapshot_capacity(snapshot, required) {
            return a::remote_frame_error_oom;
        }
        let changed = (*snapshot).width != info.width || (*snapshot).height != info.height;
        let rect = if (*snapshot).revision == 0 || changed {
            Rect {
                w: info.width,
                h: info.height,
                ..Default::default()
            }
        } else {
            (*p).history
                .union_since((*snapshot).revision, info.revision, info.width, info.height)
        };
        if !rect.empty() {
            copy_rect(
                (*snapshot).pixels,
                (*p).pixels,
                info.width,
                info.width,
                rect,
            )
        }
        (*snapshot).revision = info.revision;
        (*snapshot).width = info.width;
        (*snapshot).height = info.height;
        (*p).snapshot_active = index;
        out.write(a::RemoteFrameMapInfo {
            pixels_addr: (*snapshot).pixels.addr() as u64,
            capacity_pixels: (*snapshot).capacity_pixels as u64,
            generation: u64::from(info.revision),
            ..Default::default()
        });
        0
    }
}
/// # Safety
/// Domain caller authenticated the exact capture owner and complete outputs;
/// resulting lease retains only its immutable snapshot until exact release.
pub unsafe fn snapshot_acquire(
    owner: Owner,
    expected: u32,
    out_info: *mut a::RemoteFrameInfo,
    out_lease: *mut a::RemoteFrameLease,
) -> i32 {
    unsafe {
        out_info.write(a::RemoteFrameInfo::default());
        out_lease.write(a::RemoteFrameLease::default())
    }
    let Some(_execution) = FrameExecution::enter() else {
        return unavailable();
    };
    let Some(epoch) = current_epoch() else {
        return unavailable();
    };
    unsafe {
        let p = data();
        if !(*p).ready || (*p).pixels.is_null() || expected != 0 && expected != (*p).info.revision {
            return unavailable();
        }
        let info = (*p).info;
        let selected = state(|p| {
            if !(*p).registry.has_consumer(owner) {
                return None;
            }
            let owned = (*p)
                .registry
                .leases
                .iter()
                .filter(|entry| entry.id != 0 && entry.owner == owner)
                .count();
            if owned >= model::LEASES_PER_CONSUMER {
                return None;
            }
            let selected = (*p)
                .leased
                .iter()
                .position(|snapshot| {
                    snapshot.epoch == epoch
                        && snapshot.revision == info.revision
                        && snapshot.width == info.width
                        && snapshot.height == info.height
                        && !snapshot.pixels.is_null()
                })
                .or_else(|| {
                    (*p).registry
                        .references
                        .iter()
                        .position(|&count| count == 0)
                })?;
            Some((selected, (*p).registry.references[selected] != 0))
        });
        let Some((selected, held)) = selected else {
            return unavailable();
        };
        let snapshot = ptr::addr_of_mut!((*p).leased)
            .cast::<Snapshot>()
            .add(selected);
        let required = info.frame_pixels as usize;
        if required == 0 || required > MAX_FRAME_BYTES / 4 {
            return a::remote_frame_error_invalid;
        }
        if (*snapshot).capacity_pixels < required || (*snapshot).pixels.is_null() {
            if held {
                return unavailable();
            }
            let memory = heap::r4os_rust_heap_alloc(required * 4, 4);
            if memory.is_null() {
                return a::remote_frame_error_oom;
            }
            free((*snapshot).memory, (*snapshot).capacity_pixels);
            snapshot.write(Snapshot {
                memory,
                pixels: memory.cast(),
                capacity_pixels: required,
                ..Snapshot::EMPTY
            });
        }
        if (*snapshot).epoch != epoch || (*snapshot).revision != info.revision {
            if held {
                return unavailable();
            }
            let rect = if (*snapshot).epoch != epoch
                || (*snapshot).width != info.width
                || (*snapshot).height != info.height
                || (*snapshot).revision == 0
            {
                Rect {
                    w: info.width,
                    h: info.height,
                    ..Default::default()
                }
            } else {
                (*p).history.union_since(
                    (*snapshot).revision,
                    info.revision,
                    info.width,
                    info.height,
                )
            };
            if !rect.empty() {
                copy_rect(
                    (*snapshot).pixels,
                    (*p).pixels,
                    info.width,
                    info.width,
                    rect,
                );
                (*p).snapshot_bytes = (*p)
                    .snapshot_bytes
                    .saturating_add(u64::from(rect.w) * u64::from(rect.h) * 4)
            }
            (*snapshot).epoch = epoch;
            (*snapshot).revision = info.revision;
            (*snapshot).width = info.width;
            (*snapshot).height = info.height;
        }
        let lease = state(|p| {
            if epoch != (*p).epoch || (*p).discard_pending {
                return None;
            }
            let now = timer::event_nanoseconds();
            let lease = (*p)
                .registry
                .begin_lease(owner, selected, epoch, now)
                .ok()?;
            Some((lease, now))
        });
        let Some((lease, now)) = lease else {
            return unavailable();
        };
        out_info.write(info);
        out_lease.write(a::RemoteFrameLease {
            id: lease.id,
            pixels_addr: (*snapshot).pixels.addr() as u64,
            capacity_pixels: required as u64,
            epoch,
            acquired_ns: now,
            ..Default::default()
        });
        0
    }
}
/// # Safety
/// Caller retains the full exact immutable lease record and authenticated owner.
pub unsafe fn snapshot_release(owner: Owner, lease: *const a::RemoteFrameLease) -> i32 {
    let lease = unsafe { lease.read() };
    if lease.version != 1
        || lease.size as usize != size_of::<a::RemoteFrameLease>()
        || lease.id == 0
        || lease.epoch == 0
    {
        return a::remote_frame_error_invalid;
    }
    let result = state(|p| unsafe {
        (*p).registry
            .end_lease(owner, lease.id, lease.epoch, timer::event_nanoseconds())
            .map_err(|_| a::remote_frame_error_invalid)?;
        (*p).reap_pending = true;
        Ok::<_, i32>(())
    });
    match result {
        Ok(()) => {
            drain_if_available();
            0
        }
        Err(e) => e,
    }
}
/// # Safety
/// Complete statistics output remains writable through the passive snapshot.
pub unsafe fn capture_stats(out: *mut a::RemoteFrameCaptureStats) -> i32 {
    if out.is_null() {
        return a::remote_frame_error_invalid;
    }
    unsafe { out.write(a::RemoteFrameCaptureStats::default()) };
    let Some(_execution) = FrameExecution::enter() else {
        return a::remote_frame_error_unavailable;
    };
    unsafe {
        let p = data();
        (*out).live_bytes = ((*p).capacity_pixels * 4) as u64;
        for snapshot in &(*p).leased {
            if !snapshot.memory.is_null() {
                (*out).snapshots += 1;
                (*out).snapshot_bytes += (snapshot.capacity_pixels * 4) as u64
            }
        }
        (*out).published_bytes = (*p).published_bytes;
        (*out).snapshot_copy_bytes = (*p).snapshot_bytes;
        state(|p| {
            (*out).consumers = (*p).registry.count;
            (*out).epoch = (*p).epoch;
            (*out).revision = u64::from(revision());
            (*out).acquires = (*p).registry.acquired;
            (*out).misses = (*p).snapshot_misses;
            (*out).max_reader_ns = (*p).registry.max_reader_ns;
            for count in (*p).registry.references {
                (*out).leases += count
            }
        });
        0
    }
}
