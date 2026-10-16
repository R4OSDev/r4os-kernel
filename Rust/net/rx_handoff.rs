//! Existing bounded NIC-to-protocol handoff. Netcore retains admission for
//! every mutation; a processing slot remains occupied until exact release.
pub const CAPACITY: usize = 64;
pub const MAX_FRAME_BYTES: usize = 1536;
const FREE: u8 = 0;
const QUEUED: u8 = 1;
const PROCESSING: u8 = 2;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Metadata {
    pub l4_checksum_valid: bool,
    pub software_fallback: bool,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Slot {
    pub state: u8,
    pub generation: u32,
    pub adapter_index: usize,
    pub len: u16,
    pub enqueued_ns: u64,
    pub metadata: Metadata,
    pub data: [u8; MAX_FRAME_BYTES],
}
impl Slot {
    pub const EMPTY: Self = Self {
        state: FREE,
        generation: 0,
        adapter_index: 0,
        len: 0,
        enqueued_ns: 0,
        metadata: Metadata {
            l4_checksum_valid: false,
            software_fallback: false,
        },
        data: [0; MAX_FRAME_BYTES],
    };
}
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum EnqueueResult {
    Accepted,
    InvalidFrame,
    Busy,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Claim {
    pub slot: u8,
    pub generation: u32,
    pub adapter_index: usize,
    pub len: u16,
    pub enqueued_ns: u64,
    pub metadata: Metadata,
}
#[repr(C)]
pub struct Queue {
    pub slots: [Slot; CAPACITY],
    pub ready: [u8; CAPACITY],
    pub ready_head: usize,
    pub ready_tail: usize,
    pub ready_count: usize,
    pub free_slots: [u8; CAPACITY],
    pub free_count: usize,
    pub accepted: u64,
    pub claimed: u64,
    pub released: u64,
    pub cancelled: u64,
    pub busy: u64,
    pub high_water: usize,
}
impl Queue {
    pub const EMPTY: Self = Self {
        slots: [Slot::EMPTY; CAPACITY],
        ready: [0; CAPACITY],
        ready_head: 0,
        ready_tail: 0,
        ready_count: 0,
        free_slots: [0; CAPACITY],
        free_count: 0,
        accepted: 0,
        claimed: 0,
        released: 0,
        cancelled: 0,
        busy: 0,
        high_water: 0,
    };
    pub fn reset(&mut self) {
        // All fields admit the original all-zero initialization. Clear in
        // place: this queue is larger than a kernel task's complete stack.
        unsafe {
            core::ptr::write_bytes(self as *mut Self, 0, 1);
        }
        self.reset_storage();
    }
    pub fn enqueue(
        &mut self,
        adapter: usize,
        frame: &[u8],
        enqueued_ns: u64,
        metadata: Metadata,
    ) -> EnqueueResult {
        if frame.is_empty() || frame.len() > MAX_FRAME_BYTES {
            return EnqueueResult::InvalidFrame;
        }
        if self.free_count == 0 {
            self.busy = self.busy.wrapping_add(1);
            return EnqueueResult::Busy;
        }
        self.free_count -= 1;
        let index = usize::from(self.free_slots[self.free_count]);
        let slot = &mut self.slots[index];
        slot.generation = slot.generation.wrapping_add(1);
        if slot.generation == 0 {
            slot.generation = 1;
        }
        slot.adapter_index = adapter;
        slot.len = frame.len() as u16;
        slot.enqueued_ns = enqueued_ns;
        slot.metadata = metadata;
        slot.data[..frame.len()].copy_from_slice(frame);
        slot.state = QUEUED;
        self.ready[self.ready_tail] = index as u8;
        self.ready_tail = (self.ready_tail + 1) % CAPACITY;
        self.ready_count += 1;
        self.accepted = self.accepted.wrapping_add(1);
        self.high_water = self.high_water.max(self.occupied_count());
        EnqueueResult::Accepted
    }
    pub fn claim(&mut self) -> Option<Claim> {
        if self.ready_count == 0 {
            return None;
        }
        let index = usize::from(self.ready[self.ready_head]);
        self.ready_head = (self.ready_head + 1) % CAPACITY;
        self.ready_count -= 1;
        let slot = &mut self.slots[index];
        if slot.state != QUEUED {
            return None;
        }
        slot.state = PROCESSING;
        self.claimed = self.claimed.wrapping_add(1);
        Some(Claim {
            slot: index as u8,
            generation: slot.generation,
            adapter_index: slot.adapter_index,
            len: slot.len,
            enqueued_ns: slot.enqueued_ns,
            metadata: slot.metadata,
        })
    }
    pub fn frame(&self, claim: Claim) -> Option<&[u8]> {
        let index = usize::from(claim.slot);
        if index >= CAPACITY {
            return None;
        }
        let slot = &self.slots[index];
        if slot.state != PROCESSING || slot.generation != claim.generation || slot.len != claim.len
        {
            return None;
        }
        Some(&slot.data[..usize::from(slot.len)])
    }
    pub fn release(&mut self, claim: Claim) -> bool {
        let index = usize::from(claim.slot);
        if index >= CAPACITY {
            return false;
        }
        let slot = &mut self.slots[index];
        if slot.state != PROCESSING || slot.generation != claim.generation {
            return false;
        }
        slot.state = FREE;
        slot.adapter_index = 0;
        slot.len = 0;
        slot.enqueued_ns = 0;
        slot.metadata = Metadata::default();
        self.free_slots[self.free_count] = index as u8;
        self.free_count += 1;
        self.released = self.released.wrapping_add(1);
        true
    }
    pub fn cancel_all(&mut self) -> usize {
        let now = self.occupied_count();
        self.cancelled = self.cancelled.wrapping_add(now as u64);
        self.reset_storage();
        now
    }
    pub fn queued_count(&self) -> usize {
        self.ready_count
    }
    pub fn occupied_count(&self) -> usize {
        CAPACITY - self.free_count
    }
    fn reset_storage(&mut self) {
        self.ready_head = 0;
        self.ready_tail = 0;
        self.ready_count = 0;
        self.free_count = CAPACITY;
        for slot in &mut self.slots {
            slot.state = FREE;
            slot.adapter_index = 0;
            slot.len = 0;
            slot.enqueued_ns = 0;
            slot.metadata = Metadata::default();
        }
        for (index, slot) in self.free_slots.iter_mut().enumerate() {
            *slot = index as u8;
        }
    }
}
/// # Safety
/// Exclusive caller admission; all prior producers/consumers have drained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_reset(q: *mut Queue) {
    unsafe {
        (&mut *q).reset();
    }
}
/// # Safety
/// Exclusive queue, readable canonical frame and borrowed metadata; payload is
/// copied once before admission is returned, no source pointer is retained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_enqueue(
    q: *mut Queue,
    adapter: usize,
    bytes: *const u8,
    len: usize,
    tick: u64,
    meta: *const Metadata,
) -> u8 {
    if len == 0 || len > MAX_FRAME_BYTES {
        return EnqueueResult::InvalidFrame as u8;
    }
    unsafe {
        (&mut *q).enqueue(
            adapter,
            core::slice::from_raw_parts(bytes, len),
            tick,
            *meta,
        ) as u8
    }
}
/// # Safety
/// Exclusive queue and nonaliasing claim output; false leaves output untouched.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_claim(q: *mut Queue, out: *mut Claim) -> bool {
    unsafe {
        if let Some(value) = (&mut *q).claim() {
            out.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Exact claim and readable queue. Caller retains processing ownership until
/// release, so the returned raw payload cannot be overwritten by a producer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_frame(
    q: *const Queue,
    claim: *const Claim,
    len: *mut usize,
) -> *const u8 {
    unsafe {
        if let Some(frame) = (&*q).frame(*claim) {
            len.write(frame.len());
            frame.as_ptr()
        } else {
            len.write(0);
            core::ptr::null()
        }
    }
}
/// # Safety
/// Exclusive queue admission and exact retained slot/generation claim.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_release(q: *mut Queue, claim: *const Claim) -> bool {
    unsafe { (&mut *q).release(*claim) }
}
/// # Safety
/// Exclusive queue; all producers and processing callbacks have already drained.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_cancel_all(q: *mut Queue) -> usize {
    unsafe { (&mut *q).cancel_all() }
}
/// # Safety
/// Borrowed queue under its original caller admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_queued(q: *const Queue) -> usize {
    unsafe { (&*q).queued_count() }
}
/// # Safety
/// Borrowed queue under its original caller admission.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_rx_occupied(q: *const Queue) -> usize {
    unsafe { (&*q).occupied_count() }
}

