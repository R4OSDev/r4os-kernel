//! Sole PS/2/USB mouse state and cursor projection. Poll callbacks and display
//! bounds are sampled outside short input metadata admission; wakes follow it.
use super::metadata::MetadataLock;
use crate::{
    arch::{io, pic},
    service::desktop_events,
};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU64, Ordering},
};
pub const IRQ: u8 = 12;
pub const CURSOR_W: usize = 12;
pub const CURSOR_H: usize = 16;
pub type PollHook = unsafe extern "C" fn();
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct State {
    pub x: i32,
    pub y: i32,
    pub dx: i32,
    pub dy: i32,
    pub wheel: i32,
    pub buttons: u8,
    pub packets: u64,
    pub present: bool,
}
impl State {
    const EMPTY: Self = Self {
        x: 0,
        y: 0,
        dx: 0,
        dy: 0,
        wheel: 0,
        buttons: 0,
        packets: 0,
        present: false,
    };
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Bounds {
    pub width: u32,
    pub height: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CursorOverlay {
    pub visible: bool,
    pub x: i32,
    pub y: i32,
    pub max_x: i32,
    pub max_y: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stats {
    pub irq_count: u64,
    pub bytes_received: u64,
    pub packets: u64,
    pub usb_packets: u64,
    pub x: i32,
    pub y: i32,
    pub dx: i32,
    pub dy: i32,
    pub wheel: i32,
    pub buttons: u8,
    pub present: bool,
}
struct Runtime {
    state: State,
    packet: [u8; 3],
    packet_index: usize,
    button_press_latch: u8,
    irq_count: u64,
    bytes_received: u64,
    usb_packets: u64,
    poll_hook: Option<PollHook>,
    cursor_visible: bool,
    cursor_x: i32,
    cursor_y: i32,
}
struct Storage(UnsafeCell<Runtime>);
// SAFETY: all runtime access holds LOCK; atomic motion totals are independent.
unsafe impl Sync for Storage {}
static CURRENT: Storage = Storage(UnsafeCell::new(Runtime {
    state: State::EMPTY,
    packet: [0; 3],
    packet_index: 0,
    button_press_latch: 0,
    irq_count: 0,
    bytes_received: 0,
    usb_packets: 0,
    poll_hook: None,
    cursor_visible: false,
    cursor_x: 0,
    cursor_y: 0,
}));
static LOCK: MetadataLock = MetadataLock::new();
static MOTION_TOTALS: AtomicU64 = AtomicU64::new(0);
fn state<T>(f: impl FnOnce(&mut Runtime) -> T) -> T {
    let _guard = LOCK.acquire();
    f(unsafe { &mut *CURRENT.0.get() })
}

fn limits() -> (i32, i32) {
    let mut b = Bounds::default();
    unsafe { input_mouse_bounds(&mut b) };
    (
        b.width.saturating_sub(1).min(i32::MAX as u32) as i32,
        b.height.saturating_sub(1).min(i32::MAX as u32) as i32,
    )
}
fn bounded(mut value: State, (x, y): (i32, i32)) -> State {
    value.x = value.x.clamp(0, x);
    value.y = value.y.clamp(0, y);
    value
}
fn add_motion(dx: i32, dy: i32) {
    let mut previous = MOTION_TOTALS.load(Ordering::Relaxed);
    loop {
        let x = (previous as u32).wrapping_add(dx as u32);
        let y = ((previous >> 32) as u32).wrapping_add(dy as u32);
        let next = u64::from(x) | (u64::from(y) << 32);
        match MOTION_TOTALS.compare_exchange_weak(
            previous,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(v) => previous = v,
        }
    }
}
pub fn motion_totals() -> u64 {
    MOTION_TOTALS.load(Ordering::Relaxed)
}
fn refresh(s: &mut Runtime) {
    if s.cursor_visible {
        s.cursor_x = s.state.x;
        s.cursor_y = s.state.y;
    }
}
fn poll_usb() {
    let hook = state(|s| s.poll_hook);
    if let Some(hook) = hook {
        unsafe { hook() }
    }
}
pub fn init(bounds: Bounds) -> bool {
    let x = i32::try_from(bounds.width.saturating_sub(1)).expect("Original mouse init width");
    let y = i32::try_from(bounds.height.saturating_sub(1)).expect("Original mouse init height");
    state(|s| {
        s.state.x = x / 2;
        s.state.y = y / 2;
        s.cursor_x = s.state.x;
        s.cursor_y = s.state.y;
    });
    drain_output();
    write_command(0xa8);
    write_command(0x20);
    let Some(mut config) = read_data_wait() else {
        return false;
    };
    config |= 2;
    config &= !0x20;
    write_command(0x60);
    write_data(config);
    if !send_command(0xf6) || !send_command(0xf4) {
        return false;
    }
    state(|s| s.state.present = true);
    pic::unmask(IRQ);
    true
}
pub fn disable() {
    state(|s| s.cursor_visible = false);
    pic::mask(IRQ);
    state(|s| {
        s.state.present = false;
        s.packet_index = 0;
        s.button_press_latch = 0;
    });
}
pub fn note_irq() {
    state(|s| s.irq_count = s.irq_count.wrapping_add(1));
}
pub fn controller_byte(byte: u8) {
    let packet = state(|s| {
        s.bytes_received = s.bytes_received.wrapping_add(1);
        if s.packet_index == 0 && byte & 8 == 0 {
            return None;
        }
        s.packet[s.packet_index] = byte;
        s.packet_index += 1;
        if s.packet_index < 3 {
            return None;
        }
        s.packet_index = 0;
        Some(s.packet)
    });
    if let Some(p) = packet {
        handle_packet(p);
    }
}
fn sign_extend(v: u8, negative: bool) -> i32 {
    i32::from(v) - if negative { 256 } else { 0 }
}
fn handle_packet(p: [u8; 3]) {
    if p[0] & 0xc0 != 0 {
        return;
    }
    let dx = sign_extend(p[1], p[0] & 0x10 != 0);
    let dy = sign_extend(p[2], p[0] & 0x20 != 0);
    let (x, y) = limits();
    state(|s| {
        s.state.dx = dx;
        s.state.dy = -dy;
        add_motion(dx, -dy);
        s.state.x = s.state.x.saturating_add(dx).clamp(0, x);
        s.state.y = s.state.y.saturating_sub(dy).clamp(0, y);
        let buttons = p[0] & 7;
        s.button_press_latch |= buttons & !s.state.buttons;
        s.state.buttons = buttons;
        s.state.packets += 1;
        refresh(s);
    });
    desktop_events::r4os_rust_desktop_signal();
}
pub fn snapshot() -> State {
    poll_usb();
    let l = limits();
    bounded(state(|s| s.state), l)
}
pub fn stats() -> Stats {
    poll_usb();
    let l = limits();
    state(|s| {
        let current = bounded(s.state, l);
        Stats {
            irq_count: s.irq_count,
            bytes_received: s.bytes_received,
            packets: s.state.packets,
            usb_packets: s.usb_packets,
            x: current.x,
            y: current.y,
            dx: s.state.dx,
            dy: s.state.dy,
            wheel: s.state.wheel,
            buttons: s.state.buttons,
            present: s.state.present,
        }
    })
}
pub fn snapshot_for_api() -> State {
    poll_usb();
    let l = limits();
    state(|s| {
        let mut out = bounded(s.state, l);
        let latched = s.button_press_latch;
        if latched != 0 {
            out.buttons |= latched;
            s.button_press_latch = 0;
        }
        s.state.wheel = 0;
        out
    })
}
pub fn inject(dx: i32, dy: i32, buttons: u8, wheel: i32) {
    let (x, y) = limits();
    state(|s| {
        s.state.dx = dx;
        s.state.dy = dy;
        add_motion(dx, dy);
        s.state.wheel += wheel;
        s.state.x = s.state.x.saturating_add(dx).clamp(0, x);
        s.state.y = s.state.y.saturating_add(dy).clamp(0, y);
        let new_buttons = buttons & 7;
        s.button_press_latch |= new_buttons & !s.state.buttons;
        s.state.buttons = new_buttons;
        s.state.present = true;
        s.state.packets += 1;
        s.usb_packets += 1;
        refresh(s);
    });
    desktop_events::r4os_rust_desktop_signal();
}
pub fn cursor_overlay() -> CursorOverlay {
    let (x, y) = limits();
    state(|s| CursorOverlay {
        visible: s.cursor_visible,
        x: s.cursor_x.clamp(0, x),
        y: s.cursor_y.clamp(0, y),
        max_x: x,
        max_y: y,
    })
}
pub fn cursor_mask(x: usize, y: usize) -> u8 {
    if x == 0 && y < 14 {
        return 2;
    }
    if x == 1 && y < 12 {
        return 1;
    }
    if x == y && y < 11 {
        return 2;
    }
    if x + 1 == y && y < 12 {
        return 1;
    }
    if y == 11 && x < 8 {
        return 2;
    }
    if y == 12 && x < 7 {
        return 1;
    }
    if x == 4 && (10..16).contains(&y) {
        return 2;
    }
    if x == 5 && (11..16).contains(&y) {
        return 1;
    }
    if x == 6 && (13..16).contains(&y) {
        return 2;
    }
    0
}
fn wait_input_clear() {
    let mut guard = 0;
    while unsafe { io::inb(0x64) } & 2 != 0 && guard < 100_000 {
        unsafe { io::outb(0x80, 0) }
        guard += 1;
    }
}
fn write_command(command: u8) {
    wait_input_clear();
    unsafe { io::outb(0x64, command) }
}
fn write_data(data: u8) {
    wait_input_clear();
    unsafe { io::outb(0x60, data) }
}
fn read_data_wait() -> Option<u8> {
    for _ in 0..100_000 {
        if unsafe { io::inb(0x64) } & 1 != 0 {
            return Some(unsafe { io::inb(0x60) });
        }
    }
    None
}
fn send_command(command: u8) -> bool {
    write_command(0xd4);
    write_data(command);
    read_data_wait().unwrap_or(0) == 0xfa
}
fn drain_output() {
    let mut guard = 0;
    while unsafe { io::inb(0x64) } & 1 != 0 && guard < 256 {
        let _ = unsafe { io::inb(0x60) };
        guard += 1;
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains initialized readable bounds or an aligned exclusive output for the complete call; hardware initialization keeps the original boot admission.
pub unsafe extern "C" fn r4os_rust_input_mouse_init(bounds: *const Bounds) -> bool {
    init(unsafe { bounds.read() })
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_disable() {
    disable();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_irq() {
    note_irq();
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_byte(value: u8) {
    controller_byte(value);
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains initialized readable bounds or an aligned exclusive output for the complete call; hardware initialization keeps the original boot admission.
pub unsafe extern "C" fn r4os_rust_input_mouse_snapshot(out: *mut State, api: bool) {
    let value = if api { snapshot_for_api() } else { snapshot() };
    unsafe { out.write(value) }
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains initialized readable bounds or an aligned exclusive output for the complete call; hardware initialization keeps the original boot admission.
pub unsafe extern "C" fn r4os_rust_input_mouse_stats(out: *mut Stats) {
    let value = stats();
    unsafe { out.write(value) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_poll_hook(hook: Option<PollHook>) {
    state(|s| s.poll_hook = hook);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_inject(dx: i32, dy: i32, buttons: u8, wheel: i32) {
    inject(dx, dy, buttons, wheel);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_enable_cursor() {
    state(|s| {
        s.cursor_visible = true;
        s.cursor_x = s.state.x;
        s.cursor_y = s.state.y;
    });
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_disable_cursor() {
    state(|s| s.cursor_visible = false);
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_refresh_cursor() {
    state(refresh);
}
#[unsafe(no_mangle)]
/// # Safety
/// Caller retains initialized readable bounds or an aligned exclusive output for the complete call; hardware initialization keeps the original boot admission.
pub unsafe extern "C" fn r4os_rust_input_mouse_cursor(out: *mut CursorOverlay) {
    let value = cursor_overlay();
    unsafe { out.write(value) }
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_mask(x: usize, y: usize) -> u8 {
    cursor_mask(x, y)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_input_mouse_motion() -> u64 {
    motion_totals()
}

const _: () = {
    assert!(core::mem::size_of::<State>() == 40);
    assert!(core::mem::align_of::<State>() == 8);
    assert!(core::mem::offset_of!(State, x) == 0);
    assert!(core::mem::offset_of!(State, y) == 4);
    assert!(core::mem::offset_of!(State, dx) == 8);
    assert!(core::mem::offset_of!(State, dy) == 12);
    assert!(core::mem::offset_of!(State, wheel) == 16);
    assert!(core::mem::offset_of!(State, buttons) == 20);
    assert!(core::mem::offset_of!(State, packets) == 24);
    assert!(core::mem::offset_of!(State, present) == 32);
};
const _: () = {
    assert!(core::mem::size_of::<CursorOverlay>() == 20);
    assert!(core::mem::align_of::<CursorOverlay>() == 4);
    assert!(core::mem::offset_of!(CursorOverlay, visible) == 0);
    assert!(core::mem::offset_of!(CursorOverlay, x) == 4);
    assert!(core::mem::offset_of!(CursorOverlay, y) == 8);
    assert!(core::mem::offset_of!(CursorOverlay, max_x) == 12);
    assert!(core::mem::offset_of!(CursorOverlay, max_y) == 16);
};
const _: () = {
    assert!(core::mem::size_of::<Bounds>() == 8);
    assert!(core::mem::align_of::<Bounds>() == 4);
    assert!(core::mem::offset_of!(Bounds, width) == 0);
    assert!(core::mem::offset_of!(Bounds, height) == 4);
};
const _: () = {
    assert!(core::mem::size_of::<Stats>() == 56);
    assert!(core::mem::align_of::<Stats>() == 8);
    assert!(core::mem::offset_of!(Stats, irq_count) == 0);
    assert!(core::mem::offset_of!(Stats, bytes_received) == 8);
    assert!(core::mem::offset_of!(Stats, packets) == 16);
    assert!(core::mem::offset_of!(Stats, usb_packets) == 24);
    assert!(core::mem::offset_of!(Stats, x) == 32);
    assert!(core::mem::offset_of!(Stats, y) == 36);
    assert!(core::mem::offset_of!(Stats, dx) == 40);
    assert!(core::mem::offset_of!(Stats, dy) == 44);
    assert!(core::mem::offset_of!(Stats, wheel) == 48);
    assert!(core::mem::offset_of!(Stats, buttons) == 52);
    assert!(core::mem::offset_of!(Stats, present) == 53);
};

unsafe fn input_mouse_bounds(out: *mut Bounds) {
    let b = crate::display::surface_pipeline::pixel_bounds();
    unsafe {
        out.write(Bounds {
            width: b as u32,
            height: (b >> 32) as u32,
        });
    }
}
