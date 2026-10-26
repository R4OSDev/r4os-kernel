//! Original bounded cache capacity, FIFO/LRU index and disabled read-ahead
//! model. Caller holds the existing cache owner; no hardware or waits here.
use core::{
    cell::UnsafeCell,
    mem::{offset_of, size_of},
    ptr,
};

pub const MAX_ENTRIES: usize = 8192;
/// Bulk reads keep at most one bounded fill window in the cold LRU prefix.
pub const STREAM_SECTORS: u32 = 32;
pub const TRIM_PAGES: usize = 16;
pub const MAX_DEVICES: usize = 8;
pub const NO_INDEX: u16 = 0xffff;
pub const NO_DEVICE: u8 = 0xff;
/// At most64KB of resident staging per synchronous fill operation.
pub const MAX_FILL_PAGES: u16 = 16;
const DIRTY_HIGH_PAGES: u16 = 64;
const DIRTY_LOW_PAGES: u16 = 32;

#[repr(C)]
pub struct Capacity {
    pub ram_pages: u16,
    pub active_pages: u16,
    pub pressure_level: u8,
    pub read_ahead_pages: u16,
}

pub fn capacity_for_memory(reference_free_frames: u64, current_free_frames: u64) -> Capacity {
    let ram_pages = (reference_free_frames / 128).clamp(64, MAX_ENTRIES as u64) as u16;
    let (active_pages, pressure_level) = if current_free_frames <= 1024 {
        (ram_pages.min(64), 3)
    } else if current_free_frames <= 4096 {
        (ram_pages.min(128), 2)
    } else if current_free_frames <= 8192 {
        (ram_pages.min(256), 1)
    } else {
        (ram_pages, 0)
    };
    let unconstrained_window = (active_pages / 32).clamp(1, 16);
    let read_ahead_pages = match pressure_level {
        3 => 0,
        2 => 1,
        1 => unconstrained_window.min(4),
        _ => unconstrained_window,
    };
    Capacity {
        ram_pages,
        active_pages,
        pressure_level,
        read_ahead_pages,
    }
}

pub fn fill_run_page_limit(
    requested: u16,
    max_sectors: u16,
    page_sectors: u16,
    capacity: u16,
) -> u16 {
    if requested == 0 || page_sectors == 0 || capacity == 0 {
        return 0;
    }
    let device = if max_sectors == 0 {
        requested
    } else {
        (max_sectors / page_sectors).max(1)
    };
    requested.min(device.min(capacity))
}

