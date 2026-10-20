//! General kernel output routing. Only short configuration copies use the
//! existing diagnostic owner; serial, hooks and firmware callbacks run after
//! its release. The console remains the original resident boot-console owner.
use super::owner::Guard;
use crate::display::{console::Console, firmware::Lease};
use core::{cell::UnsafeCell, ptr};

pub type SerialSink = unsafe extern "C" fn(u8);
pub type SerialBulkSink = unsafe extern "C" fn(*const u8, usize);
pub type SerialControlHook = extern "C" fn();
pub type OutputHook = extern "C" fn(u8) -> bool;
#[derive(Clone, Copy)]
struct State {
    serial: Option<SerialSink>,
    bulk: Option<SerialBulkSink>,
    flush: Option<SerialControlHook>,
    sync: Option<SerialControlHook>,
    console: *mut Console,
    hook: Option<OutputHook>,
    intercept_serial: bool,
}
struct Storage(UnsafeCell<State>);
// SAFETY: configuration is copied/mutated only under the existing diagnostic
// owner. No state reference or owner crosses callbacks or firmware admission.
unsafe impl Sync for Storage {}
static STATE: Storage = Storage(UnsafeCell::new(State {
    serial: None,
    bulk: None,
    flush: None,
    sync: None,
    console: ptr::null_mut(),
    hook: None,
    intercept_serial: false,
}));
fn snapshot() -> State {
    let _owner = Guard::acquire(0);
    unsafe { STATE.0.get().read() }
}
/// # Safety
/// Every installed callback retains its resident code and accepts the exact
/// synchronous byte/buffer/control contract of the initialized UART owner.
pub unsafe fn set_serial(
    serial: Option<SerialSink>,
    bulk: Option<SerialBulkSink>,
    flush: Option<SerialControlHook>,
    sync: Option<SerialControlHook>,
) {
    let _owner = Guard::acquire(0);
    let state = STATE.0.get();
    unsafe {
        (*state).serial = serial;
        (*state).bulk = bulk;
        (*state).flush = flush;
        (*state).sync = sync;
    }
}
/// # Safety
/// The exact boot console remains resident for the kernel lifetime. Replacing
/// routing never transfers, frees or revokes its framebuffer ownership.
pub unsafe fn set_console(console: *mut Console) {
    let _owner = Guard::acquire(0);
    unsafe { (*STATE.0.get()).console = console };
}
pub fn set_output_hook(hook: Option<OutputHook>, intercept_serial: bool) {
    let _owner = Guard::acquire(0);
    unsafe {
        (*STATE.0.get()).hook = hook;
        (*STATE.0.get()).intercept_serial = intercept_serial && hook.is_some();
    }
}
pub fn serial_flush() {
    if let Some(hook) = snapshot().flush {
        hook();
    }
}
pub fn serial_enter_sync() {
    if let Some(hook) = snapshot().sync {
        hook();
    }
}
fn serial_write(state: State, text: &[u8]) {
    if let Some(write) = state.bulk {
        unsafe { write(text.as_ptr(), text.len()) };
    } else if let Some(putc) = state.serial {
        for &byte in text {
            unsafe { putc(byte) };
        }
    }
}
pub fn serial_putc_raw(byte: u8) {
    if let Some(putc) = snapshot().serial {
        unsafe { putc(byte) };
    }
}
pub fn serial_write_raw(text: &[u8]) {
    serial_write(snapshot(), text);
}
fn console_byte(console: *mut Console, byte: u8) {
    if !console.is_null()
        && let Some(_lease) = Lease::acquire()
    {
        unsafe { (*console).putc(byte) };
    }
}
fn console_write(console: *mut Console, text: &[u8]) {
    if !console.is_null()
        && let Some(_lease) = Lease::acquire()
    {
        unsafe { (*console).puts_raw(text.as_ptr(), text.len()) };
    }
}
pub fn console_putc_raw(byte: u8) {
    console_byte(snapshot().console, byte);
}
pub fn console_scalar_raw(scalar: u32) {
    let console = snapshot().console;
    if !console.is_null()
        && let Some(_lease) = Lease::acquire()
    {
        unsafe {
            (*console).put_scalar(scalar);
        }
    }
}
pub fn putc(byte: u8) {
    let state = snapshot();
    if state.intercept_serial
        && let Some(hook) = state.hook
        && hook(byte)
    {
        return;
    }
    if let Some(putc) = state.serial {
        unsafe { putc(byte) };
    }
    if !state.intercept_serial
        && let Some(hook) = state.hook
        && hook(byte)
    {
        return;
    }
    console_byte(state.console, byte);
}
pub fn puts(text: &[u8]) {
    let state = snapshot();
    if state.intercept_serial {
        for &byte in text {
            putc(byte);
        }
        return;
    }
    serial_write(state, text);
    if let Some(hook) = state.hook {
        for &byte in text {
            if !hook(byte) {
                console_byte(state.console, byte);
            }
        }
        return;
    }
    console_write(state.console, text);
}
pub fn hex(value: u64, digits: u8) {
    for index in (0..digits).rev() {
        let shift = u32::from(index) * 4;
        assert!(shift < 64);
        putc(b"0123456789ABCDEF"[((value >> shift) & 15) as usize]);
    }
}
pub fn dec(mut value: u64) {
    if value == 0 {
        putc(b'0');
        return;
    }
    let mut bytes = [0; 20];
    let mut cursor = bytes.len();
    while value != 0 {
        cursor -= 1;
        bytes[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    puts(&bytes[cursor..]);
}
pub fn clear_console() {
    console_control(4, 0, 0, 0, 0);
}
fn console_control(op: u8, a: u32, b: u32, c: u32, d: u32) -> u32 {
    let console = snapshot().console;
    if console.is_null() {
        return 0;
    }
    if op == 10 {
        return unsafe { (*console).text_cols() };
    }
    if op == 11 {
        return unsafe { (*console).text_rows() };
    }
    let Some(_lease) = Lease::acquire() else {
        return 0;
    };
    unsafe {
        match op {
            4 => (*console).clear(),
            5 => (*console).set_margins(a, b, c, d),
            6 => (*console).set_colors(a, b),
            7 => (*console).set_font_scale(a),
            8 => (*console).set_cursor(a, b),
            9 => (*console).clear_framed(a, b),
            _ => unreachable!(),
        }
    }
    0
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_log_hook(hook: Option<OutputHook>, intercept_serial: bool) {
    set_output_hook(hook, intercept_serial);
}
/// # Safety
/// Input retains len readable resident bytes for the synchronous operation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_log_write(mode: u8, text: *const u8, len: usize) {
    let bytes = if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(text, len) }
    };
    match mode {
        0 => puts(bytes),
        1 => serial_write_raw(bytes),
        _ => unreachable!(),
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_log_number(value: u64, digits: u8, is_hex: bool) {
    if is_hex {
        hex(value, digits);
    } else {
        dec(value);
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_log_control(op: u8, a: u32, b: u32, c: u32, d: u32) -> u32 {
    match op {
        0 => serial_flush(),
        1 => serial_enter_sync(),
        2 => serial_putc_raw(a as u8),
        3 => console_putc_raw(a as u8),
        4..=11 => return console_control(op, a, b, c, d),
        12 => putc(a as u8),
        _ => unreachable!(),
    }
    0
}
