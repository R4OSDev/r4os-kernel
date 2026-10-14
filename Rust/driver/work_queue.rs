//! Original bounded R4D queue/ledger policy, under the caller's runtime owner.
//! These fixed private C records are embedded in the existing Work owner.
//! No reference survives a callback, wait, dispatch or backing release.
use core::ptr;
const CAPACITY: usize = 16;
const NONE: u8 = 255;
const FREE: u8 = 0;
const QUEUED: u8 = 1;
const RUNNING: u8 = 2;
const COMPLETED: u8 = 3;
const CANCELLED: u8 = 4;
const TASK: u8 = 0;
const IRQ: u8 = 1;
const DEADLINE: u8 = 2;

#[repr(C)]
pub struct OwnerBookkeeping {
    pub used: u32,
    pub queued: u32,
    pub running: u32,
    pub deadline_running: u32,
    pub completed: u32,
    pub cancelled: u32,
    pub irq_queued: u32,
    pub task_queued: u32,
    pub deadline_queued: u32,
    pub waiters: u32,
    pub waiters_max: u32,
    pub used_high_water: u32,
    pub retained_high_water: u32,
    pub deadline_queue_high_water: u32,
}
impl OwnerBookkeeping {
    fn retained(&self) -> u32 {
        self.completed + self.cancelled
    }
    fn high_water(&mut self) {
        self.used_high_water = self.used_high_water.max(self.used);
        self.retained_high_water = self.retained_high_water.max(self.retained());
    }
    fn subtract_class(&mut self, source: u8) {
        let count = match source {
            TASK => &mut self.task_queued,
            IRQ => &mut self.irq_queued,
            DEADLINE => &mut self.deadline_queued,
            _ => panic!("Invalid Work source"),
        };
        *count = count.saturating_sub(1);
    }
}

