//! Sole early framebuffer/console bootstrap and resident pixel-shadow storage.
//! Display admission and log sink binding retain the same native owner.
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
        super::manager::register_boot_backend(boot_target(frame()));
        super::outputs::init_boot(&frame().read());
        let stats = super::manager::stats();
        let target = if stats.registered {
            super::surface_pipeline::Target {
                width: u64::from(stats.mode.width),
                height: u64::from(stats.mode.height),
                pitch: u64::from(stats.mode.pitch),
                bpp: stats.mode.bpp,
            }
        } else {
            super::surface_pipeline::Target::default()
        };
        super::surface_pipeline::init_target(target);
        (*console()).clear();
        let _ = super::bootscreen::render(frame());
        (*console()).invalidate_external();
        crate::boot::log_route::set_console(console());
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

unsafe fn boot_target(frame: *mut Framebuffer) -> super::core_types::Target {
    use super::core_types::{Mapping, Mode, Target, Text};
    let f = unsafe { frame.read() };
    let mut flags = 1 | 2 | 4 | 16 | 32;
    if framebuffer::supports_rgb32(&f) {
        flags |= 8 | 64 | 128
    }
    Target {
        name: Text::literal(b"bootfb"),
        kind: super::manager::BOOTFB,
        flags,
        mode: Mode {
            width: u32::try_from(f.width).expect("boot width"),
            height: u32::try_from(f.height).expect("boot height"),
            pitch: u32::try_from(f.pitch).expect("boot pitch"),
            bpp: f.bpp,
            memory_model: f.memory_model,
            red_mask_size: f.red_mask_size,
            red_mask_shift: f.red_mask_shift,
            green_mask_size: f.green_mask_size,
            green_mask_shift: f.green_mask_shift,
            blue_mask_size: f.blue_mask_size,
            blue_mask_shift: f.blue_mask_shift,
        },
        mapping: Mapping {
            kind: 1,
            cache_policy: 1,
            virt_base: f.address.addr() as u64,
            byte_len: f.pitch * f.height,
            volatile_cpu_writes: true,
        },
        framebuffer: frame,
    }
}
