//! Layout-independent physical transitions and the original reset-on-overflow queue.
#[repr(u8)]
#[derive(Clone, Copy, Default)]
pub enum Kind {
    Down,
    Up,
    #[default]
    Reset,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Event {
    pub kind: Kind,
    pub usage: u16,
    pub modifiers: u32,
    pub flags: u32,
    pub sequence: u64,
    pub tick: u64,
}
impl Event {
    const EMPTY: Self = Self {
        kind: Kind::Reset,
        usage: 0,
        modifiers: 0,
        flags: 0,
        sequence: 0,
        tick: 0,
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Snapshot {
    pub pending: u32,
    pub high_water: u32,
    pub transitions: u64,
    pub duplicate_transitions: u64,
    pub overflow_resets: u64,
}
const STORAGE_LEN: usize = 257;
pub const CAPACITY: u32 = 256;
pub const FLAG_REPEAT: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Queue {
    pub items: [Event; STORAGE_LEN],
    pub held: [bool; 256],
    pub head: usize,
    pub tail: usize,
    pub next_sequence: u64,
    pub high_water: u32,
    pub transitions: u64,
    pub duplicate_transitions: u64,
    pub overflow_resets: u64,
}
impl Queue {
    pub const EMPTY: Self = Self {
        items: [Event::EMPTY; STORAGE_LEN],
        held: [false; 256],
        head: 0,
        tail: 0,
        next_sequence: 1,
        high_water: 0,
        transitions: 0,
        duplicate_transitions: 0,
        overflow_resets: 0,
    };
    pub fn clear(&mut self) {
        self.items.fill(Event::EMPTY);
        self.held.fill(false);
        self.head = 0;
        self.tail = 0;
    }
    pub fn transition(&mut self, usage: u16, down: bool, tick: u64) -> bool {
        if usage == 0 || usage >= 256 {
            return false;
        }
        let i = usage as usize;
        if self.held[i] == down {
            self.duplicate_transitions = self.duplicate_transitions.wrapping_add(1);
            if !down {
                return false;
            }
            if self.full() {
                self.publish_reset(tick);
                return true;
            }
            let sequence = self.take_sequence();
            self.push(Event {
                kind: Kind::Down,
                usage,
                modifiers: self.modifiers(),
                flags: FLAG_REPEAT,
                sequence,
                tick,
            });
            return true;
        }
        if self.full() {
            self.publish_reset(tick);
            return true;
        }
        self.held[i] = down;
        let sequence = self.take_sequence();
        self.push(Event {
            kind: if down { Kind::Down } else { Kind::Up },
            usage,
            modifiers: self.modifiers(),
            sequence,
            tick,
            ..Event::EMPTY
        });
        self.transitions = self.transitions.wrapping_add(1);
        true
    }
    pub fn pop(&mut self) -> Option<Event> {
        if self.head == self.tail {
            return None;
        }
        let e = self.items[self.tail];
        self.items[self.tail] = Event::EMPTY;
        self.tail = (self.tail + 1) % STORAGE_LEN;
        Some(e)
    }
    pub fn pending(&self) -> bool {
        self.head != self.tail
    }
    fn count(&self) -> u32 {
        (if self.head >= self.tail {
            self.head - self.tail
        } else {
            STORAGE_LEN - self.tail + self.head
        }) as u32
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            pending: self.count(),
            high_water: self.high_water,
            transitions: self.transitions,
            duplicate_transitions: self.duplicate_transitions,
            overflow_resets: self.overflow_resets,
        }
    }
    fn full(&self) -> bool {
        (self.head + 1) % STORAGE_LEN == self.tail
    }
    fn push(&mut self, e: Event) {
        self.items[self.head] = e;
        self.head = (self.head + 1) % STORAGE_LEN;
        self.high_water = self.high_water.max(self.count());
    }
    fn publish_reset(&mut self, tick: u64) {
        self.items.fill(Event::EMPTY);
        self.held.fill(false);
        self.head = 0;
        self.tail = 0;
        self.overflow_resets = self.overflow_resets.wrapping_add(1);
        let sequence = self.take_sequence();
        self.push(Event {
            sequence,
            tick,
            ..Event::EMPTY
        });
    }
    fn take_sequence(&mut self) -> u64 {
        let n = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        if self.next_sequence == 0 {
            self.next_sequence = 1
        }
        n
    }
    fn modifiers(&self) -> u32 {
        let mut v = 0;
        for (i, k) in [0xe0, 0xe4, 0xe2, 0xe6, 0xe1, 0xe5, 0xe3, 0xe7]
            .into_iter()
            .enumerate()
        {
            v |= u32::from(self.held[k]) << i;
        }
        v
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns serialized access to the initialized resident physical queue; optional event/snapshot outputs are aligned and exclusive.
pub unsafe extern "C" fn r4os_rust_input_physical_clear(q: *mut Queue) {
    unsafe { (*q).clear() }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns serialized access to the initialized resident physical queue; optional event/snapshot outputs are aligned and exclusive.
pub unsafe extern "C" fn r4os_rust_input_physical_transition(
    q: *mut Queue,
    u: u16,
    d: bool,
    t: u64,
) -> bool {
    unsafe { (*q).transition(u, d, t) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns serialized access to the initialized resident physical queue; optional event/snapshot outputs are aligned and exclusive.
pub unsafe extern "C" fn r4os_rust_input_physical_pop(q: *mut Queue, o: *mut Event) -> bool {
    let Some(v) = (unsafe { (*q).pop() }) else {
        return false;
    };
    unsafe { o.write(v) }
    true
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns serialized access to the initialized resident physical queue; optional event/snapshot outputs are aligned and exclusive.
pub unsafe extern "C" fn r4os_rust_input_physical_pending(q: *const Queue) -> bool {
    unsafe { (*q).pending() }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns serialized access to the initialized resident physical queue; optional event/snapshot outputs are aligned and exclusive.
pub unsafe extern "C" fn r4os_rust_input_physical_snapshot(q: *const Queue, o: *mut Snapshot) {
    unsafe { o.write((*q).snapshot()) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_set1_usage(s: u8, e: bool) -> u16 {
    set1_usage(s, e).unwrap_or(0)
}

pub fn set1_usage(scancode: u8, extended: bool) -> Option<u16> {
    if extended {
        return match scancode {
            0x1C => Some(0x58),
            0x1D => Some(0xE4),
            0x35 => Some(0x54),
            0x37 => Some(0x46),
            0x38 => Some(0xE6),
            0x47 => Some(0x4A),
            0x48 => Some(0x52),
            0x49 => Some(0x4B),
            0x4B => Some(0x50),
            0x4D => Some(0x4F),
            0x4F => Some(0x4D),
            0x50 => Some(0x51),
            0x51 => Some(0x4E),
            0x52 => Some(0x49),
            0x53 => Some(0x4C),
            0x5B => Some(0xE3),
            0x5C => Some(0xE7),
            0x5D => Some(0x65),
            _ => None,
        };
    }
    match scancode {
        0x01 => Some(0x29),
        0x02..=0x0B => Some(u16::from(scancode) + 0x1C),
        0x0C => Some(0x2D),
        0x0D => Some(0x2E),
        0x0E => Some(0x2A),
        0x0F => Some(0x2B),
        0x10 => Some(0x14),
        0x11 => Some(0x1A),
        0x12 => Some(0x08),
        0x13 => Some(0x15),
        0x14 => Some(0x17),
        0x15 => Some(0x1C),
        0x16 => Some(0x18),
        0x17 => Some(0x0C),
        0x18 => Some(0x12),
        0x19 => Some(0x13),
        0x1A => Some(0x2F),
        0x1B => Some(0x30),
        0x1C => Some(0x28),
        0x1D => Some(0xE0),
        0x1E => Some(0x04),
        0x1F => Some(0x16),
        0x20 => Some(0x07),
        0x21 => Some(0x09),
        0x22 => Some(0x0A),
        0x23 => Some(0x0B),
        0x24 => Some(0x0D),
        0x25 => Some(0x0E),
        0x26 => Some(0x0F),
        0x27 => Some(0x33),
        0x28 => Some(0x34),
        0x29 => Some(0x35),
        0x2A => Some(0xE1),
        0x2B => Some(0x31),
        0x2C => Some(0x1D),
        0x2D => Some(0x1B),
        0x2E => Some(0x06),
        0x2F => Some(0x19),
        0x30 => Some(0x05),
        0x31 => Some(0x11),
        0x32 => Some(0x10),
        0x33 => Some(0x36),
        0x34 => Some(0x37),
        0x35 => Some(0x38),
        0x36 => Some(0xE5),
        0x37 => Some(0x55),
        0x38 => Some(0xE2),
        0x39 => Some(0x2C),
        0x3A => Some(0x39),
        0x3B..=0x44 => Some(u16::from(scancode) - 1),
        0x45 => Some(0x53),
        0x46 => Some(0x47),
        0x47 => Some(0x5F),
        0x48 => Some(0x60),
        0x49 => Some(0x61),
        0x4A => Some(0x56),
        0x4B => Some(0x5C),
        0x4C => Some(0x5D),
        0x4D => Some(0x5E),
        0x4E => Some(0x57),
        0x4F => Some(0x59),
        0x50 => Some(0x5A),
        0x51 => Some(0x5B),
        0x52 => Some(0x62),
        0x53 => Some(0x63),
        0x56 => Some(0x64),
        0x57 => Some(0x44),
        0x58 => Some(0x45),
        _ => None,
    }
}

const _: () = {
    assert!(core::mem::size_of::<Event>() == 32);
    assert!(core::mem::align_of::<Event>() == 8);
    assert!(core::mem::offset_of!(Event, kind) == 0);
    assert!(core::mem::offset_of!(Event, usage) == 2);
    assert!(core::mem::offset_of!(Event, modifiers) == 4);
    assert!(core::mem::offset_of!(Event, flags) == 8);
    assert!(core::mem::offset_of!(Event, sequence) == 16);
    assert!(core::mem::offset_of!(Event, tick) == 24);
};
const _: () = {
    assert!(core::mem::size_of::<Snapshot>() == 32);
    assert!(core::mem::align_of::<Snapshot>() == 8);
    assert!(core::mem::offset_of!(Snapshot, pending) == 0);
    assert!(core::mem::offset_of!(Snapshot, high_water) == 4);
    assert!(core::mem::offset_of!(Snapshot, transitions) == 8);
    assert!(core::mem::offset_of!(Snapshot, duplicate_transitions) == 16);
    assert!(core::mem::offset_of!(Snapshot, overflow_resets) == 24);
};
const _: () = {
    assert!(core::mem::size_of::<Queue>() == 8536);
    assert!(core::mem::align_of::<Queue>() == 8);
    assert!(core::mem::offset_of!(Queue, items) == 0);
    assert!(core::mem::offset_of!(Queue, held) == 8224);
    assert!(core::mem::offset_of!(Queue, head) == 8480);
    assert!(core::mem::offset_of!(Queue, tail) == 8488);
    assert!(core::mem::offset_of!(Queue, next_sequence) == 8496);
    assert!(core::mem::offset_of!(Queue, high_water) == 8504);
    assert!(core::mem::offset_of!(Queue, transitions) == 8512);
    assert!(core::mem::offset_of!(Queue, duplicate_transitions) == 8520);
    assert!(core::mem::offset_of!(Queue, overflow_resets) == 8528);
};