const _: () = {
    assert!(core::mem::size_of::<Metadata>() == 2);
    assert!(core::mem::align_of::<Metadata>() == 1);
    assert!(core::mem::offset_of!(Metadata, l4_checksum_valid) == 0);
    assert!(core::mem::offset_of!(Metadata, software_fallback) == 1);
};
const _: () = {
    assert!(core::mem::size_of::<Slot>() == 1576);
    assert!(core::mem::align_of::<Slot>() == 8);
    assert!(core::mem::offset_of!(Slot, state) == 0);
    assert!(core::mem::offset_of!(Slot, generation) == 4);
    assert!(core::mem::offset_of!(Slot, adapter_index) == 8);
    assert!(core::mem::offset_of!(Slot, len) == 16);
    assert!(core::mem::offset_of!(Slot, enqueued_ns) == 24);
    assert!(core::mem::offset_of!(Slot, metadata) == 32);
    assert!(core::mem::offset_of!(Slot, data) == 34);
};
const _: () = {
    assert!(core::mem::size_of::<Claim>() == 40);
    assert!(core::mem::align_of::<Claim>() == 8);
    assert!(core::mem::offset_of!(Claim, slot) == 0);
    assert!(core::mem::offset_of!(Claim, generation) == 4);
    assert!(core::mem::offset_of!(Claim, adapter_index) == 8);
    assert!(core::mem::offset_of!(Claim, len) == 16);
    assert!(core::mem::offset_of!(Claim, enqueued_ns) == 24);
    assert!(core::mem::offset_of!(Claim, metadata) == 32);
};
const _: () = {
    assert!(core::mem::size_of::<Queue>() == 101072);
    assert!(core::mem::align_of::<Queue>() == 8);
    assert!(core::mem::offset_of!(Queue, slots) == 0);
    assert!(core::mem::offset_of!(Queue, ready) == 100864);
    assert!(core::mem::offset_of!(Queue, ready_head) == 100928);
    assert!(core::mem::offset_of!(Queue, ready_tail) == 100936);
    assert!(core::mem::offset_of!(Queue, ready_count) == 100944);
    assert!(core::mem::offset_of!(Queue, free_slots) == 100952);
    assert!(core::mem::offset_of!(Queue, free_count) == 101016);
    assert!(core::mem::offset_of!(Queue, accepted) == 101024);
    assert!(core::mem::offset_of!(Queue, claimed) == 101032);
    assert!(core::mem::offset_of!(Queue, released) == 101040);
    assert!(core::mem::offset_of!(Queue, cancelled) == 101048);
    assert!(core::mem::offset_of!(Queue, busy) == 101056);
    assert!(core::mem::offset_of!(Queue, high_water) == 101064);
};