#[repr(C)]
pub struct Bookkeeping {
    pub states: [u8; CAPACITY],
    pub classes: [u8; CAPACITY],
    pub queue_prev: [u8; CAPACITY],
    pub queue_next: [u8; CAPACITY],
    pub final_prev: [u8; CAPACITY],
    pub final_next: [u8; CAPACITY],
    pub irq_head: u8,
    pub irq_tail: u8,
    pub task_head: u8,
    pub task_tail: u8,
    pub deadline_head: u8,
    pub deadline_tail: u8,
    pub final_head: u8,
    pub final_tail: u8,
    pub free_mask: u16,
    pub free_count: u32,
    pub queued_count: u32,
    pub running_count: u32,
    pub deadline_running_count: u32,
    pub completed_count: u32,
    pub cancelled_count: u32,
    pub irq_queued_count: u32,
    pub task_queued_count: u32,
    pub deadline_queued_count: u32,
    pub queue_high_water: u32,
    pub used_high_water: u32,
    pub retained_high_water: u32,
    pub deadline_queue_high_water: u32,
    pub current_irq_burst: u32,
}
#[repr(C)]
pub struct Selection {
    pub slot: usize,
    pub source: u8,
    pub irq_preferred: bool,
    pub task_fairness: bool,
}
const _: () = {
    assert!(core::mem::size_of::<OwnerBookkeeping>() == 56);
    assert!(core::mem::size_of::<Bookkeeping>() == 164);
    assert!(core::mem::offset_of!(Bookkeeping, free_count) == 108);
    assert!(core::mem::size_of::<Selection>() == 16);
    assert!(core::mem::offset_of!(Selection, source) == 8);
};
impl Bookkeeping {
    fn used(&self) -> u32 {
        CAPACITY as u32 - self.free_count
    }
    fn retained(&self) -> u32 {
        self.completed_count + self.cancelled_count
    }
    fn heads(&self, source: u8) -> (u8, u8) {
        match source {
            TASK => (self.task_head, self.task_tail),
            IRQ => (self.irq_head, self.irq_tail),
            DEADLINE => (self.deadline_head, self.deadline_tail),
            _ => panic!("Invalid Work source"),
        }
    }
    fn set_head(&mut self, source: u8, slot: u8) {
        match source {
            TASK => self.task_head = slot,
            IRQ => self.irq_head = slot,
            DEADLINE => self.deadline_head = slot,
            _ => panic!("Invalid Work source"),
        }
    }
    fn set_tail(&mut self, source: u8, slot: u8) {
        match source {
            TASK => self.task_tail = slot,
            IRQ => self.irq_tail = slot,
            DEADLINE => self.deadline_tail = slot,
            _ => panic!("Invalid Work source"),
        }
    }
    fn class_count(&mut self, source: u8) -> &mut u32 {
        match source {
            TASK => &mut self.task_queued_count,
            IRQ => &mut self.irq_queued_count,
            DEADLINE => &mut self.deadline_queued_count,
            _ => panic!("Invalid Work source"),
        }
    }
    fn append_queue(&mut self, slot: usize, source: u8) {
        let (_, tail) = self.heads(source);
        self.queue_prev[slot] = NONE;
        self.queue_next[slot] = NONE;
        if tail == NONE {
            self.set_head(source, slot as u8);
        } else {
            self.queue_next[tail as usize] = slot as u8;
            self.queue_prev[slot] = tail;
        }
        self.set_tail(source, slot as u8);
    }
    fn remove_queue(&mut self, slot: usize) {
        let source = self.classes[slot];
        let prev = self.queue_prev[slot];
        let next = self.queue_next[slot];
        if prev == NONE {
            self.set_head(source, next);
        } else {
            self.queue_next[prev as usize] = next;
        }
        if next == NONE {
            self.set_tail(source, prev);
        } else {
            self.queue_prev[next as usize] = prev;
        }
        self.queue_prev[slot] = NONE;
        self.queue_next[slot] = NONE;
    }
    fn append_final(&mut self, slot: usize) {
        self.final_prev[slot] = self.final_tail;
        self.final_next[slot] = NONE;
        if self.final_tail == NONE {
            self.final_head = slot as u8;
        } else {
            self.final_next[self.final_tail as usize] = slot as u8;
        }
        self.final_tail = slot as u8;
    }
    fn remove_final(&mut self, slot: usize) {
        let prev = self.final_prev[slot];
        let next = self.final_next[slot];
        if prev == NONE {
            self.final_head = next;
        } else {
            self.final_next[prev as usize] = next;
        }
        if next == NONE {
            self.final_tail = prev;
        } else {
            self.final_prev[next as usize] = prev;
        }
        self.final_prev[slot] = NONE;
        self.final_next[slot] = NONE;
    }
    fn high_water(&mut self) {
        self.queue_high_water = self.queue_high_water.max(self.queued_count);
        self.used_high_water = self.used_high_water.max(self.used());
        self.retained_high_water = self.retained_high_water.max(self.retained());
        self.deadline_queue_high_water = self
            .deadline_queue_high_water
            .max(self.deadline_queued_count);
    }
    fn reserve(&mut self, source: u8) -> Option<usize> {
        if self.free_mask == 0 {
            return None;
        }
        if source == DEADLINE {
            if self.deadline_queued_count >= 4 {
                return None;
            }
        } else if self.free_count <= 2 {
            return None;
        }
        let slot = self.free_mask.trailing_zeros() as usize;
        self.free_mask &= !(1u16 << slot);
        self.free_count -= 1;
        self.states[slot] = QUEUED;
        self.classes[slot] = source;
        self.queued_count += 1;
        *self.class_count(source) += 1;
        self.append_queue(slot, source);
        self.high_water();
        Some(slot)
    }
    fn start(&mut self, selection: &Selection) {
        self.remove_queue(selection.slot);
        self.states[selection.slot] = RUNNING;
        self.queued_count -= 1;
        self.running_count += 1;
        if selection.source == DEADLINE {
            self.deadline_running_count += 1;
        }
        *self.class_count(selection.source) -= 1;
    }
    fn normal(&mut self) -> Option<Selection> {
        let has_irq = self.irq_head != NONE;
        let has_task = self.task_head != NONE;
        if !has_irq && !has_task {
            return None;
        }
        let selection = if has_irq && (!has_task || self.current_irq_burst < 4) {
            self.current_irq_burst = self.current_irq_burst.saturating_add(1);
            Selection {
                slot: self.irq_head as usize,
                source: IRQ,
                irq_preferred: has_task,
                task_fairness: false,
            }
        } else {
            self.current_irq_burst = 0;
            Selection {
                slot: self.task_head as usize,
                source: TASK,
                irq_preferred: false,
                task_fairness: has_irq,
            }
        };
        self.start(&selection);
        Some(selection)
    }
    fn cancel(&mut self, slot: usize) -> bool {
        if slot >= CAPACITY || self.states[slot] != QUEUED {
            return false;
        }
        let source = self.classes[slot];
        self.remove_queue(slot);
        self.states[slot] = CANCELLED;
        self.queued_count -= 1;
        self.cancelled_count += 1;
        *self.class_count(source) -= 1;
        self.append_final(slot);
        self.high_water();
        true
    }
    fn complete(&mut self, slot: usize) -> bool {
        if slot >= CAPACITY || self.states[slot] != RUNNING {
            return false;
        }
        self.states[slot] = COMPLETED;
        self.running_count -= 1;
        if self.classes[slot] == DEADLINE && self.deadline_running_count != 0 {
            self.deadline_running_count -= 1;
        }
        self.completed_count += 1;
        self.append_final(slot);
        self.high_water();
        true
    }
    fn release(&mut self, slot: usize) -> bool {
        if slot >= CAPACITY {
            return false;
        }
        match self.states[slot] {
            COMPLETED => self.completed_count -= 1,
            CANCELLED => self.cancelled_count -= 1,
            _ => return false,
        }
        self.remove_final(slot);
        self.states[slot] = FREE;
        self.classes[slot] = TASK;
        self.free_mask |= 1u16 << slot;
        self.free_count += 1;
        true
    }
}