#[repr(C)]
pub struct SequentialDecision {
    pub next_page: u64,
    pub pages: u16,
    pub random_reset: bool,
}
#[repr(C)]
pub struct Sequential {
    pub have_last: bool,
    pub last_end_page: u64,
    pub window_pages: u16,
}
impl Sequential {
    fn observe(&mut self, first: u64, end: u64, max_window: u16) -> SequentialDecision {
        if end <= first {
            return SequentialDecision {
                next_page: 0,
                pages: 0,
                random_reset: false,
            };
        }
        let random_reset = self.have_last && first != self.last_end_page;
        let sequential = self.have_last && first == self.last_end_page;
        if max_window == 0 {
            self.window_pages = 0;
        } else if sequential {
            let grown = if self.window_pages == 0 {
                1
            } else {
                self.window_pages.saturating_mul(2)
            };
            self.window_pages = max_window.min(grown);
        } else {
            self.window_pages = 0;
        }
        self.have_last = true;
        self.last_end_page = end;
        SequentialDecision {
            next_page: end,
            pages: self.window_pages,
            random_reset,
        }
    }
    fn reset(&mut self) {
        self.have_last = false;
        self.last_end_page = 0;
        self.window_pages = 0;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Queue {
    Detached,
    Free,
    Clean,
    Dirty,
    BusyClean,
    BusyDirty,
}
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Link {
    pub prev: u16,
    pub next: u16,
    pub device: u8,
    pub queue: Queue,
    pub cold: bool,
}
impl Link {
    const EMPTY: Self = Self {
        prev: NO_INDEX,
        next: NO_INDEX,
        device: NO_DEVICE,
        queue: Queue::Detached,
        cold: false,
    };
}
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Device {
    pub clean_head: u16,
    pub clean_tail: u16,
    pub dirty_head: u16,
    pub dirty_tail: u16,
    pub entries: u16,
    pub clean: u16,
    pub dirty: u16,
    pub busy_dirty: u16,
    pub dirty_high_water: u16,
    pub pressure_active: bool,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct ReadAheadRequest {
    pub page: u64,
    pub pages: u16,
    pub generation: u64,
}
#[repr(C)]
pub struct ReadAhead {
    pub generation: u64,
    pub pending: bool,
    pub pending_page: u64,
    pub pending_pages: u16,
    pub inflight: bool,
    pub inflight_page: u64,
    pub inflight_pages: u16,
    pub resident_pages: u16,
    pub sequential: Sequential,
}
impl ReadAhead {
    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.generation = 1;
        }
    }
    fn schedule(&mut self, page: u64, pages: u16) -> bool {
        if pages == 0 {
            return false;
        }
        if (self.pending && self.pending_page == page && self.pending_pages == pages)
            || (self.inflight && self.inflight_page == page && self.inflight_pages == pages)
        {
            return false;
        }
        let cancelled = self.pending
            || (self.inflight && (self.inflight_page != page || self.inflight_pages != pages));
        if cancelled {
            self.bump_generation();
        }
        self.pending = true;
        self.pending_page = page;
        self.pending_pages = pages;
        cancelled
    }
    fn demand(&mut self, page: u64) -> bool {
        let mut cancelled = false;
        if self.pending {
            self.pending = false;
            self.pending_pages = 0;
            cancelled = true;
        }
        if self.inflight && !Self::contains_page(self.inflight_page, self.inflight_pages, page) {
            cancelled = true;
            self.bump_generation();
        }
        cancelled
    }
    fn cancel_all(&mut self) -> bool {
        let cancelled = self.pending || self.inflight;
        self.pending = false;
        self.pending_pages = 0;
        if self.inflight {
            self.bump_generation();
        }
        self.sequential.reset();
        cancelled
    }
    fn begin(&mut self) -> Option<ReadAheadRequest> {
        if !self.pending || self.inflight {
            return None;
        }
        self.pending = false;
        self.inflight = true;
        self.inflight_page = self.pending_page;
        self.inflight_pages = self.pending_pages;
        self.pending_pages = 0;
        Some(ReadAheadRequest {
            page: self.inflight_page,
            pages: self.inflight_pages,
            generation: self.generation,
        })
    }
    fn complete(&mut self, request: ReadAheadRequest, success: bool) -> bool {
        let current = self.inflight
            && self.inflight_page == request.page
            && self.inflight_pages == request.pages
            && self.generation == request.generation;
        if self.inflight && self.inflight_page == request.page {
            self.inflight = false;
            self.inflight_pages = 0;
        }
        current && success
    }
    fn consume_resident(&mut self) {
        if self.resident_pages != 0 {
            self.resident_pages -= 1;
        }
    }
    fn contains_page(first: u64, pages: u16, page: u64) -> bool {
        pages != 0 && page >= first && page - first < u64::from(pages)
    }
}

#[repr(C)]
pub struct Index {
    pub links: [Link; MAX_ENTRIES],
    pub devices: [Device; MAX_DEVICES],
    pub free_head: u16,
    pub free_tail: u16,
    pub free_count: u16,
    pub busy_count: u16,
    pub clean_device_cursor: u8,
    pub dirty_device_cursor: u8,
    pub clean_device_probes: u64,
    pub dirty_device_probes: u64,
}
impl Index {
    const fn initial_state() -> Self {
        let mut state = Self {
            links: [Link::EMPTY; MAX_ENTRIES],
            devices: [Device {
                clean_head: NO_INDEX,
                clean_tail: NO_INDEX,
                dirty_head: NO_INDEX,
                dirty_tail: NO_INDEX,
                entries: 0,
                clean: 0,
                dirty: 0,
                busy_dirty: 0,
                dirty_high_water: 0,
                pressure_active: false,
            }; MAX_DEVICES],
            free_head: 0,
            free_tail: (MAX_ENTRIES - 1) as u16,
            free_count: MAX_ENTRIES as u16,
            busy_count: 0,
            clean_device_cursor: 0,
            dirty_device_cursor: 0,
            clean_device_probes: 0,
            dirty_device_probes: 0,
        };
        let mut index = 0;
        while index < MAX_ENTRIES {
            state.links[index] = Link {
                prev: if index == 0 {
                    NO_INDEX
                } else {
                    (index - 1) as u16
                },
                next: if index + 1 == MAX_ENTRIES {
                    NO_INDEX
                } else {
                    (index + 1) as u16
                },
                device: NO_DEVICE,
                queue: Queue::Free,
                cold: false,
            };
            index += 1;
        }
        state
    }
    // SAFETY: The exact caller output is exclusive, aligned and fully resident,
    // disjoint from the immutable native constructor template. Copy in place
    // without materializing the whole index on a bounded task stack.
    unsafe fn initialize(output: *mut Self) {
        unsafe { ptr::copy_nonoverlapping(ptr::addr_of!(INDEX_TEMPLATE), output, 1) };
    }
    fn claim_free(&mut self) -> Option<usize> {
        let raw = self.free_head;
        if raw == NO_INDEX {
            return None;
        }
        let index = usize::from(raw);
        let next = self.links[index].next;
        self.free_head = next;
        if next == NO_INDEX {
            self.free_tail = NO_INDEX;
        } else {
            self.links[usize::from(next)].prev = NO_INDEX;
        }
        self.free_count -= 1;
        self.links[index] = Link::EMPTY;
        Some(index)
    }
    fn attach_clean(&mut self, index: usize, device: usize) -> bool {
        self.attach_class(index, device, false)
    }
    pub(super) fn attach_class(&mut self, index: usize, device: usize, cold: bool) -> bool {
        if index >= MAX_ENTRIES || device >= MAX_DEVICES {
            return false;
        }
        if self.links[index].queue != Queue::Detached {
            return false;
        }
        self.links[index].device = device as u8;
        self.links[index].queue = Queue::Clean;
        self.links[index].cold = cold;
        if cold {
            self.link_clean_head(index, device);
        } else {
            self.link_clean_tail(index, device);
        }
        self.devices[device].entries += 1;
        self.devices[device].clean += 1;
        true
    }
    fn touch_clean(&mut self, index: usize) {
        if index >= MAX_ENTRIES || self.links[index].queue != Queue::Clean {
            return;
        }
        let device = usize::from(self.links[index].device);
        self.links[index].cold = false;
        if self.devices[device].clean_tail == index as u16 {
            return;
        }
        self.unlink_clean(index, device);
        self.link_clean_tail(index, device);
    }
    fn mark_dirty(&mut self, index: usize) -> bool {
        if index >= MAX_ENTRIES {
            return false;
        }
        let queue = self.links[index].queue;
        if queue == Queue::Dirty || queue == Queue::BusyDirty {
            return true;
        }
        if queue != Queue::Clean {
            return false;
        }
        let device = usize::from(self.links[index].device);
        self.unlink_clean(index, device);
        self.devices[device].clean -= 1;
        self.devices[device].dirty += 1;
        self.devices[device].dirty_high_water = self.devices[device]
            .dirty_high_water
            .max(self.devices[device].dirty);
        if self.devices[device].dirty >= DIRTY_HIGH_PAGES {
            self.devices[device].pressure_active = true;
        }
        self.links[index].queue = Queue::Dirty;
        self.links[index].cold = false;
        self.link_dirty_tail(index, device);
        true
    }
    fn clear_dirty(&mut self, index: usize) -> bool {
        if index >= MAX_ENTRIES {
            return false;
        }
        let queue = self.links[index].queue;
        if queue != Queue::Dirty && queue != Queue::BusyDirty {
            return queue == Queue::Clean || queue == Queue::BusyClean;
        }
        let device = usize::from(self.links[index].device);
        if queue == Queue::Dirty {
            self.unlink_dirty(index, device);
        }
        if queue == Queue::BusyDirty {
            self.devices[device].busy_dirty -= 1;
        }
        self.devices[device].dirty -= 1;
        self.devices[device].clean += 1;
        if self.devices[device].dirty <= DIRTY_LOW_PAGES {
            self.devices[device].pressure_active = false;
        }
        if queue == Queue::Dirty {
            self.links[index].queue = Queue::Clean;
            self.link_clean_tail(index, device);
        } else {
            self.links[index].queue = Queue::BusyClean;
        }
        true
    }
    fn pin(&mut self, index: usize) -> bool {
        if index >= MAX_ENTRIES {
            return false;
        }
        let device = usize::from(self.links[index].device);
        match self.links[index].queue {
            Queue::Clean => {
                self.unlink_clean(index, device);
                self.links[index].queue = Queue::BusyClean;
            }
            Queue::Dirty => {
                self.unlink_dirty(index, device);
                self.links[index].queue = Queue::BusyDirty;
                self.devices[device].busy_dirty += 1;
            }
            _ => return false,
        }
        self.busy_count += 1;
        true
    }
    fn unpin(&mut self, index: usize, retry_oldest: bool) -> bool {
        if index >= MAX_ENTRIES {
            return false;
        }
        let device = usize::from(self.links[index].device);
        match self.links[index].queue {
            Queue::BusyClean => {
                self.links[index].queue = Queue::Clean;
                if self.links[index].cold {
                    self.link_clean_head(index, device);
                } else {
                    self.link_clean_tail(index, device);
                }
            }
            Queue::BusyDirty => {
                self.devices[device].busy_dirty -= 1;
                self.links[index].queue = Queue::Dirty;
                if retry_oldest {
                    self.link_dirty_head(index, device);
                } else {
                    self.link_dirty_tail(index, device);
                }
            }
            _ => return false,
        }
        self.busy_count -= 1;
        true
    }
}

impl Index {
    fn detach(&mut self, index: usize) -> bool {
        if index >= MAX_ENTRIES {
            return false;
        }
        let device = usize::from(self.links[index].device);
        match self.links[index].queue {
            Queue::Clean => {
                self.unlink_clean(index, device);
                self.devices[device].clean -= 1;
            }
            Queue::Dirty => {
                self.unlink_dirty(index, device);
                self.devices[device].dirty -= 1;
                if self.devices[device].dirty <= DIRTY_LOW_PAGES {
                    self.devices[device].pressure_active = false;
                }
            }
            Queue::Detached => return true,
            _ => return false,
        }
        self.devices[device].entries -= 1;
        self.links[index] = Link::EMPTY;
        true
    }
    fn release(&mut self, index: usize) -> bool {
        if index >= MAX_ENTRIES {
            return false;
        }
        if self.links[index].queue != Queue::Detached && !self.detach(index) {
            return false;
        }
        self.links[index] = Link {
            prev: self.free_tail,
            queue: Queue::Free,
            ..Link::EMPTY
        };
        if self.free_tail == NO_INDEX {
            self.free_head = index as u16;
        } else {
            self.links[usize::from(self.free_tail)].next = index as u16;
        }
        self.free_tail = index as u16;
        self.free_count += 1;
        true
    }
    fn clean_victim(&mut self, preferred_device: Option<usize>) -> Option<usize> {
        if let Some(device) = preferred_device.filter(|device| *device < MAX_DEVICES) {
            self.clean_device_probes = self.clean_device_probes.wrapping_add(1);
            if self.devices[device].clean_head != NO_INDEX {
                return Some(usize::from(self.devices[device].clean_head));
            }
        }
        for probes in 0..MAX_DEVICES {
            let device = (usize::from(self.clean_device_cursor) + probes) % MAX_DEVICES;
            self.clean_device_probes = self.clean_device_probes.wrapping_add(1);
            let head = self.devices[device].clean_head;
            if head == NO_INDEX {
                continue;
            }
            self.clean_device_cursor = ((device + 1) % MAX_DEVICES) as u8;
            return Some(usize::from(head));
        }
        None
    }
    fn dirty_head(&self, device: usize) -> Option<usize> {
        if device >= MAX_DEVICES {
            return None;
        }
        let head = self.devices[device].dirty_head;
        if head == NO_INDEX {
            None
        } else {
            Some(usize::from(head))
        }
    }
    fn next_dirty(&self, index: usize) -> Option<usize> {
        if index >= MAX_ENTRIES || self.links[index].queue != Queue::Dirty {
            return None;
        }
        let next = self.links[index].next;
        if next == NO_INDEX {
            None
        } else {
            Some(usize::from(next))
        }
    }
    fn next_dirty_device(&mut self, pressure_only: bool) -> Option<usize> {
        for probes in 0..MAX_DEVICES {
            let index = (usize::from(self.dirty_device_cursor) + probes) % MAX_DEVICES;
            self.dirty_device_probes = self.dirty_device_probes.wrapping_add(1);
            let device = self.devices[index];
            if device.dirty_head == NO_INDEX || (pressure_only && !device.pressure_active) {
                continue;
            }
            self.dirty_device_cursor = ((index + 1) % MAX_DEVICES) as u8;
            return Some(index);
        }
        None
    }
    fn entry_count(&self) -> u16 {
        MAX_ENTRIES as u16 - self.free_count
    }
    fn dirty_count(&self) -> u16 {
        let mut total = 0;
        for device in &self.devices {
            total += device.dirty;
        }
        total
    }
    fn clean_count(&self) -> u16 {
        let mut total = 0;
        for device in &self.devices {
            total += device.clean;
        }
        total
    }
    fn unlink_clean(&mut self, index: usize, device: usize) {
        self.unlink_from_list(index, device, false);
    }
    fn unlink_dirty(&mut self, index: usize, device: usize) {
        self.unlink_from_list(index, device, true);
    }
    fn unlink_from_list(&mut self, index: usize, device: usize, dirty: bool) {
        let prev = self.links[index].prev;
        let next = self.links[index].next;
        if prev == NO_INDEX {
            if dirty {
                self.devices[device].dirty_head = next;
            } else {
                self.devices[device].clean_head = next;
            }
        } else {
            self.links[usize::from(prev)].next = next;
        }
        if next == NO_INDEX {
            if dirty {
                self.devices[device].dirty_tail = prev;
            } else {
                self.devices[device].clean_tail = prev;
            }
        } else {
            self.links[usize::from(next)].prev = prev;
        }
        self.links[index].prev = NO_INDEX;
        self.links[index].next = NO_INDEX;
    }
    fn link_clean_head(&mut self, index: usize, device: usize) {
        let old = self.devices[device].clean_head;
        self.links[index].prev = NO_INDEX;
        self.links[index].next = old;
        if old == NO_INDEX {
            self.devices[device].clean_tail = index as u16;
        } else {
            self.links[usize::from(old)].prev = index as u16;
        }
        self.devices[device].clean_head = index as u16;
    }
    fn link_clean_tail(&mut self, index: usize, device: usize) {
        self.link_tail(index, device, false);
    }
    fn link_dirty_tail(&mut self, index: usize, device: usize) {
        self.link_tail(index, device, true);
    }
    fn link_dirty_head(&mut self, index: usize, device: usize) {
        let old_head = self.devices[device].dirty_head;
        self.links[index].prev = NO_INDEX;
        self.links[index].next = old_head;
        if old_head == NO_INDEX {
            self.devices[device].dirty_tail = index as u16;
        } else {
            self.links[usize::from(old_head)].prev = index as u16;
        }
        self.devices[device].dirty_head = index as u16;
    }
    fn link_tail(&mut self, index: usize, device: usize, dirty: bool) {
        let old_tail = if dirty {
            self.devices[device].dirty_tail
        } else {
            self.devices[device].clean_tail
        };
        self.links[index].prev = old_tail;
        self.links[index].next = NO_INDEX;
        if old_tail == NO_INDEX {
            if dirty {
                self.devices[device].dirty_head = index as u16;
            } else {
                self.devices[device].clean_head = index as u16;
            }
        } else {
            self.links[usize::from(old_tail)].next = index as u16;
        }
        if dirty {
            self.devices[device].dirty_tail = index as u16;
        } else {
            self.devices[device].clean_tail = index as u16;
        }
    }
}

// The original Zig global was already fully initialized before page_cache.init.
// Keep that pre-init state with one native mutable owner, not an undefined Zig
// placeholder or a second Zig constructor. The template is immutable only.
static INDEX_TEMPLATE: Index = Index::initial_state();
#[repr(transparent)]
pub struct GlobalIndex(UnsafeCell<Index>);
// SAFETY: The original cache owner serializes every runtime access; pre-runtime
// initialization and early reads are BSP-only. No independent mutation exists.
unsafe impl Sync for GlobalIndex {}
#[unsafe(no_mangle)]
pub static R4OS_FS_CACHE_INDEX: GlobalIndex = GlobalIndex(UnsafeCell::new(Index::initial_state()));
// Same exact resident index used by the native cache and original C fixtures.
// The cache mutex (or exclusive BSP reset) protects every dereference.
pub(super) fn global_index() -> *mut Index {
    R4OS_FS_CACHE_INDEX.0.get()
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fs_cache_fill_limit(
    requested: u16,
    max_sectors: u16,
    page_sectors: u16,
    capacity: u16,
) -> u16 {
    fill_run_page_limit(requested, max_sectors, page_sectors, capacity)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fs_cache_fill_attempts(owns_transport_retry: bool) -> u8 {
    if owns_transport_retry { 1 } else { 2 }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fs_cache_fill_mask(
    operation: u8,
    valid: u8,
    readable: u8,
    success: bool,
) -> u8 {
    match operation {
        0 => readable & !valid,
        1 => {
            if success {
                valid | readable
            } else {
                valid
            }
        }
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_fs_cache_age_due(now: u64, dirty_since: u64, max_age: u64) -> bool {
    dirty_since != 0 && now >= dirty_since && now - dirty_since >= max_age
}

/// # Safety
/// Exclusive aligned output is caller-owned, resident and writable for this
/// synchronous scalar query. No mutation, callback or wait spans the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_cache_capacity(
    reference: u64,
    current: u64,
    output: *mut Capacity,
) {
    unsafe { ptr::write(output, capacity_for_memory(reference, current)) };
}

/// # Safety
/// State is initialized and exclusively protected by the original cache
/// caller owner. Observe output is aligned/exclusive and disjoint from state;
/// reset does not use output. No pointer/reference survives the pure call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_cache_sequential(
    state: *mut Sequential,
    operation: u8,
    first: u64,
    end: u64,
    window: u16,
    output: *mut SequentialDecision,
) {
    let state = unsafe { &mut *state };
    match operation {
        0 => unsafe { ptr::write(output, state.observe(first, end, window)) },
        1 => state.reset(),
        _ => unreachable!(),
    }
}

/// # Safety
/// Initialized state is exclusive under the original cache caller owner.
/// Complete input and begin output, when used, are aligned resident records
/// disjoint from state. Other operations do not use these optional pointers.
/// The disabled read-ahead model performs no I/O or wait and admits no worker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_cache_readahead(
    state: *mut ReadAhead,
    operation: u8,
    page: u64,
    pages: u16,
    input: *const ReadAheadRequest,
    success: bool,
    output: *mut ReadAheadRequest,
) -> bool {
    let state = unsafe { &mut *state };
    match operation {
        0 => state.schedule(page, pages),
        1 => state.demand(page),
        2 => state.cancel_all(),
        3 => {
            if let Some(request) = state.begin() {
                unsafe { ptr::write(output, request) };
                true
            } else {
                false
            }
        }
        4 => state.complete(unsafe { ptr::read(input) }, success),
        5 => {
            state.consume_resident();
            true
        }
        _ => unreachable!(),
    }
}

/// # Safety
/// Exact aligned state is fully resident and exclusive under the original
/// cache caller owner. Init writes its uninitialized backing in place; every
/// other operation requires an initialized index with its original queue
/// invariants. No callback, wait or second cache state exists in this owner.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_fs_cache_index(
    state: *mut Index,
    operation: u8,
    index: usize,
    device: usize,
    flag: bool,
) -> usize {
    if operation == 0 {
        unsafe { Index::initialize(state) };
        return 0;
    }
    // The original const Zig queries may refer to read-only storage. Never
    // manufacture an exclusive reference for these read-only operations.
    if matches!(operation, 11 | 12 | 14 | 15 | 16) {
        let state = unsafe { &*state };
        return match operation {
            11 => state.dirty_head(device).unwrap_or(usize::MAX),
            12 => state.next_dirty(index).unwrap_or(usize::MAX),
            14 => usize::from(state.entry_count()),
            15 => usize::from(state.dirty_count()),
            16 => usize::from(state.clean_count()),
            _ => unreachable!(),
        };
    }
    let state = unsafe { &mut *state };
    match operation {
        1 => state.claim_free().unwrap_or(usize::MAX),
        2 => usize::from(state.attach_clean(index, device)),
        3 => {
            state.touch_clean(index);
            0
        }
        4 => usize::from(state.mark_dirty(index)),
        5 => usize::from(state.clear_dirty(index)),
        6 => usize::from(state.pin(index)),
        7 => usize::from(state.unpin(index, flag)),
        8 => usize::from(state.detach(index)),
        9 => usize::from(state.release(index)),
        10 => state
            .clean_victim(if device == usize::MAX {
                None
            } else {
                Some(device)
            })
            .unwrap_or(usize::MAX),
        13 => state.next_dirty_device(flag).unwrap_or(usize::MAX),
        _ => unreachable!(),
    }
}

const _: () = {
    assert!(size_of::<Capacity>() == 8);
    assert!(offset_of!(Capacity, read_ahead_pages) == 6);
    assert!(size_of::<SequentialDecision>() == 16);
    assert!(size_of::<Sequential>() == 24);
    assert!(offset_of!(Sequential, last_end_page) == 8);
    assert!(size_of::<Link>() == 8);
    assert!(size_of::<Device>() == 20);
    assert!(size_of::<ReadAheadRequest>() == 24);
    assert!(size_of::<ReadAhead>() == 72);
    assert!(offset_of!(ReadAhead, sequential) == 48);
    assert!(MAX_ENTRIES < u16::MAX as usize);
    assert!(size_of::<Index>() == MAX_ENTRIES * size_of::<Link>() + 192);
    assert!(offset_of!(Index, devices) == MAX_ENTRIES * size_of::<Link>());
    assert!(offset_of!(Index, clean_device_probes) == MAX_ENTRIES * size_of::<Link>() + 176);
};
