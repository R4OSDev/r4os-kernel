//! Original bounded serial FIFO and saturating TX deadline; caller owns the
//! shared queue and serial runtime admission, no separate storage is added.
pub const MAX_PAYLOAD: usize = 256;
pub const CAPACITY: usize = 16;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Message {
    pub len: u16,
    pub data: [u8; MAX_PAYLOAD],
}
impl Message {
    pub const EMPTY: Self = Self {
        len: 0,
        data: [0; MAX_PAYLOAD],
    };
}
#[repr(C)]
pub struct MessageQueue {
    pub items: [Message; CAPACITY],
    pub head: usize,
    pub tail: usize,
    pub count: usize,
    pub high_water: usize,
    pub drops: u64,
}
impl MessageQueue {
    pub const EMPTY: Self = Self {
        items: [Message::EMPTY; CAPACITY],
        head: 0,
        tail: 0,
        count: 0,
        high_water: 0,
        drops: 0,
    };
    pub fn push(&mut self, payload: &[u8]) -> bool {
        if payload.len() > MAX_PAYLOAD {
            return false;
        }
        if self.count == CAPACITY {
            self.drops = self.drops.wrapping_add(1);
            return false;
        }
        self.items[self.head] = Message {
            len: payload.len() as u16,
            ..Message::EMPTY
        };
        self.items[self.head].data[..payload.len()].copy_from_slice(payload);
        self.head = (self.head + 1) % CAPACITY;
        self.count += 1;
        self.high_water = self.high_water.max(self.count);
        true
    }
    pub fn pop(&mut self) -> Option<Message> {
        if self.count == 0 {
            return None;
        }
        let value = self.items[self.tail];
        self.items[self.tail] = Message::EMPTY;
        self.tail = (self.tail + 1) % CAPACITY;
        self.count -= 1;
        Some(value)
    }
    pub fn clear(&mut self) {
        *self = Self::EMPTY;
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TxDeadline {
    pub start_tick: u64,
    pub timeout_ticks: u64,
}
impl TxDeadline {
    pub fn expired(self, now: u64) -> bool {
        self.timeout_ticks == 0 || now.saturating_sub(self.start_tick) >= self.timeout_ticks
    }
}
/// # Safety
/// Exclusive queue admission, readable payload and stable paired record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_serial_push(
    queue: *mut MessageQueue,
    bytes: *const u8,
    len: usize,
) -> bool {
    if len > MAX_PAYLOAD {
        return false;
    }
    let payload = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(bytes, len) }
    };
    unsafe { (&mut *queue).push(payload) }
}
/// # Safety
/// Exclusive queue admission and nonaliasing full message output.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_serial_pop(
    queue: *mut MessageQueue,
    out: *mut Message,
) -> bool {
    unsafe {
        out.write(Message::EMPTY);
        if let Some(value) = (&mut *queue).pop() {
            out.write(value);
            true
        } else {
            false
        }
    }
}
/// # Safety
/// Caller exclusively owns the stable resident queue.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_net_serial_clear(queue: *mut MessageQueue) {
    unsafe {
        (&mut *queue).clear();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_net_serial_expired(start: u64, timeout: u64, now: u64) -> bool {
    TxDeadline {
        start_tick: start,
        timeout_ticks: timeout,
    }
    .expired(now)
}

const _: () = {
    assert!(core::mem::size_of::<Message>() == 258);
    assert!(core::mem::align_of::<Message>() == 2);
    assert!(core::mem::offset_of!(Message, len) == 0);
    assert!(core::mem::offset_of!(Message, data) == 2);
};
const _: () = {
    assert!(core::mem::size_of::<MessageQueue>() == 4168);
    assert!(core::mem::align_of::<MessageQueue>() == 8);
    assert!(core::mem::offset_of!(MessageQueue, items) == 0);
    assert!(core::mem::offset_of!(MessageQueue, head) == 4128);
    assert!(core::mem::offset_of!(MessageQueue, tail) == 4136);
    assert!(core::mem::offset_of!(MessageQueue, count) == 4144);
    assert!(core::mem::offset_of!(MessageQueue, high_water) == 4152);
    assert!(core::mem::offset_of!(MessageQueue, drops) == 4160);
};
const _: () = {
    assert!(core::mem::size_of::<TxDeadline>() == 16);
    assert!(core::mem::align_of::<TxDeadline>() == 8);
    assert!(core::mem::offset_of!(TxDeadline, start_tick) == 0);
    assert!(core::mem::offset_of!(TxDeadline, timeout_ticks) == 8);
};
