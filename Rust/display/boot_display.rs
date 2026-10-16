//! Sole early framebuffer/console bootstrap and resident pixel-shadow storage.
//! Display admission remains at the existing display owner; typed C peers keep
//! its still-Zig callback ABI without retaining another bootstrap state.
use super::{
    console::Console,
    console_scroll_buffer,
    framebuffer::{self, Framebuffer},
};
use crate::memory::heap;
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ConsoleMetrics {
    pub cols: u32,
    pub rows: u32,
    pub scale: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct State {
    pub console_metrics: ConsoleMetrics,
}
struct Storage<T>(UnsafeCell<MaybeUninit<T>>);
// SAFETY: linear BSP startup is the sole initializer. Output owners retain
// exact firmware admission; passive bootstrap state is immutable after READY.
unsafe impl<T> Sync for Storage<T> {}
static FRAME: Storage<Framebuffer> = Storage(UnsafeCell::new(MaybeUninit::uninit()));
static CONSOLE: Storage<Console> = Storage(UnsafeCell::new(MaybeUninit::uninit()));
static STATE: Storage<State> = Storage(UnsafeCell::new(MaybeUninit::uninit()));
static READY: AtomicBool = AtomicBool::new(false);
unsafe extern "C" {
    fn r4os_zig_boot_display_register(frame: *mut Framebuffer);
    fn r4os_zig_boot_display_outputs(frame: *mut Framebuffer);
    fn r4os_zig_boot_display_surface();
    fn r4os_zig_boot_display_sink(console: *mut Console);
}
fn frame() -> *mut Framebuffer {
    FRAME.0.get().cast()
}
fn console() -> *mut Console {
    CONSOLE.0.get().cast()
}
pub fn get() -> Option<State> {
    if READY.load(Ordering::Acquire) {
        Some(unsafe { STATE.0.get().cast::<State>().read() })
    } else {
        None
    }
}
pub fn init() -> Option<State> {
    if let Some(state) = get() {
        return Some(state);
    }
    let source = framebuffer::boot_framebuffer()?;
    unsafe {
        frame().write(source);
        console().write(Console::new(frame()));
        r4os_zig_boot_display_register(frame());
        r4os_zig_boot_display_outputs(frame());
        r4os_zig_boot_display_surface();
        (*console()).clear();
        let _ = super::bootscreen::render(frame());
        (*console()).invalidate_external();
        r4os_zig_boot_display_sink(console());
        let state = State {
            console_metrics: ConsoleMetrics {
                cols: ptr::addr_of!((*console()).cols).read(),
                rows: ptr::addr_of!((*console()).rows).read(),
                scale: ptr::addr_of!((*console()).scale).read(),
            },
        };
        STATE.0.get().cast::<State>().write(state);
        READY.store(true, Ordering::Release);
        Some(state)
    }
}
pub fn prepare_pixel_backing(budget: u64) -> bool {
    if !READY.load(Ordering::Acquire) {
        return false;
    }
    if unsafe { ptr::addr_of!((*console()).has_pixel_backing).read() } {
        return true;
    }
    let f = unsafe { &*frame() };
    let Some(bytes) = console_scroll_buffer::required_pixel_bytes(f.width, f.height) else {
        return false;
    };
    if bytes as u64 > budget {
        return false;
    }
    let memory = heap::r4os_rust_heap_alloc(bytes, core::mem::align_of::<u32>());
    if memory.is_null() {
        return false;
    }
    if unsafe { (*console()).attach_pixels(memory.cast(), bytes / 4) } {
        return true;
    }
    let _ = unsafe { heap::r4os_rust_heap_free(memory, bytes) };
    false
}
pub fn invalidate_external() {
    if !READY.load(Ordering::Acquire) {
        return;
    }
    unsafe {
        (*console()).invalidate_external();
    }
}
/// # Safety
/// Caller owns the output record; only the original linear BSP may initialize.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_display_init(out: *mut State) -> bool {
    let Some(value) = init() else { return false };
    unsafe { out.write(value) }
    true
}
/// # Safety
/// Caller owns an aligned output for the copied immutable bootstrap snapshot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn r4os_rust_boot_display_get(out: *mut State) -> bool {
    let Some(value) = get() else { return false };
    unsafe { out.write(value) }
    true
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_display_prepare(budget: u64) -> bool {
    prepare_pixel_backing(budget)
}
#[unsafe(no_mangle)]
pub extern "C" fn r4os_rust_boot_display_invalidate() {
    invalidate_external();
}
const _: () = {
    assert!(core::mem::size_of::<ConsoleMetrics>() == 12);
    assert!(core::mem::align_of::<ConsoleMetrics>() == 4);
    assert!(core::mem::offset_of!(ConsoleMetrics, cols) == 0);
    assert!(core::mem::offset_of!(ConsoleMetrics, rows) == 4);
    assert!(core::mem::offset_of!(ConsoleMetrics, scale) == 8);
    assert!(core::mem::size_of::<State>() == 12);
    assert!(core::mem::align_of::<State>() == 4);
    assert!(core::mem::offset_of!(State, console_metrics) == 0);
};