/// # Safety
/// Exclusive aligned record under its owner's runtime boundary, or isolated
/// fixture storage. This call performs no callback, wait or dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_work_owner_ledger(
    book: *mut OwnerBookkeeping,
    operation: u8,
    value: u8,
) -> u32 {
    let b = unsafe { &mut *book };
    match operation {
        0 => {
            b.used = b.used.saturating_add(1);
            b.queued = b.queued.saturating_add(1);
            let count = match value {
                TASK => &mut b.task_queued,
                IRQ => &mut b.irq_queued,
                DEADLINE => &mut b.deadline_queued,
                _ => panic!("Invalid Work source"),
            };
            *count = count.saturating_add(1);
            b.deadline_queue_high_water = b.deadline_queue_high_water.max(b.deadline_queued);
            b.high_water();
        }
        1 => {
            b.queued = b.queued.saturating_sub(1);
            b.running = b.running.saturating_add(1);
            if value == DEADLINE {
                b.deadline_running = b.deadline_running.saturating_add(1);
            }
            b.subtract_class(value);
        }
        2 => {
            b.queued = b.queued.saturating_sub(1);
            b.cancelled = b.cancelled.saturating_add(1);
            b.subtract_class(value);
            b.high_water();
        }
        3 => {
            b.running = b.running.saturating_sub(1);
            if value == DEADLINE {
                b.deadline_running = b.deadline_running.saturating_sub(1);
            }
            b.completed = b.completed.saturating_add(1);
            b.high_water();
        }
        4 => {
            b.used = b.used.saturating_sub(1);
            match value {
                COMPLETED => b.completed = b.completed.saturating_sub(1),
                CANCELLED => b.cancelled = b.cancelled.saturating_sub(1),
                _ => {}
            }
        }
        5 => {
            b.waiters = b.waiters.saturating_add(1);
            b.waiters_max = b.waiters_max.max(b.waiters);
        }
        6 => b.waiters = b.waiters.saturating_sub(1),
        _ => panic!("Invalid Work ledger operation"),
    }
    0
}

/// # Safety
/// Aligned read-only ledger under its existing owner or fixture lifetime.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_work_owner_retained(book: *const OwnerBookkeeping) -> u32 {
    unsafe { (&*book).retained() }
}

