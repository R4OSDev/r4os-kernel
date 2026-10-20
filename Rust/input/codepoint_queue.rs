//! Original bounded codepoint queue; storage belongs to its invoking input owner.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub pending: u32,
    pub free: u32,
    pub high_water: u32,
    pub push_attempts: u64,
    pub dropped: u64,
}
pub const STORAGE_LEN: usize = 256;
pub const CAPACITY: u32 = 255;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Queue {
    pub items: [u32; STORAGE_LEN],
    pub head: usize,
    pub tail: usize,
    pub high_water: u32,
    pub push_attempts: u64,
    pub dropped: u64,
}
impl Queue {
    pub const EMPTY: Self = Self {
        items: [0; STORAGE_LEN],
        head: 0,
        tail: 0,
        high_water: 0,
        push_attempts: 0,
        dropped: 0,
    };
    pub fn clear(&mut self) {
        self.items.fill(0);
        self.head = 0;
        self.tail = 0;
    }
    pub fn try_push(&mut self, cp: u32) -> bool {
        self.push_attempts = self.push_attempts.wrapping_add(1);
        let next = (self.head + 1) % STORAGE_LEN;
        if next == self.tail {
            self.dropped = self.dropped.wrapping_add(1);
            return false;
        }
        self.items[self.head] = cp;
        self.head = next;
        self.high_water = self.high_water.max(self.pending_count());
        true
    }
    pub fn peek(&self) -> Option<u32> {
        if self.head == self.tail {
            None
        } else {
            Some(self.items[self.tail])
        }
    }
    pub fn pop(&mut self) -> Option<u32> {
        if self.head == self.tail {
            return None;
        }
        let cp = self.items[self.tail];
        self.items[self.tail] = 0;
        self.tail = (self.tail + 1) % STORAGE_LEN;
        Some(cp)
    }
    pub fn pending(&self) -> bool {
        self.head != self.tail
    }
    pub fn pending_count(&self) -> u32 {
        (if self.head >= self.tail {
            self.head - self.tail
        } else {
            STORAGE_LEN - self.tail + self.head
        }) as u32
    }
    pub fn free_count(&self) -> u32 {
        CAPACITY - self.pending_count()
    }
    pub fn can_accept(&self, n: u32) -> bool {
        n <= self.free_count()
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            pending: self.pending_count(),
            free: self.free_count(),
            high_water: self.high_water,
            push_attempts: self.push_attempts,
            dropped: self.dropped,
        }
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_reset(q: *mut Queue) {
    unsafe { q.write(Queue::EMPTY) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_clear(q: *mut Queue) {
    unsafe { (*q).clear() }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_push(q: *mut Queue, c: u32) -> bool {
    unsafe { (*q).try_push(c) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_pop(q: *mut Queue, o: *mut u32) -> bool {
    let Some(v) = (unsafe { (*q).pop() }) else {
        return false;
    };
    unsafe { o.write(v) }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_pending(q: *const Queue) -> bool {
    unsafe { (*q).pending() }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_count(q: *const Queue) -> u32 {
    unsafe { (*q).pending_count() }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_free(q: *const Queue) -> u32 {
    unsafe { (*q).free_count() }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_accept(q: *const Queue, n: u32) -> bool {
    unsafe { (*q).can_accept(n) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains serialized access to the initialized resident queue; any output is aligned and exclusive for this synchronous call.
pub unsafe extern "C" fn r4os_rust_input_codepoint_snapshot(q: *const Queue, o: *mut Snapshot) {
    unsafe { o.write((*q).snapshot()) }
}

const _: () = {
    assert!(core::mem::size_of::<Snapshot>() == 32);
    assert!(core::mem::align_of::<Snapshot>() == 8);
    assert!(core::mem::offset_of!(Snapshot, pending) == 0);
    assert!(core::mem::offset_of!(Snapshot, free) == 4);
    assert!(core::mem::offset_of!(Snapshot, high_water) == 8);
    assert!(core::mem::offset_of!(Snapshot, push_attempts) == 16);
    assert!(core::mem::offset_of!(Snapshot, dropped) == 24);
};
const _: () = {
    assert!(core::mem::size_of::<Queue>() == 1064);
    assert!(core::mem::align_of::<Queue>() == 8);
    assert!(core::mem::offset_of!(Queue, items) == 0);
    assert!(core::mem::offset_of!(Queue, head) == 1024);
    assert!(core::mem::offset_of!(Queue, tail) == 1032);
    assert!(core::mem::offset_of!(Queue, high_water) == 1040);
    assert!(core::mem::offset_of!(Queue, push_attempts) == 1048);
    assert!(core::mem::offset_of!(Queue, dropped) == 1056);
};
