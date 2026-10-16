//! Fixed bootlog storage, guarded by the original rank-70 owner.
use super::owner::Guard;
use core::cell::UnsafeCell;
pub(crate) const CAPACITY: usize = 65536;
struct State {
    bytes: [u8; CAPACITY],
    cursor: usize,
    wrapped: bool,
    total: u64,
}
struct Storage(UnsafeCell<State>);
// SAFETY: every access to State is bounded and covered by the exact bootlog
// owner; no callback, wait, allocation or reference escapes that owner.
unsafe impl Sync for Storage {}
static LOG: Storage = Storage(UnsafeCell::new(State {
    bytes: [0; CAPACITY],
    cursor: 0,
    wrapped: false,
    total: 0,
}));
#[repr(C)]
pub struct Info {
    pub length: usize,
    pub flags: u32,
    pub total_written: u64,
    pub dropped_bytes: u64,
}
impl State {
    fn append(&mut self, byte: u8) {
        self.bytes[self.cursor] = byte;
        self.cursor += 1;
        if self.cursor == CAPACITY {
            self.cursor = 0;
            self.wrapped = true;
        }
        self.total = self.total.wrapping_add(1);
    }
    fn length(&self) -> usize {
        if self.wrapped { CAPACITY } else { self.cursor }
    }
    fn flags(&self) -> u32 {
        u32::from(self.wrapped)
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns a writable C-layout Info until return.
pub unsafe extern "C" fn r4os_rust_bootlog_info(output: *mut Info) {
    let _guard = Guard::acquire(0);
    let state = unsafe { &*LOG.0.get() };
    unsafe {
        output.write(Info {
            length: state.length(),
            flags: state.flags(),
            total_written: state.total,
            dropped_bytes: state.total.saturating_sub(CAPACITY as u64),
        })
    };
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller keeps len readable bytes resident until return; they don't alias LOG.
pub unsafe extern "C" fn r4os_rust_bootlog_write(data: *const u8, len: usize) {
    let _guard = Guard::acquire(0);
    let state = unsafe { &mut *LOG.0.get() };
    for index in 0..len {
        state.append(unsafe { data.add(index).read() });
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_bootlog_dec(mut value: u64) {
    let _guard = Guard::acquire(0);
    let state = unsafe { &mut *LOG.0.get() };
    let mut text = [0; 20];
    let mut len = 0;
    if value == 0 {
        state.append(b'0');
        return;
    }
    while value != 0 && len < text.len() {
        text[len] = b'0' + (value % 10) as u8;
        len += 1;
        value /= 10;
    }
    while len != 0 {
        len -= 1;
        state.append(text[len]);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_bootlog_hex(value: u64, width: usize) {
    let _guard = Guard::acquire(0);
    let state = unsafe { &mut *LOG.0.get() };
    for index in (0..width).rev() {
        state.append(b"0123456789ABCDEF"[((value >> (index * 4)) & 15) as usize]);
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller owns len writable bytes resident until return, outside the log ring.
pub unsafe extern "C" fn r4os_rust_bootlog_read(
    offset: usize,
    output: *mut u8,
    len: usize,
) -> usize {
    let _guard = Guard::acquire(0);
    let state = unsafe { &*LOG.0.get() };
    if offset >= state.length() {
        return 0;
    }
    let count = len.min(state.length() - offset);
    for index in 0..count {
        let source = if state.wrapped {
            (state.cursor + offset + index) % CAPACITY
        } else {
            offset + index
        };
        unsafe { output.add(index).write(state.bytes[source]) };
    }
    count
}

/// Synchronous bounded ring append from a retained native span.
pub fn text(bytes: &[u8]) {
    unsafe { r4os_rust_bootlog_write(bytes.as_ptr(), bytes.len()) }
}

/// Bounded original log-line formatting; raw owner names need not be UTF-8.
pub(crate) struct Line<const N: usize> {
    bytes: [u8; N],
    len: usize,
}
impl<const N: usize> Line<N> {
    pub(crate) fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
    pub(crate) fn append(&mut self, bytes: &[u8]) {
        let end = self.len.checked_add(bytes.len()).expect("Log line size");
        assert!(end <= N, "Original log line capacity");
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }
    pub(crate) fn emit(self) {
        text(&self.bytes[..self.len]);
    }
}
impl<const N: usize> core::fmt::Write for Line<N> {
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        self.append(text.as_bytes());
        Ok(())
    }
}