/// # Safety
/// Exclusive exact C record under the caller's runtime boundary, or isolated
/// fixture storage. All operations are bounded and retain no pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_work_queue(
    book: *mut Bookkeeping,
    operation: u8,
    value: usize,
) -> usize {
    let b = unsafe { &mut *book };
    match operation {
        0 => b
            .reserve(u8::try_from(value).expect("Invalid Work source"))
            .unwrap_or(usize::MAX),
        1 => usize::from(b.cancel(value)),
        2 => usize::from(b.complete(value)),
        3 => usize::from(b.release(value)),
        _ => panic!("Invalid Work queue mutation"),
    }
}

/// # Safety
/// Shared aligned exact C record, protected by the existing Work owner or
/// isolated fixture lifetime. No concurrent mutation during this scalar read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_work_queue_query(
    book: *const Bookkeeping,
    operation: u8,
    slot: usize,
) -> usize {
    let b = unsafe { &*book };
    match operation {
        4 => {
            if slot < CAPACITY {
                b.states[slot] as usize
            } else {
                FREE as usize
            }
        }
        5 => {
            if slot < CAPACITY {
                b.classes[slot] as usize
            } else {
                TASK as usize
            }
        }
        6 => b.used() as usize,
        7 => b.retained() as usize,
        8 => {
            if b.final_head == NONE {
                usize::MAX
            } else {
                b.final_head as usize
            }
        }
        _ => panic!("Invalid Work queue query"),
    }
}

/// # Safety
/// Exclusive book and writable output; optional deadlines contain `len`
/// aligned readable u64s and do not alias either output. Held runtime owner
/// protects every input for this bounded selection, without dispatch.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_work_queue_select(
    book: *mut Bookkeeping,
    deadline_lane: bool,
    deadlines: *const u64,
    len: usize,
    output: *mut Selection,
) -> bool {
    let b = unsafe { &mut *book };
    let selection = if !deadline_lane {
        b.normal()
    } else {
        if b.deadline_head == NONE || len < CAPACITY {
            return false;
        }
        let mut best = b.deadline_head as usize;
        let mut cursor = b.queue_next[best];
        while cursor != NONE {
            if unsafe { deadlines.add(cursor as usize).read() < deadlines.add(best).read() } {
                best = cursor as usize;
            }
            cursor = b.queue_next[cursor as usize];
        }
        let s = Selection {
            slot: best,
            source: DEADLINE,
            irq_preferred: false,
            task_fairness: false,
        };
        b.start(&s);
        Some(s)
    };
    if let Some(s) = selection {
        unsafe { ptr::write(output, s) };
        true
    } else {
        false
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_work_release_decision(
    state: u8,
    published: bool,
    waiters: u32,
    claimed: bool,
) -> u8 {
    match state {
        FREE => 0,
        QUEUED | RUNNING => 1,
        COMPLETED | CANCELLED => {
            if !published {
                2
            } else if waiters != 0 {
                3
            } else if claimed {
                4
            } else {
                5
            }
        }
        _ => panic!("Invalid Work state"),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_work_cleanup_decision(state: u8, published: bool, waiters: u32) -> u8 {
    match state {
        FREE => 0,
        QUEUED => 1,
        RUNNING => 2,
        COMPLETED | CANCELLED => {
            if !published {
                3
            } else if waiters != 0 {
                4
            } else {
                5
            }
        }
        _ => panic!("Invalid Work state"),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_work_handle(operation: u8, a: usize, generation: u32) -> usize {
    match operation {
        0 => {
            (((generation & 0x00ff_ffff) << 8)
                | u32::try_from(a + 1).expect("Work slot exceeds handle")) as usize
        }
        1 => {
            let code = a & 255;
            if code == 0 || code > CAPACITY {
                usize::MAX
            } else {
                code - 1
            }
        }
        2 => {
            let next = (generation + 1) & 0x00ff_ffff;
            if next == 0 { 1 } else { next as usize }
        }
        _ => panic!("Invalid Work handle operation"),
    }
}
